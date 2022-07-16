use ash::vk;
use ash::vk::DependencyFlags;
use crossbeam_channel::{Receiver, Sender};
use log::{error, info, log, Level};
use nalgebra::{Matrix4, Perspective3};
use obj::{load_obj, Obj};
use once_cell::sync::Lazy;
use parking_lot::{Condvar, Mutex};
use smallvec::SmallVec;
use std::default::Default;
use std::error::Error;
use std::ffi::CStr;
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::{Arc, Barrier};
use std::thread::JoinHandle;
use vk_mem::Allocator;
use anyhow::Result;

use engine::filesystem::DIRS;

use crate::vulkan::engine::alloc::{GpuObject, Image};
use crate::vulkan::engine::init::create_depth_image;
use crate::vulkan::engine::pipeline::{cleanup_cache, create_pipeline};
use crate::vulkan::engine::swapchain::Swapchain;
use crate::vulkan::mesh::Vertex;
use crate::vulkan::texture::Texture;
use crate::{Camera, cull_test, Material, Mesh, RenderingEngine};

pub(crate) mod alloc;
mod init;
mod pipeline;
mod swapchain;

const FRAMES_IN_FLIGHT: usize = 2;

pub struct Engine {
    frame_count: u64,
    _entry: Box<ash::Entry>,
    instance: Box<ash::Instance>,
    physical_device: vk::PhysicalDevice,
    device: Arc<ash::Device>,
    surface_loader: Box<ash::extensions::khr::Surface>,
    #[cfg(feature = "validation-layers")]
    debug_messenger: (
        Box<ash::extensions::ext::DebugUtils>,
        vk::DebugUtilsMessengerEXT,
    ),
    surface: vk::SurfaceKHR,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    surface_format: vk::SurfaceFormatKHR,
    swapchain: ManuallyDrop<Swapchain>,
    allocator: Arc<Allocator>,
    frames: [Frame; FRAMES_IN_FLIGHT],
    render_channels: SmallVec<[Sender<RenderCommand>; 12]>,
    render_thread_handles: SmallVec<[JoinHandle<()>; 12]>,
    render_barrier: Arc<Barrier>,
    present_channel: ManuallyDrop<Sender<PresentData>>,
    present_thread_handle: ManuallyDrop<JoinHandle<()>>,
    last_mesh: *const Mesh,
    last_material: *const Material,
    current_thread: usize,
    utility_pool: vk::CommandPool,
    global_descriptor_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    depth_format: vk::Format,
    depth_image: ManuallyDrop<Image>,
    depth_view: vk::ImageView,
    queue_families: [u32; 2],
    resolution: [u32; 2],
    vsync: bool,
}

#[derive(Debug)]
struct Frame {
    primary_buffer: vk::CommandBuffer,
    primary_pool: vk::CommandPool,
    secondary_buffers: SmallVec<[vk::CommandBuffer; 12]>,
    secondary_pools: SmallVec<[vk::CommandPool; 12]>,
    fence: vk::Fence,
    graphics_semaphore: vk::Semaphore,
    present_semaphore: vk::Semaphore,
    ubo: ManuallyDrop<GpuObject<Ubo>>,
    global_descriptor: vk::DescriptorSet,
    sync_data: Arc<(Mutex<RenderResult>, Condvar)>,
}

#[derive(Eq, PartialEq, Debug)]
enum RenderResult {
    NotDone,
    Ok,
    OutOfDate,
}

#[derive(Debug)]
struct Ubo {
    view: Matrix4<f32>,
    projection: Matrix4<f32>,
    orthographic: Matrix4<f32>
}

enum RenderCommand {
    Begin(
        vk::CommandBuffer,
        Matrix4<f32>,
        Perspective3<f32>,
        vk::DescriptorSet,
        vk::Format,
        vk::Format,
    ),
    Render(Arc<Mesh>, Arc<Material>, Matrix4<f32>),
    End,
}

struct PresentData {
    render_semaphore: vk::Semaphore,
    present_semaphore: vk::Semaphore,
    cmd: vk::CommandBuffer,
    swapchain: vk::SwapchainKHR,
    swapchain_loader: Arc<ash::extensions::khr::Swapchain>,
    image_index: u32,
    signal_fence: vk::Fence,
    sync_data: Arc<(Mutex<RenderResult>, Condvar)>,
}

/// Converts opengl to vulkan coordinate system
#[rustfmt::skip]
static COORDINATE_CORRECTION: Lazy<Matrix4<f32>> = Lazy::new(|| {
    Matrix4::from_row_slice(&[
        1f32, 0f32, 0f32, 0f32,
        0f32, -1f32, 0f32, 0f32,
        0f32, 0f32, 0.5f32, 0.5f32,
        0f32, 0f32, 0f32, 1f32,
    ])
});

impl RenderingEngine for Engine {
    fn begin_rendering(&mut self, camera: &Camera) {
        let proj = *COORDINATE_CORRECTION * camera.projection.to_homogeneous();
        let frame = &mut self.frames[self.frame_count as usize % FRAMES_IN_FLIGHT];
        let fences = [frame.fence];
        unsafe {
            if let Err(err) = self.device.wait_for_fences(&fences, true, u64::MAX) {
                error!("Error waiting on fence: {err}");
            }
            let suboptimal = {
                let mut lock = frame.sync_data.0.lock();
                frame
                    .sync_data
                    .1
                    .wait_while(&mut lock, |e| *e == RenderResult::NotDone);
                if *lock == RenderResult::OutOfDate {
                    *lock = RenderResult::Ok;
                    true
                } else {
                    *lock = RenderResult::NotDone;
                    false
                }
            };
            if suboptimal
                || match self.swapchain.next(frame.present_semaphore) {
                    Ok(val) => val,
                    Err(e)
                        if e == vk::Result::SUBOPTIMAL_KHR
                            || e == vk::Result::ERROR_OUT_OF_DATE_KHR =>
                    {
                        true
                    }
                    Err(e) => panic!("Failed to acquire swapchain image: {e:?}"),
                }
            {
                self.device.device_wait_idle().unwrap();
                let old = ManuallyDrop::take(&mut self.swapchain);
                self.swapchain = ManuallyDrop::new(
                    Swapchain::new(
                        &self.instance,
                        self.device.clone(),
                        self.physical_device,
                        self.surface,
                        &self.surface_loader,
                        &self.queue_families,
                        self.surface_format.format,
                        self.vsync,
                        &self.resolution,
                        Some(&old),
                    )
                    .expect("Failed to recreate swapchain"),
                );
                ManuallyDrop::drop(&mut self.depth_image);
                self.device.destroy_image_view(self.depth_view, None);
                let (image, depth_view) = create_depth_image(
                    &self.device,
                    self.depth_format,
                    self.swapchain.extent,
                    self.allocator.clone(),
                )
                .unwrap();
                self.depth_image = ManuallyDrop::new(image);
                self.depth_view = depth_view;
                info!(
                    "Swapchain resized to {}x{}",
                    self.swapchain.extent.width, self.swapchain.extent.height
                );
                self.begin_rendering(camera);
                return;
            }
            self.device.reset_fences(&fences).unwrap();
            frame.ubo.view = camera.view.to_homogeneous();
            frame.ubo.projection = proj;
            frame.ubo.orthographic = *COORDINATE_CORRECTION * camera.orthographic.to_homogeneous();
            self.device
                .reset_command_pool(frame.primary_pool, vk::CommandPoolResetFlags::empty())
                .unwrap();
            for pool in &frame.secondary_pools {
                self.device
                    .reset_command_pool(*pool, vk::CommandPoolResetFlags::empty())
                    .unwrap();
            }
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(frame.primary_buffer, &begin_info)
                .unwrap();

            pre_image_transition(
                &self.device,
                frame.primary_buffer,
                self.swapchain.get_current_image(),
                **self.depth_image,
            );

            begin(
                self.swapchain.get_current_image_view(),
                self.depth_view,
                self.swapchain.extent,
                frame.primary_buffer,
                &self.device,
            );
            for (index, channel) in self.render_channels.iter().enumerate() {
                channel
                    .send(RenderCommand::Begin(
                        frame.secondary_buffers[index],
                        camera.view.to_homogeneous(),
                        camera.projection,
                        frame.global_descriptor,
                        self.surface_format.format,
                        self.depth_format,
                    ))
                    .unwrap();
            }
        }
    }

    fn render(&mut self, mesh: &Arc<Mesh>, material: &Arc<Material>, transform: Matrix4<f32>) {
        if !(std::ptr::eq(mesh.as_ref(), self.last_mesh)
            && std::ptr::eq(material.as_ref(), self.last_material))
        {
            self.current_thread = (self.current_thread + 1) % self.render_channels.len();
            self.last_mesh = mesh.as_ref();
            self.last_material = material.as_ref();
        }
        let channel = &self.render_channels[self.current_thread];
        channel
            .send(RenderCommand::Render(
                mesh.clone(),
                material.clone(),
                transform,
            ))
            .expect("Failed to send render command");
    }

    fn end_rendering(&mut self) {
        for channel in &self.render_channels {
            channel.send(RenderCommand::End).unwrap();
        }
        self.render_barrier.wait();
        let frame = &self.frames[self.frame_count as usize % FRAMES_IN_FLIGHT];

        let image_barrier = [vk::ImageMemoryBarrier::builder()
            .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .image(self.swapchain.get_current_image())
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .build()];

        unsafe {
            self.device
                .cmd_execute_commands(frame.primary_buffer, &frame.secondary_buffers);
            self.device.cmd_end_rendering(frame.primary_buffer);

            self.device.cmd_pipeline_barrier(
                frame.primary_buffer,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                DependencyFlags::empty(),
                &[],
                &[],
                &image_barrier,
            );

            self.device
                .end_command_buffer(frame.primary_buffer)
                .unwrap();
        }

        self.present_channel
            .send(PresentData {
                render_semaphore: frame.graphics_semaphore,
                present_semaphore: frame.present_semaphore,
                cmd: frame.primary_buffer,
                swapchain: self.swapchain.swapchain,
                swapchain_loader: self.swapchain.loader.clone(),
                image_index: self.swapchain.current_image_index as u32,
                signal_fence: frame.fence,
                sync_data: frame.sync_data.clone(),
            })
            .unwrap();
        self.frame_count += 1;
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.resolution = [width, height];
    }

    fn load_model(&mut self, path: &Path) -> Result<Arc<Mesh>, Box<dyn Error>> {
        let obj: Obj = load_obj(BufReader::new(File::open(path)?))?;
        let vertices = obj
            .vertices
            .into_iter()
            .map(|vertex| Vertex {
                position: nalgebra::Vector3::from(vertex.position),
                normal: nalgebra::UnitVector3::new_normalize(nalgebra::Vector3::from(
                    vertex.normal,
                )),
                uv: Default::default(), //todo
            })
            .collect();
        let indices = obj.indices.into_iter().map(|index| index as u32).collect();

        let alloc = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(self.utility_pool)
            .level(vk::CommandBufferLevel::PRIMARY);
        let cmd = unsafe { self.device.allocate_command_buffers(&alloc)? }[0];
        let mesh = Mesh::new(
            vertices,
            indices,
            &self.device,
            cmd,
            self.graphics_queue,
            self.allocator.clone(),
        )
        .map(Arc::new);
        let cmd = [cmd];
        unsafe { self.device.free_command_buffers(self.utility_pool, &cmd) };
        info!("Loaded model {path:?}");
        Ok(mesh?)
    }

    fn load_material(&mut self) -> Result<Arc<Material>, Box<dyn Error>> {
        let shaders = DIRS.asset.join("shaders");
        let data = vec![
            fs::read(shaders.join("base.vert.spv"))?,
            fs::read(shaders.join("base.frag.spv"))?,
        ];

        let (pipeline, layout) = create_pipeline(
            &self.device,
            self.surface_format.format,
            self.depth_format,
            self.swapchain.extent,
            data,
            self.global_descriptor_layout,
        )?;
        let alloc = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(self.utility_pool)
            .level(vk::CommandBufferLevel::PRIMARY);
        let cmd = unsafe { self.device.allocate_command_buffers(&alloc)? }[0];
        let anisotropy = unsafe {
            self.instance
                .get_physical_device_properties(self.physical_device)
                .limits
                .max_sampler_anisotropy
        };
        let texture = Texture::new(
            "texture.png",
            self.device.clone(),
            cmd,
            self.graphics_queue,
            anisotropy,
            self.allocator.clone(),
        );
        let cmd = [cmd];
        unsafe { self.device.free_command_buffers(self.utility_pool, &cmd) };

        info!("Created graphics pipeline");
        Ok(Arc::new(Material {
            pipeline,
            layout,
            device: self.device.clone(),
            texture: texture.ok(),
        }))
    }

    fn wait(&self) {
        unsafe { self.device.device_wait_idle().unwrap() };
    }
}

/// This function runs in worker threads and records rendering commands to secondary command buffers
///
/// # Arguments
///
/// * `receiver`: channel to receive rendering commands on
/// * `device`: device handle
/// * `barrier`: barrier for synchronizing worker threads with the main thread
fn render_thread(receiver: Receiver<RenderCommand>, device: &ash::Device, barrier: &Barrier) {
    let mut cmd = vk::CommandBuffer::null();
    let mut last_mesh = std::ptr::null();
    let mut last_material = std::ptr::null();
    let mut view = Default::default();
    let mut projection = Perspective3::from_matrix_unchecked(Default::default());
    let mut global_descriptors = [vk::DescriptorSet::null()];
    while let Ok(command) = receiver.recv() {
        match command {
            // initialize some per frame data for this thread and begin the command buffer
            RenderCommand::Begin(
                cmd_buf,
                view_matrix,
                proj,
                desc,
                surface_format,
                depth_format,
            ) => unsafe {
                cmd = cmd_buf;
                view = view_matrix;
                projection = proj;
                global_descriptors[0] = desc;
                let colors = [surface_format];
                let mut rendering_info = vk::CommandBufferInheritanceRenderingInfo::builder()
                    .color_attachment_formats(&colors)
                    .rasterization_samples(vk::SampleCountFlags::TYPE_1)
                    .depth_attachment_format(depth_format);
                let inheritance_info =
                    vk::CommandBufferInheritanceInfo::builder().push_next(&mut rendering_info);
                let begin_info = vk::CommandBufferBeginInfo::builder()
                    .inheritance_info(&inheritance_info)
                    .flags(
                        vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT
                            | vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE,
                    );
                device.begin_command_buffer(cmd, &begin_info).unwrap();
            },

            // record the rendering commands
            RenderCommand::Render(mesh, material, transform) => {
                debug_assert_ne!(cmd, vk::CommandBuffer::null());
                if cull_test(&mesh, &transform, &view, &projection) {
                    unsafe {
                        if !std::ptr::eq(mesh.as_ref(), last_mesh) {
                            last_mesh = mesh.as_ref();
                            mesh.bind(device, cmd);
                        }

                        if !std::ptr::eq(material.as_ref(), last_material) {
                            last_material = material.as_ref();
                            material.bind(device, cmd);
                            device.cmd_bind_descriptor_sets(
                                cmd,
                                vk::PipelineBindPoint::GRAPHICS,
                                material.get_pipeline_layout(),
                                0,
                                &global_descriptors,
                                &[],
                            );
                        }

                        device.cmd_push_constants(
                            cmd,
                            material.get_pipeline_layout(),
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            std::slice::from_raw_parts(
                                transform.as_ptr() as *const u8,
                                std::mem::size_of::<Matrix4<f32>>(),
                            ),
                        );

                        device.cmd_draw_indexed(cmd, mesh.get_index_count(), 1, 0, 0, 0);
                    }
                }
            }

            // end the command buffer, reset pointers, and synchronize with the other threads using the barrier
            RenderCommand::End => unsafe {
                device.end_command_buffer(cmd).unwrap();
                last_mesh = std::ptr::null();
                last_material = std::ptr::null();
                cmd = vk::CommandBuffer::null();
                barrier.wait();
            },
        }
    }
}

/// This function is used to perform queue submission and
/// presentation in a dedicated thread
///
/// # Arguments
///
/// * `receiver`: the receiver for the channel that sends presentation data to the thread
/// * `device`: device handle
/// * `graphics_queue`: graphics queue handle
/// * `presentation_queue`: presentation queue handle
fn presentation_thread(
    receiver: Receiver<PresentData>,
    device: &ash::Device,
    graphics_queue: vk::Queue,
    presentation_queue: vk::Queue,
) {
    while let Ok(data) = receiver.recv() {
        let submit_info = [vk::SubmitInfo::builder()
            .command_buffers(&[data.cmd])
            .wait_semaphores(&[data.present_semaphore])
            .signal_semaphores(&[data.render_semaphore])
            .wait_dst_stage_mask(&[vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT])
            .build()];

        let wait_semaphore = [data.render_semaphore];
        let swapchain = [data.swapchain];
        let image_index = [data.image_index];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphore)
            .swapchains(&swapchain)
            .image_indices(&image_index);

        unsafe {
            device
                .queue_submit(graphics_queue, &submit_info, data.signal_fence)
                .map_err(|e| error!("Queue submission error {e:?}"))
                .expect("Queue submit failed");
            let suboptimal = match data
                .swapchain_loader
                .queue_present(presentation_queue, &present_info)
            {
                Ok(val) => val,
                Err(e) if e == vk::Result::ERROR_OUT_OF_DATE_KHR => true,
                Err(e) => panic!("Swapchain presentation error: {e}"),
            };
            {
                let mut lock = data.sync_data.0.lock();
                *lock = if suboptimal {
                    RenderResult::OutOfDate
                } else {
                    RenderResult::Ok
                };
            }
            data.sync_data.1.notify_one();
        }
    }
}

/// Helper function to handle transitioning the color image and depth image to the correct layout
unsafe fn begin(
    image_view: vk::ImageView,
    depth_view: vk::ImageView,
    extent: vk::Extent2D,
    cmd: vk::CommandBuffer,
    device: &ash::Device,
) {
    let color_attachment = [vk::RenderingAttachmentInfo::builder()
        .image_view(image_view)
        .image_layout(vk::ImageLayout::ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0., 0., 0., 1.],
            },
        })
        .build()];
    let depth_attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(depth_view)
        .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::DONT_CARE)
        .clear_value(vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.,
                stencil: 0,
            },
        });

    let rendering_info = vk::RenderingInfo::builder()
        .flags(vk::RenderingFlagsKHR::CONTENTS_SECONDARY_COMMAND_BUFFERS)
        .layer_count(1)
        .color_attachments(&color_attachment)
        .depth_attachment(&depth_attachment)
        .render_area(vk::Rect2D {
            offset: Default::default(),
            extent,
        });

    device.cmd_begin_rendering(cmd, &rendering_info);
}

unsafe fn pre_image_transition(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    color_image: vk::Image,
    depth_image: vk::Image,
) {
    let image_barrier = [vk::ImageMemoryBarrier::builder()
        .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::ATTACHMENT_OPTIMAL)
        .image(color_image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .build()];

    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::TOP_OF_PIPE,
        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &image_barrier,
    );

    let depth_barrier = [vk::ImageMemoryBarrier::builder()
        .dst_access_mask(
            vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ,
        )
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
        .image(depth_image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
        .build()];
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::TOP_OF_PIPE,
        vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &depth_barrier,
    );
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();

            ManuallyDrop::drop(&mut self.present_channel);
            let present_thread_handle = ManuallyDrop::take(&mut self.present_thread_handle);
            if let Err(e) = present_thread_handle.join() {
                error!("Error in presentation thread {e:?}");
            }

            self.render_channels.clear();
            while let Some(handle) = self.render_thread_handles.pop() {
                if let Err(e) = handle.join() {
                    error!("Error in rendering thread {e:?}");
                }
            }

            self.device.destroy_command_pool(self.utility_pool, None);
            for frame in &mut self.frames {
                self.device.destroy_command_pool(frame.primary_pool, None);
                for pool in &frame.secondary_pools {
                    self.device.destroy_command_pool(*pool, None);
                }
                self.device
                    .destroy_semaphore(frame.graphics_semaphore, None);
                self.device.destroy_semaphore(frame.present_semaphore, None);
                self.device.destroy_fence(frame.fence, None);
                ManuallyDrop::drop(&mut frame.ubo);
            }

            ManuallyDrop::drop(&mut self.depth_image);
            self.device.destroy_image_view(self.depth_view, None);
            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
            self.device
                .destroy_descriptor_set_layout(self.global_descriptor_layout, None);

            ManuallyDrop::drop(&mut self.swapchain);

            if let Some(alloc) = Arc::get_mut(&mut self.allocator) {
                alloc.destroy();
            } else {
                error!("Allocator reference count was > 1 at destruction, this may indicate a memory leak");
                // TODO use get_unchecked to destroy the allocator anyway once it's stabilized
                panic!("Allocator destroyed while still in use");
            }
            cleanup_cache(&self.device);
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
            #[cfg(feature = "validation-layers")]
            self.debug_messenger
                .0
                .destroy_debug_utils_messenger(self.debug_messenger.1, None);
            self.instance.destroy_instance(None);
        }
    }
}

/// Callback that logs validation layer messages using the log crate.
/// Disabled if validation-layers feature is not enabled
#[cfg(feature = "validation-layers")]
unsafe extern "system" fn debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut std::ffi::c_void,
) -> vk::Bool32 {
    use ash::vk::DebugUtilsMessageSeverityFlagsEXT as Flags;
    let severity = match message_severity {
        Flags::INFO => Level::Info,
        Flags::WARNING => Level::Warn,
        Flags::ERROR => Level::Error,
        Flags::VERBOSE => Level::Trace,
        _ => Level::Debug,
    };
    log!(target: "Validation Layers", severity, "[{:?}] {}: {}",
        message_types,
        CStr::from_ptr((*p_callback_data).p_message_id_name).to_string_lossy(),
        CStr::from_ptr((*p_callback_data).p_message).to_string_lossy());
    ash::vk::FALSE
}

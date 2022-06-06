use std::error::Error;
use std::ffi::CStr;
use std::fs::File;
use std::io::BufReader;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::{Arc, Barrier};
use std::thread::JoinHandle;

use ash::vk;
use crossbeam_channel::{Receiver, Sender};
use log::{error, log, Level, info};
use nalgebra::{Matrix4, Perspective3, Transform3};
use obj::{load_obj, Obj};
use smallvec::SmallVec;

use crate::vulkan::engine::alloc::{destroy_allocator, GpuObject};
use crate::{cull_test, Material, Mesh, RenderingEngine};
use crate::vulkan::engine::pipeline::{cleanup_cache, create_graphics_pipeline};
use crate::vulkan::mesh::Vertex;

pub(crate) mod alloc;
mod init;
mod pipeline;

const FRAMES_IN_FLIGHT: usize = 2;
pub struct Engine {
    frame_count: u64,
    _entry: Box<ash::Entry>,
    instance: Box<ash::Instance>,
    device: Arc<ash::Device>,
    surface_loader: Box<ash::extensions::khr::Surface>,
    #[cfg(feature = "validation-layers")]
    debug_messenger: (
        Box<ash::extensions::ext::DebugUtils>,
        vk::DebugUtilsMessengerEXT,
    ),
    surface: vk::SurfaceKHR,
    graphics_queue: vk::Queue,
    presentation_queue: vk::Queue,
    surface_format: vk::SurfaceFormatKHR,
    swapchain: vk::SwapchainKHR,
    swapchain_loader: Arc<ash::extensions::khr::Swapchain>,
    swapchain_extent: vk::Extent2D,
    swapchain_images: Vec<vk::Image>,
    swapchain_views: Vec<vk::ImageView>,
    frames: [Frame; FRAMES_IN_FLIGHT],
    render_channels: SmallVec<[Sender<RenderCommand>; 12]>,
    render_thread_handles: SmallVec<[JoinHandle<()>; 12]>,
    render_barrier: Arc<Barrier>,
    present_channel: ManuallyDrop<Sender<PresentData>>,
    present_thread_handle: ManuallyDrop<JoinHandle<()>>,
    last_mesh: *const Mesh,
    last_material: *const Material,
    current_thread: usize,
    current_image_index: u32,
    utility_pool: vk::CommandPool
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
    ubo: ManuallyDrop<GpuObject<Ubo>>
}

#[derive(Debug)]
struct Ubo {
    view: Matrix4<f32>,
    projection: Matrix4<f32>,
}

enum RenderCommand {
    Begin(vk::CommandBuffer, Matrix4<f32>, Perspective3<f32>),
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
}

impl RenderingEngine for Engine {
    fn begin_rendering(&mut self, view: &Matrix4<f32>, projection: &Perspective3<f32>) {
        let frame = &mut self.frames[self.frame_count as usize % FRAMES_IN_FLIGHT];
        let fences = [frame.fence];
        unsafe {
            if let Err(err) = self.device.wait_for_fences(&fences, true, u64::MAX) {
                error!("Error waiting on fence: {err}");
            }
            self.device.reset_fences(&fences).unwrap();
            let (index, _ok) = self
                .swapchain_loader
                .acquire_next_image(
                    self.swapchain,
                    u64::MAX,
                    frame.present_semaphore,
                    vk::Fence::null(),
                )
                .expect("Failed to acquire swapchain image");
            self.current_image_index = index;
            frame.ubo.view = *view;
            frame.ubo.projection = projection.to_homogeneous();
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

            for buf in &frame.secondary_buffers {
                let colors = [self.surface_format.format];
                let mut rendering_info = vk::CommandBufferInheritanceRenderingInfo::builder()
                    .color_attachment_formats(&colors)
                    .rasterization_samples(vk::SampleCountFlags::TYPE_1);
                let inheritance_info =
                    vk::CommandBufferInheritanceInfo::builder().push_next(&mut rendering_info);
                let begin_info = vk::CommandBufferBeginInfo::builder()
                    .inheritance_info(&inheritance_info)
                    .flags(
                        vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT
                            | vk::CommandBufferUsageFlags::RENDER_PASS_CONTINUE,
                    );
                self.device.begin_command_buffer(*buf, &begin_info).unwrap();
            }

            let color_attachment = [vk::RenderingAttachmentInfo::builder()
                .image_view(self.swapchain_views[index as usize])
                .image_layout(vk::ImageLayout::ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [1.;4]
                    }
                })
                .build()];

            let rendering_info = vk::RenderingInfo::builder()
                .flags(vk::RenderingFlagsKHR::CONTENTS_SECONDARY_COMMAND_BUFFERS)
                .layer_count(1)
                .color_attachments(&color_attachment)
                .render_area(vk::Rect2D {
                    offset: Default::default(),
                    extent: self.swapchain_extent,
                });
            self.device
                .cmd_begin_rendering(frame.primary_buffer, &rendering_info);
            for (index, channel) in self.render_channels.iter().enumerate() {
                channel
                    .send(RenderCommand::Begin(
                        frame.secondary_buffers[index],
                        *view,
                        *projection,
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

        unsafe {
            for buffer in &frame.secondary_buffers {
                self.device.end_command_buffer(*buffer).unwrap();
            }
            self.device
                .cmd_execute_commands(frame.primary_buffer, &frame.secondary_buffers);
            self.device.cmd_end_rendering(frame.primary_buffer);
            self.device
                .end_command_buffer(frame.primary_buffer)
                .unwrap();
        }

        self.present_channel
            .send(PresentData {
                render_semaphore: frame.graphics_semaphore,
                present_semaphore: frame.present_semaphore,
                cmd: frame.primary_buffer,
                swapchain: self.swapchain,
                swapchain_loader: self.swapchain_loader.clone(),
                image_index: self.current_image_index,
            })
            .unwrap();
    }

    fn resize(&mut self, width: u32, height: u32) {
        if self.swapchain_extent.width == width && self.swapchain_extent.height == height {
            return;
        }
        unsafe {
            self.device.device_wait_idle().unwrap();
            todo!("Handle resizing the swapchain")
        }
    }

    fn load_model(&mut self, path: &Path) -> Result<Arc<Mesh>, Box<dyn Error>> {
        let obj: Obj = load_obj(BufReader::new(File::open(path)?))?;
        let vertices = obj.vertices.into_iter().map(|vertex| {
            Vertex {
                position: nalgebra::Vector3::from(vertex.position),
                normal: nalgebra::UnitVector3::new_normalize(nalgebra::Vector3::from(vertex.normal)),
            }
        }).collect();
        let indices = obj.indices.into_iter().map(|index| index as u32).collect();

        let alloc = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(self.utility_pool)
            .level(vk::CommandBufferLevel::PRIMARY);
        let cmd = unsafe {self.device.allocate_command_buffers(&alloc)?}[0];
        let mesh = Mesh::new(vertices, indices, &self.device, cmd, self.graphics_queue).map(Arc::new);
        let cmd = [cmd];
        unsafe {self.device.free_command_buffers(self.utility_pool, &cmd)};
        info!("Loaded model {path:?}");
        mesh
    }

    fn load_material(&mut self) -> Result<Arc<Material>, Box<dyn Error>> {
        let (pipeline, layout) = create_graphics_pipeline(&self.device, self.surface_format.format, self.swapchain_extent)?;
        info!("Created graphics pipeline");
        Ok(Arc::new(Material {
            pipeline,
            layout,
            device: self.device.clone()
        }))
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
    while let Ok(command) = receiver.recv() {
        match command {
            // initialize some per frame data for this thread
            RenderCommand::Begin(cmd_buf, view_matrix, proj) => {
                cmd = cmd_buf;
                view = view_matrix;
                projection = proj;
            }

            // record the rendering commands
            RenderCommand::Render(mesh, material, transform) => {
                debug_assert_ne!(cmd, vk::CommandBuffer::null());
                if !cull_test(&mesh, &transform, &view, &projection) {
                    continue;
                }
                unsafe {
                    if !std::ptr::eq(mesh.as_ref(), last_mesh) {
                        last_mesh = mesh.as_ref();
                       // mesh.bind(device, cmd);
                    }
                    if !std::ptr::eq(material.as_ref(), last_material) {
                        last_material = material.as_ref();
                        material.bind(device, cmd);
                    }

                    // device.cmd_push_constants(
                    //     cmd,
                    //     material.get_pipeline_layout(),
                    //     vk::ShaderStageFlags::VERTEX,
                    //     0,
                    //     std::slice::from_raw_parts(
                    //         transform.as_ptr() as *const u8,
                    //         std::mem::size_of::<Transform3<f32>>(),
                    //     ),
                    // );

                    //device.cmd_draw_indexed(cmd, mesh.get_index_count(), 1, 0, 0, 0);
                    device.cmd_draw(cmd, 3, 1, 0, 0);
                }
            }

            // reset pointers and synchronize with the other threads using the barrier
            RenderCommand::End => {
                last_mesh = std::ptr::null();
                last_material = std::ptr::null();
                cmd = vk::CommandBuffer::null();
                barrier.wait();
            }
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
                .queue_submit(graphics_queue, &submit_info, vk::Fence::null())
                .expect("Queue submit failed");
            data.swapchain_loader
                .queue_present(presentation_queue, &present_info)
                .expect("Queue presentation failed");
        }
    }
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

            for view in &self.swapchain_views {
                self.device.destroy_image_view(*view, None);
            }

            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            destroy_allocator();
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

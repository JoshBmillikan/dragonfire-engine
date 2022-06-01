use std::ffi::{CStr, CString};
use std::mem::ManuallyDrop;
use std::sync::{Arc, Barrier};
use std::thread::JoinHandle;

use ash::vk;
use crossbeam_channel::{Receiver, Sender};
use log::{error, log, Level};
use nalgebra::{Matrix4, Perspective3, Projective3, Transform3};
use smallvec::SmallVec;

use crate::vulkan::engine::alloc::destroy_allocator;
use crate::{cull_test, Material, Mesh, RenderingEngine};

mod alloc;
mod init;

const FRAMES_IN_FLIGHT: usize = 2;
pub struct Engine {
    frame_count: u64,
    entry: Box<ash::Entry>,
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
    swapchain_loader: Box<ash::extensions::khr::Swapchain>,
    swapchain_extent: vk::Extent2D,
    swapchain_images: Vec<vk::Image>,
    swapchain_views: Vec<vk::ImageView>,
    frames: [Frame; FRAMES_IN_FLIGHT],
    render_channels: SmallVec<[Sender<RenderCommand>; 12]>,
    render_thread_handles: SmallVec<[JoinHandle<()>; 12]>,
    render_barrier: Arc<Barrier>,
    present_channel: ManuallyDrop<Sender<()>>,
    present_thread_handle: ManuallyDrop<JoinHandle<()>>,
    last_mesh: *const Mesh,
    last_material: *const Material,
    current_thread: usize,
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
}

enum RenderCommand {
    Begin(vk::CommandBuffer, Transform3<f32>, Perspective3<f32>),
    Render(Arc<Mesh>, Arc<Material>, Matrix4<f32>),
    End,
}

impl RenderingEngine for Engine {
    fn begin_rendering(&mut self, view: &Transform3<f32>, projection: &Perspective3<f32>) {
        let frame = self.get_frame();
        todo!()
    }

    fn render(&mut self, mesh: &Arc<Mesh>, material: &Arc<Material>, transform: &Transform3<f32>) {
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
                transform.to_homogeneous(),
            ))
            .expect("Failed to send render command");
    }

    fn end_rendering(&mut self) {
        for channel in &self.render_channels {
            channel.send(RenderCommand::End).unwrap();
        }
        todo!()
    }

    fn resize(&mut self, width: u32, height: u32) {
        if self.swapchain_extent.width == width && self.swapchain_extent.height == height {
            return;
        }
        unsafe {
            self.device.device_wait_idle().unwrap();
            todo!()
        }
    }
}

impl Engine {
    #[inline]
    fn get_frame(&self) -> &Frame {
        &self.frames[self.frame_count as usize % FRAMES_IN_FLIGHT]
    }
}

fn render_thread(receiver: Receiver<RenderCommand>, device: &ash::Device, barrier: &Barrier) {
    let mut cmd = vk::CommandBuffer::null();
    let mut last_mesh = std::ptr::null();
    let mut last_material = std::ptr::null();
    let mut view = Default::default();
    let mut projection = Perspective3::from_matrix_unchecked(Default::default());
    while let Ok(command) = receiver.recv() {
        match command {
            RenderCommand::Begin(cmd_buf, view_matrix, proj) => {
                cmd = cmd_buf;
                view = view_matrix;
                projection = proj;
            }
            RenderCommand::Render(mesh, material, transform) => {
                debug_assert_ne!(cmd, vk::CommandBuffer::null());
                if !cull_test(&mesh, &transform, &view, &projection) {
                    continue;
                }
                unsafe {
                    if !std::ptr::eq(mesh.as_ref(), last_mesh) {
                        last_mesh = mesh.as_ref();
                        mesh.bind(device, cmd);
                    }
                    if !std::ptr::eq(material.as_ref(), last_material) {
                        last_material = material.as_ref();
                        material.bind(device, cmd);
                    }

                    device.cmd_push_constants(
                        cmd,
                        material.get_pipeline_layout(),
                        vk::ShaderStageFlags::VERTEX,
                        0,
                        std::slice::from_raw_parts(
                            transform.as_ptr() as *const u8,
                            std::mem::size_of::<Transform3<f32>>(),
                        ),
                    );

                    device.cmd_draw_indexed(cmd, mesh.indices.len() as u32, 1, 0, 0, 0);
                }
            }
            RenderCommand::End => {
                last_mesh = std::ptr::null();
                last_material = std::ptr::null();
                cmd = vk::CommandBuffer::null();
                barrier.wait();
            }
        }
    }
}

fn presentation_thread(
    receiver: Receiver<()>,
    device: &ash::Device,
    graphics_queue: vk::Queue,
    presentation_queue: vk::Queue,
) {
    while let Ok(data) = receiver.recv() {}
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

            for frame in &self.frames {
                self.device.destroy_command_pool(frame.primary_pool, None);
                for pool in &frame.secondary_pools {
                    self.device.destroy_command_pool(*pool, None);
                }
                self.device
                    .destroy_semaphore(frame.graphics_semaphore, None);
                self.device.destroy_semaphore(frame.present_semaphore, None);
                self.device.destroy_fence(frame.fence, None);
            }

            for view in &self.swapchain_views {
                self.device.destroy_image_view(*view, None);
            }
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
            destroy_allocator();
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

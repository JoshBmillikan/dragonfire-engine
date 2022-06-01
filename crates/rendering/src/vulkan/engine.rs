use std::ffi::{CStr, CString};
use std::sync::Arc;
use std::thread::JoinHandle;

use ash::vk;
use crossbeam_channel::{Receiver, Sender};
use log::{error, Level, log};
use smallvec::SmallVec;

use crate::nalgebra::{Projective3, Transform3};
use crate::RenderingEngine;

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
    render_thread_handles: SmallVec<[JoinHandle<()>;12]>,
}

#[derive(Debug)]
struct Frame {
    primary_buffer: vk::CommandBuffer,
    primary_pool: vk::CommandPool,
    secondary_buffers: SmallVec<[vk::CommandBuffer; 12]>,
    secondary_pools: SmallVec<[vk::CommandPool; 12]>,
    fence: vk::Fence,
    graphics_semaphore: vk::Semaphore,
    present_semaphore: vk::Semaphore
}

enum RenderCommand {

}

impl RenderingEngine for Engine {
    fn begin_rendering(&mut self, view: &Transform3<f32>, projection: &Projective3<f32>) {
        todo!()
    }

    fn render(&mut self) {
        todo!()
    }

    fn end_rendering(&mut self) {
        todo!()
    }

    fn resize(&mut self, width: u32, height: u32) {
        if self.swapchain_extent.width == width && self.swapchain_extent.height == height {
            return;
        }
        todo!()
    }
}

fn render_thread(receiver: Receiver<RenderCommand>, device: &ash::Device) {

}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            
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
                self.device.destroy_semaphore(frame.graphics_semaphore, None);
                self.device.destroy_semaphore(frame.present_semaphore, None);
                self.device.destroy_fence(frame.fence, None);
            }

            for view in &self.swapchain_views {
                self.device.destroy_image_view(*view, None);
            }
            self.swapchain_loader
                .destroy_swapchain(self.swapchain, None);
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

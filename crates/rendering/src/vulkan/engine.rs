use std::ffi::{CStr, CString};

use ash::vk;

use engine::log::Level;

use crate::nalgebra::{Projective3, Transform3};
use crate::RenderingEngine;

mod init;

const FRAMES_IN_FLIGHT: usize = 2;
pub struct Engine {
    frame_count: u64,
    entry: Box<ash::Entry>,
    instance: Box<ash::Instance>,
    device: Box<ash::Device>,
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
}

#[derive(Debug)]
struct Frame {

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

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
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
    engine::log::log!(target: "Validation Layers", severity, "[{:?}] {}: {}",
        message_types,
        CStr::from_ptr((*p_callback_data).p_message_id_name).to_string_lossy(),
        CStr::from_ptr((*p_callback_data).p_message).to_string_lossy());
    ash::vk::FALSE
}

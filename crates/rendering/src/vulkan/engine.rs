use ash::vk;
use engine::log::Level;

use crate::RenderingEngine;
use std::ffi::{CStr, CString};

mod init;
pub struct Engine {
    entry: Box<ash::Entry>,
    instance: Box<ash::Instance>,
    device: Box<ash::Device>,
    surface_loader: Box<ash::extensions::khr::Surface>,
    surface: vk::SurfaceKHR,
}

impl RenderingEngine for Engine {

}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_device(None);
            self.surface_loader.destroy_surface(self.surface, None);
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
    engine::log::log!(target: "Validation Layers",severity,"[{:?}] {}: {}",
        message_types,
        CStr::from_ptr((*p_callback_data).p_message_id_name).to_string_lossy(),
        CStr::from_ptr((*p_callback_data).p_message).to_string_lossy());
    ash::vk::FALSE
}
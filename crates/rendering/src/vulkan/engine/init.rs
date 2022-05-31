use std::error::Error;
use std::ffi::{CStr, CString};

use ash::extensions::ext::DebugUtils;
use ash::vk;
use ash::vk::PhysicalDeviceType;
use itertools::Itertools;
use raw_window_handle::HasRawWindowHandle;

use engine::log::{info, warn};

use crate::GraphicsSettings;
use crate::vulkan::engine::{debug_callback, Engine};

impl Engine {
    pub unsafe fn new(
        window: &dyn HasRawWindowHandle,
        settings: &GraphicsSettings,
    ) -> Result<Self, Box<dyn Error>> {
        let entry = load()?;
        let instance = create_instance(&entry, window)?;
        let surface_loader = Box::new(ash::extensions::khr::Surface::new(&entry, &instance));
        let surface = ash_window::create_surface(&entry, &instance, window, None)?;
        let extensions = vec![
            ash::extensions::khr::Swapchain::name(),
            ash::extensions::khr::DynamicRendering::name()
        ];
        let physical_device = get_physical_device(&instance, &surface, &surface_loader,&extensions)?;
        todo!()
    }
}

unsafe fn create_device(instance: &ash::Instance, physical_device: vk::PhysicalDevice, extensions: &Vec<&CStr>) {
    let extensions = extensions.iter().map(|ext| ext.as_ptr()).collect::<Vec<_>>();
    let create_info = vk::DeviceCreateInfo::builder()
        .enabled_extension_names(&extensions)
        ;
}

unsafe fn get_physical_device(
    instance: &ash::Instance,
    surface: &vk::SurfaceKHR,
    surface_loader: &ash::extensions::khr::Surface,
    extensions: &Vec<&CStr>,
) -> Result<vk::PhysicalDevice, Box<dyn Error>> {
    let device = instance
        .enumerate_physical_devices()?
        .into_iter()
        .filter(|device| is_valid_device(*device, instance, extensions))
        .filter(|device| {
            let mut has_graphics = false;
            let mut has_present = false;
            for (index, prop) in instance
                .get_physical_device_queue_family_properties(*device).into_iter().enumerate() {
                if prop.queue_flags & vk::QueueFlags::GRAPHICS != vk::QueueFlags::empty() {
                    has_graphics = true;
                }

                if surface_loader.get_physical_device_surface_support(*device, index as u32, *surface).unwrap_or(false) {
                    has_present = true;
                }

                if has_graphics && has_present {
                    break;
                }
            }

            has_present && has_graphics
        })
        .find_or_first(|device| {
            instance.get_physical_device_properties(*device).device_type
                == PhysicalDeviceType::DISCRETE_GPU
        }).ok_or("No valid gpu available")?;
    info!("Using gpu {:?}" ,instance.get_physical_device_properties(device).device_name);
    Ok(device)
}

unsafe fn is_valid_device(
    device: vk::PhysicalDevice,
    instance: &ash::Instance,
    extensions: &Vec<&CStr>,
) -> bool {
    let mut dyn_render_features = vk::PhysicalDeviceDynamicRenderingFeatures::builder();
    let mut features2 = vk::PhysicalDeviceFeatures2::builder().push_next(&mut dyn_render_features);
    instance.get_physical_device_features2(device, &mut features2);
    if dyn_render_features.dynamic_rendering != vk::TRUE {
        return false;
    }

    if let Ok(props) = instance.enumerate_device_extension_properties(device) {
        for ext in extensions {
            if !props
                .iter()
                .any(|prop| prop.extension_name.as_ptr() == ext.as_ptr())
            {
                return false;
            }
        }

        true
    } else {
        false
    }
}

unsafe fn create_instance(
    entry: &ash::Entry,
    window: &dyn HasRawWindowHandle,
) -> Result<Box<ash::Instance>, Box<dyn Error>> {
    let version_str = env!("CARGO_PKG_VERSION").split('.').collect::<Vec<_>>();
    let version = vk::make_api_version(
        0,
        version_str[0].parse()?,
        version_str[1].parse()?,
        version_str[2].parse()?,
    );
    let engine_name = CString::new("Dragonfire Engine")?;

    let mut extensions = ash_window::enumerate_required_extensions(window)?.to_vec();
    if cfg!(feature = "validation-layers") {
        extensions.push(ash::extensions::ext::DebugUtils::name().as_ptr());
    }
    for ext in &extensions {
        info!("Loaded instance extension {:?}", ext);
    }
    let layers = vec![
        #[cfg(feature = "validation-layers")]
        CString::new("VK_LAYER_KHRONOS_validation").unwrap(),
    ];
    let layers = layers
        .iter()
        .map(|layer| {
            info!("Loaded layer {:?}", layer);
            layer.as_ptr()
        })
        .collect::<Vec<_>>();

    let app_info = vk::ApplicationInfo::builder()
        .engine_name(engine_name.as_c_str())
        .api_version(vk::API_VERSION_1_3)
        .engine_version(version)
        .application_version(version);

    let create_info = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_layer_names(&layers)
        .enabled_extension_names(&extensions);

    #[cfg(feature = "validation-layers")]
    let mut debug = get_debug_info();

    #[cfg(feature = "validation-layers")]
    let create_info = create_info.push_next(&mut debug);

    Ok(Box::new(entry.create_instance(&create_info, None)?))
}

unsafe fn load() -> Result<Box<ash::Entry>, Box<dyn Error>> {
    let entry = Box::new(ash::Entry::load()?);
    if let Some(version) = entry.try_enumerate_instance_version()? {
        info!(
            "Loaded Vulkan version {}.{}.{}.{}",
            vk::api_version_major(version),
            vk::api_version_minor(version),
            vk::api_version_patch(version),
            vk::api_version_variant(version)
        );
    } else {
        warn!("Unknown vulkan version loaded");
    }
    Ok(entry)
}

/// loads the debug messenger functions and handle object.
#[cfg(feature = "validation-layers")]
unsafe fn create_debug_messenger(
    entry: &ash::Entry,
    instance: &ash::Instance,
) -> Result<Option<(DebugUtils, vk::DebugUtilsMessengerEXT)>, Box<dyn Error>> {
    let utils = DebugUtils::new(entry, instance);
    let create_info = get_debug_info();
    let messenger = utils.create_debug_utils_messenger(&create_info, None)?;
    Ok(Some((utils, messenger)))
}

/// If validation layers are disabled, returns None
#[cfg(not(feature = "validation-layers"))]
unsafe fn create_debug_messenger(
    _: &ash::Entry,
    _: &ash::Instance,
) -> Result<Option<(DebugUtils, vk::DebugUtilsMessengerEXT)>, Box<dyn Error>> {
    Ok(None)
}

/// gets the create info struct for the debug messenger
#[cfg(feature = "validation-layers")]
fn get_debug_info() -> vk::DebugUtilsMessengerCreateInfoEXT {
    vk::DebugUtilsMessengerCreateInfoEXT::builder()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(debug_callback))
        .build()
}

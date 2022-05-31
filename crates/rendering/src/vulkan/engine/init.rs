use std::error::Error;
use std::ffi::{CStr, CString};

use ash::prelude::VkResult;
use ash::vk;
use ash::vk::PhysicalDeviceType;
use itertools::Itertools;
use raw_window_handle::HasRawWindowHandle;
use smallvec::SmallVec;

use engine::log::{info, warn};

use crate::vulkan::engine::{debug_callback, Engine, Frame, FRAMES_IN_FLIGHT};
use crate::GraphicsSettings;

impl Engine {
    pub unsafe fn new(
        window: &dyn HasRawWindowHandle,
        settings: &GraphicsSettings,
    ) -> Result<Self, Box<dyn Error>> {
        let entry = load()?;
        let instance = create_instance(&entry, window)?;

        #[cfg(feature = "validation-layers")]
        let debug_messenger = create_debug_messenger(&entry, &instance)?;

        let surface_loader = Box::new(ash::extensions::khr::Surface::new(&entry, &instance));
        let surface = ash_window::create_surface(&entry, &instance, window, None)?;
        let extensions = vec![
            ash::extensions::khr::Swapchain::name(),
            ash::extensions::khr::DynamicRendering::name(),
        ];
        let physical_device =
            get_physical_device(&instance, surface, &surface_loader, &extensions)?;
        let queue_families =
            get_queue_families(&instance, physical_device, surface, &surface_loader)?;
        let device = create_device(&instance, physical_device, &extensions, &queue_families)?;
        let graphics_queue = device.get_device_queue(queue_families[0], 0);
        let presentation_queue = device.get_device_queue(queue_families[1], 0);
        let surface_format = get_surface_format(physical_device, surface, &surface_loader)?;

        let swapchain_loader = Box::new(ash::extensions::khr::Swapchain::new(&instance, &device));
        let (swapchain, swapchain_extent) = create_swapchain(
            &swapchain_loader,
            physical_device,
            surface,
            &surface_loader,
            &queue_families,
            surface_format.format,
            settings,
        )?;

        let swapchain_images = swapchain_loader.get_swapchain_images(swapchain)?;
        let swapchain_views = create_swapchain_views(&swapchain_images, &device, surface_format.format);
        let frames = (0..FRAMES_IN_FLIGHT).map(|_| {
            todo!()
        }).collect::<SmallVec<[Frame;FRAMES_IN_FLIGHT]>>();

        info!("Rendering engine initialization finished");
        Ok(Engine {
            frame_count: 0,
            entry,
            instance,
            device,
            surface_loader,
            #[cfg(feature = "validation-layers")]
            debug_messenger,
            surface,
            graphics_queue,
            presentation_queue,
            surface_format,
            swapchain,
            swapchain_loader,
            swapchain_extent,
            swapchain_images,
            swapchain_views,
            frames: frames.into_inner().unwrap()
        })
    }
}

unsafe fn create_swapchain_views(
    images: &[vk::Image],
    device: &ash::Device,
    format: vk::Format,
) -> Vec<vk::ImageView> {
    images
        .iter()
        .map(|image| {
            let range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1)
                .build();
            let create_info = vk::ImageViewCreateInfo::builder()
                .format(format)
                .image(*image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .components(Default::default())
                .subresource_range(range);
            device
                .create_image_view(&create_info, None)
                .expect("Failed to create image view")
        })
        .collect()
}

unsafe fn create_swapchain(
    loader: &ash::extensions::khr::Swapchain,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    surface_loader: &ash::extensions::khr::Surface,
    queue_families: &[u32],
    image_format: vk::Format,
    settings: &GraphicsSettings,
) -> Result<(vk::SwapchainKHR, vk::Extent2D), Box<dyn Error>> {
    let capabilities =
        surface_loader.get_physical_device_surface_capabilities(physical_device, surface)?;
    let extent = if capabilities.current_extent.height == u32::MAX {
        vk::Extent2D {
            width: settings.resolution[0],
            height: settings.resolution[1],
        }
    } else {
        capabilities.current_extent
    };

    let image_count = if capabilities.max_image_count == 0 {
        capabilities.min_image_count + 1
    } else {
        capabilities
            .max_image_count
            .min(capabilities.min_image_count + 1)
    };

    let share_mode = if queue_families[0] == queue_families[1] {
        vk::SharingMode::EXCLUSIVE
    } else {
        vk::SharingMode::CONCURRENT
    };

    let create_info = vk::SwapchainCreateInfoKHR::builder()
        .surface(surface)
        .min_image_count(image_count)
        .image_format(image_format)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(share_mode)
        .queue_family_indices(queue_families)
        .pre_transform(capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(get_present_mode(physical_device, surface, surface_loader)?);
    let swapchain = loader.create_swapchain(&create_info, None)?;

    Ok((swapchain, extent))
}

unsafe fn get_present_mode(
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    surface_loader: &ash::extensions::khr::Surface,
) -> VkResult<vk::PresentModeKHR> {
    Ok(
        if let Some(mode) = surface_loader
            .get_physical_device_surface_present_modes(physical_device, surface)?
            .into_iter()
            .find(|mode| *mode == vk::PresentModeKHR::MAILBOX)
        {
            mode
        } else {
            warn!("Mailbox presentation mode not supported, falling back to FIFO");
            vk::PresentModeKHR::FIFO
        },
    )
}

unsafe fn get_surface_format(
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    surface_loader: &ash::extensions::khr::Surface,
) -> Result<vk::SurfaceFormatKHR, Box<dyn Error>> {
    Ok(surface_loader
        .get_physical_device_surface_formats(physical_device, surface)?
        .into_iter()
        .find_or_first(|fmt| {
            fmt.format == vk::Format::B8G8R8_SRGB
                && fmt.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .ok_or("Failed to find valid surface format")?)
}

unsafe fn create_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    extensions: &[&CStr],
    queue_families: &[u32],
) -> VkResult<Box<ash::Device>> {
    let extensions = extensions
        .iter()
        .map(|ext| ext.as_ptr())
        .collect::<Vec<_>>();
    let queue_priority = [1.];
    let queue_info = queue_families
        .iter()
        .sorted_unstable()
        .dedup()
        .map(|index| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(*index)
                .queue_priorities(&queue_priority)
                .build()
        })
        .collect::<Vec<_>>();

    for ext in &extensions {
        info!("Loaded device extension {:?}", CStr::from_ptr(*ext));
    }

    let create_info = vk::DeviceCreateInfo::builder()
        .enabled_extension_names(&extensions)
        .queue_create_infos(&queue_info);
    Ok(Box::new(instance.create_device(
        physical_device,
        &create_info,
        None,
    )?))
}

unsafe fn get_queue_families(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    surface_loader: &ash::extensions::khr::Surface,
) -> Result<[u32; 2], Box<dyn Error>> {
    let mut graphics = None;
    let mut present = None;
    for (index, prop) in instance
        .get_physical_device_queue_family_properties(physical_device)
        .into_iter()
        .enumerate()
    {
        if prop.queue_flags & vk::QueueFlags::GRAPHICS != vk::QueueFlags::empty() {
            graphics = Some(index as u32);
        }

        if surface_loader
            .get_physical_device_surface_support(physical_device, index as u32, surface)
            .unwrap_or(false)
        {
            present = Some(index as u32);
        }

        if present.is_some() && graphics.is_some() {
            break;
        }
    }
    Ok([
        graphics.ok_or("Failed to find graphics queue")?,
        present.ok_or("Failed to find presentation queue")?,
    ])
}

unsafe fn get_physical_device(
    instance: &ash::Instance,
    surface: vk::SurfaceKHR,
    surface_loader: &ash::extensions::khr::Surface,
    extensions: &[&CStr],
) -> Result<vk::PhysicalDevice, Box<dyn Error>> {
    let device = instance
        .enumerate_physical_devices()?
        .into_iter()
        .filter(|device| is_valid_device(*device, instance, extensions))
        .filter(|device| {
            let mut has_graphics = false;
            let mut has_present = false;
            for (index, prop) in instance
                .get_physical_device_queue_family_properties(*device)
                .into_iter()
                .enumerate()
            {
                if prop.queue_flags & vk::QueueFlags::GRAPHICS != vk::QueueFlags::empty() {
                    has_graphics = true;
                }

                if surface_loader
                    .get_physical_device_surface_support(*device, index as u32, surface)
                    .unwrap_or(false)
                {
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
        })
        .ok_or("No valid gpu available")?;
    info!(
        "Using gpu {:?}",
        CStr::from_ptr(
            instance
                .get_physical_device_properties(device)
                .device_name
                .as_ptr()
        )
    );
    Ok(device)
}

unsafe fn is_valid_device(
    device: vk::PhysicalDevice,
    instance: &ash::Instance,
    extensions: &[&CStr],
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
                .any(|prop| CStr::from_ptr(prop.extension_name.as_ptr()) == *ext)
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
        info!("Loaded instance extension {:?}", CStr::from_ptr(*ext));
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
) -> Result<
    (
        Box<ash::extensions::ext::DebugUtils>,
        vk::DebugUtilsMessengerEXT,
    ),
    Box<dyn Error>,
> {
    let utils = ash::extensions::ext::DebugUtils::new(entry, instance);
    let create_info = get_debug_info();
    let messenger = utils.create_debug_utils_messenger(&create_info, None)?;
    Ok((Box::new(utils), messenger))
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

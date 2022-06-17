use std::error::Error;
use std::ffi::{CStr, CString};
use std::mem::ManuallyDrop;
use std::num::NonZeroUsize;
use std::sync::{Arc, Barrier};
use std::thread::{available_parallelism, spawn};

use ash::prelude::VkResult;
use ash::vk;
use ash::vk::{DeviceSize, PhysicalDeviceType};
use itertools::Itertools;
use log::{info, warn};
use raw_window_handle::HasRawWindowHandle;
use smallvec::SmallVec;
use vk_mem::Allocator;

use crate::GraphicsSettings;
use crate::vulkan::engine::{
    debug_callback, Engine, Frame, FRAMES_IN_FLIGHT, presentation_thread, render_thread, Ubo,
};
use crate::vulkan::engine::alloc::{create_allocator, GpuObject, Image};

impl Engine {
    /// Creates the vulkan rendering engine using a window handle and the graphics settings
    /// # Errors
    /// Returns any errors that occur during vulkan initialization, usually as a result of vulkan api calls
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
            vk::ExtMemoryBudgetFn::name(),
        ];
        let physical_device =
            get_physical_device(&instance, surface, &surface_loader, &extensions)?;
        let queue_families =
            get_queue_families(&instance, physical_device, surface, &surface_loader)?;
        let device = create_device(&instance, physical_device, &extensions, &queue_families)?;
        let allocator = create_allocator(&entry, &instance, physical_device, &device)?;
        let graphics_queue = device.get_device_queue(queue_families[0], 0);
        let presentation_queue = device.get_device_queue(queue_families[1], 0);
        let surface_format = get_surface_format(physical_device, surface, &surface_loader)?;

        let swapchain_loader = Arc::new(ash::extensions::khr::Swapchain::new(&instance, &device));
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
        info!("Using {} swapchain images", swapchain_images.len());
        let swapchain_views =
            create_swapchain_views(&swapchain_images, &device, surface_format.format)?;

        let thread_count = 1.max(
            // half the number of cores, minimum of 1 thread
            available_parallelism()
                .map(NonZeroUsize::get)
                .unwrap_or_default()
                / 2,
        );
        info!("Using {thread_count} render threads");
        let global_descriptor_layout = create_global_descriptor_layout(&device)?;
        let descriptor_pool = create_descriptor_pool(&device)?;
        let frames = (0..FRAMES_IN_FLIGHT)
            .map(|_| {
                create_frame(
                    &device,
                    queue_families[0],
                    thread_count,
                    &allocator,
                    global_descriptor_layout,
                    descriptor_pool,
                )
            })
            .collect::<Result<SmallVec<[_; FRAMES_IN_FLIGHT]>, Box<dyn Error>>>()?;

        let render_barrier = Arc::new(Barrier::new(thread_count + 1));
        let (render_channels, render_thread_handles) = (0..thread_count)
            .map(|_| {
                let (sender, receiver) = crossbeam_channel::bounded(16);
                let device = device.clone();
                let render_barrier = render_barrier.clone();
                (
                    sender,
                    spawn(move || render_thread(receiver, &device, &render_barrier)),
                )
            })
            .unzip();

        let (present_channel, present_thread_handle) = {
            let (sender, receiver) = crossbeam_channel::bounded(0);
            let device = device.clone();
            (
                sender,
                spawn(move || {
                    presentation_thread(receiver, &device, graphics_queue, presentation_queue)
                }),
            )
        };

        let pool_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_families[0])
            .flags(vk::CommandPoolCreateFlags::TRANSIENT);
        let utility_pool = device.create_command_pool(&pool_info, None)?;

        let depth_format = get_depth_format(physical_device, &instance, vk::ImageTiling::OPTIMAL)?;
        let (depth_image, depth_view) =
            create_depth_image(&device, depth_format, swapchain_extent, allocator.clone())?;

        info!("Rendering engine initialization finished");
        Ok(Engine {
            frame_count: 0,
            _entry: entry,
            instance,
            device,
            surface_loader,
            #[cfg(feature = "validation-layers")]
            debug_messenger,
            surface,
            graphics_queue,
            surface_format,
            swapchain,
            swapchain_loader,
            swapchain_extent,
            swapchain_images,
            swapchain_views,
            allocator,
            frames: frames.into_inner().unwrap(),
            render_channels,
            render_thread_handles,
            render_barrier,
            present_channel: ManuallyDrop::new(present_channel),
            present_thread_handle: ManuallyDrop::new(present_thread_handle),
            last_mesh: std::ptr::null(),
            last_material: std::ptr::null(),
            current_thread: 0,
            current_image_index: 0,
            utility_pool,
            global_descriptor_layout,
            descriptor_pool,
            depth_format,
            depth_image: ManuallyDrop::new(depth_image),
            depth_view,
        })
    }
}

/// Loads the vulkan entry functions
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

/// Creates the vulkan instance
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
        .inspect(|layer| info!("Loaded layer {:?}", layer))
        .map(|layer| layer.as_ptr())
        .collect_vec();

    let app_name = CString::new(option_env!("APP_NAME").unwrap_or("test")).unwrap();
    let app_info = vk::ApplicationInfo::builder()
        .engine_name(engine_name.as_c_str())
        .api_version(vk::API_VERSION_1_3)
        .engine_version(version)
        .application_version(version)
        .application_name(app_name.as_c_str());

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

/// Finds the first gpu that is valid for our requirements and is not a integrated gpu.
///
/// Will fall back to a integrated gpu if no discrete gpu was found
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
    let props = instance.get_physical_device_properties(device);
    if props.device_type != PhysicalDeviceType::DISCRETE_GPU {
        warn!("No discrete gpu found, falling back to integrated gpu");
    }
    info!("Using gpu {:?}", CStr::from_ptr(props.device_name.as_ptr()));
    Ok(device)
}

/// Tests if a physical device is valid for our required features and extensions
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

/// Gets the graphics and presentation queue families
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

/// Creates the logical device handle
unsafe fn create_device(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    extensions: &[&CStr],
    queue_families: &[u32],
) -> VkResult<Arc<ash::Device>> {
    let extensions = extensions
        .iter()
        .inspect(|ext| info!("Loaded device extension {ext:?}"))
        .map(|ext| ext.as_ptr())
        .collect_vec();
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
        .collect::<SmallVec<[_; 2]>>();

    let mut features =
        vk::PhysicalDeviceDynamicRenderingFeatures::builder().dynamic_rendering(true);

    let create_info = vk::DeviceCreateInfo::builder()
        .enabled_extension_names(&extensions)
        .push_next(&mut features)
        .queue_create_infos(&queue_info);
    Ok(Arc::new(instance.create_device(
        physical_device,
        &create_info,
        None,
    )?))
}

/// Gets the surface format object for the given surface
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

/// Creates the swapchain along with its extent
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

/// Gets the presentation mode for the surface
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

/// Creates the views for the swapchain images
unsafe fn create_swapchain_views(
    images: &[vk::Image],
    device: &ash::Device,
    format: vk::Format,
) -> VkResult<Vec<vk::ImageView>> {
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
            device.create_image_view(&create_info, None)
        })
        .collect()
}

/// Creates a per-frame data structure
unsafe fn create_frame(
    device: &ash::Device,
    graphics_index: u32,
    thread_count: usize,
    allocator: &Arc<Allocator>,
    global_descriptor_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
) -> Result<Frame, Box<dyn Error>> {
    let create_info = vk::CommandPoolCreateInfo::builder().queue_family_index(graphics_index);
    let primary_pool = device.create_command_pool(&create_info, None)?;
    let alloc_info = vk::CommandBufferAllocateInfo::builder()
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1)
        .command_pool(primary_pool);
    let primary_buffer = device.allocate_command_buffers(&alloc_info)?[0];

    let secondary_pools = (0..thread_count)
        .map(|_| {
            let create_info =
                vk::CommandPoolCreateInfo::builder().queue_family_index(graphics_index);
            device.create_command_pool(&create_info, None)
        })
        .collect::<VkResult<SmallVec<_>>>()?;

    let secondary_buffers = secondary_pools
        .iter()
        .map(|pool| {
            let alloc_info = vk::CommandBufferAllocateInfo::builder()
                .level(vk::CommandBufferLevel::SECONDARY)
                .command_buffer_count(1)
                .command_pool(*pool);
            device.allocate_command_buffers(&alloc_info).map(|it| it[0])
        })
        .collect::<VkResult<_>>()?;

    let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
    let fence = device.create_fence(&fence_info, None)?;
    let graphics_semaphore = device.create_semaphore(&Default::default(), None)?;
    let present_semaphore = device.create_semaphore(&Default::default(), None)?;

    let ubo: GpuObject<Ubo> =
        GpuObject::new(allocator.clone(), vk::BufferUsageFlags::UNIFORM_BUFFER)?;
    let global_descriptor =
        create_global_descriptor_set(device, global_descriptor_layout, descriptor_pool)?;
    let buf_info = [vk::DescriptorBufferInfo::builder()
        .buffer(ubo.get_buffer())
        .offset(0)
        .range(std::mem::size_of::<Ubo>() as DeviceSize)
        .build()];
    let write = [vk::WriteDescriptorSet::builder()
        .dst_set(global_descriptor)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .buffer_info(&buf_info)
        .build()];

    device.update_descriptor_sets(&write, &[]);

    Ok(Frame {
        primary_buffer,
        primary_pool,
        secondary_buffers,
        secondary_pools,
        fence,
        graphics_semaphore,
        present_semaphore,
        ubo: ManuallyDrop::new(ubo),
        global_descriptor,
    })
}

unsafe fn create_global_descriptor_layout(
    device: &ash::Device,
) -> VkResult<vk::DescriptorSetLayout> {
    let bindings = [vk::DescriptorSetLayoutBinding::builder()
        .binding(0)
        .descriptor_count(1)
        .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .build()];
    let layout_info = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
    device.create_descriptor_set_layout(&layout_info, None)
}

unsafe fn create_global_descriptor_set(
    device: &ash::Device,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
) -> VkResult<vk::DescriptorSet> {
    let layouts = [layout];
    let alloc_info = vk::DescriptorSetAllocateInfo::builder()
        .descriptor_pool(pool)
        .set_layouts(&layouts);

    device.allocate_descriptor_sets(&alloc_info).map(|it| it[0])
}

unsafe fn create_descriptor_pool(device: &ash::Device) -> VkResult<vk::DescriptorPool> {
    let sizes = [vk::DescriptorPoolSize::builder()
        .descriptor_count(1)
        .ty(vk::DescriptorType::UNIFORM_BUFFER)
        .build()];
    let create_info = vk::DescriptorPoolCreateInfo::builder()
        .max_sets(16)
        .pool_sizes(&sizes);
    device.create_descriptor_pool(&create_info, None)
}

unsafe fn get_depth_format(
    physical_device: vk::PhysicalDevice,
    instance: &ash::Instance,
    tiling: vk::ImageTiling,
) -> Result<vk::Format, Box<dyn Error>> {
    let possible_formats = [
        vk::Format::D32_SFLOAT,
        vk::Format::D32_SFLOAT_S8_UINT,
        vk::Format::D24_UNORM_S8_UINT,
    ];
    possible_formats
        .iter()
        .find(|fmt| {
            let props = instance.get_physical_device_format_properties(physical_device, **fmt);
            if tiling == vk::ImageTiling::LINEAR {
                !(props.linear_tiling_features & vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
                    .is_empty()
            } else if tiling == vk::ImageTiling::OPTIMAL {
                !(props.optimal_tiling_features & vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
                    .is_empty()
            } else {
                false
            }
        })
        .cloned()
        .ok_or_else(|| "Failed to find a valid depth image format".into())
}

unsafe fn create_depth_image(
    device: &ash::Device,
    format: vk::Format,
    extent: vk::Extent2D,
    allocator: Arc<Allocator>,
) -> Result<(Image, vk::ImageView), Box<dyn Error>> {
    let create_info = vk::ImageCreateInfo::builder()
        .format(format)
        .image_type(vk::ImageType::TYPE_2D)
        .extent(vk::Extent3D::from(extent))
        .mip_levels(1)
        .array_layers(1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .samples(vk::SampleCountFlags::TYPE_1);
    let alloc_info = vk_mem::AllocationCreateInfo {
        usage: vk_mem::MemoryUsage::GpuOnly,
        required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
        ..Default::default()
    };
    let image = Image::new(&create_info, &alloc_info, allocator)?;
    let sub_range = vk::ImageSubresourceRange::builder()
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .aspect_mask(vk::ImageAspectFlags::DEPTH)
        .build();

    let view_info = vk::ImageViewCreateInfo::builder()
        .image(*image)
        .format(format)
        .subresource_range(sub_range)
        .view_type(vk::ImageViewType::TYPE_2D);

    let view = device.create_image_view(&view_info, None)?;
    Ok((image, view))
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

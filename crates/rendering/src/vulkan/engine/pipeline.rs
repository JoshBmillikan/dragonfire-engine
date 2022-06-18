use std::error::Error;
use std::ffi::CString;
use std::fs;
use std::io::Cursor;

use ash::prelude::VkResult;
use ash::vk;
use itertools::Itertools;
use log::{error, info};
use once_cell::sync::OnceCell;
use scopeguard::defer;
use spirv_reflect::types::ReflectShaderStageFlags;

use engine::filesystem::DIRS;

use crate::vulkan::mesh::Vertex;

static CACHE: OnceCell<vk::PipelineCache> = OnceCell::new();

pub fn create_pipeline(
    device: &ash::Device,
    image_fmt: vk::Format,
    depth_fmt: vk::Format,
    extent: vk::Extent2D,
    module_data: Vec<Vec<u8>>,
    global_descriptor_layout: vk::DescriptorSetLayout,
) -> Result<(vk::Pipeline, vk::PipelineLayout), Box<dyn Error>> {
    let module_data = module_data
        .into_iter()
        .map(|data| spirv_reflect::create_shader_module(&data).map(|it| (it, data)))
        .map_ok(|(reflect, data)| {
            let mut cursor = Cursor::new(data);
            ash::util::read_spv(&mut cursor).map(|it| (reflect, it))
        })
        .flatten_ok()
        .map_ok(|(reflect, data)| {
            let create_info = vk::ShaderModuleCreateInfo::builder().code(&data);
            unsafe { device.create_shader_module(&create_info, None) }.map(|it| (reflect, it))
        })
        .flatten_ok()
        .collect::<Result<Vec<_>, _>>()?;

    defer! {
        for (_,module) in &module_data {
            unsafe {
                device.destroy_shader_module(*module, None);
            }
        }
    }

    let name = CString::new("main").unwrap();
    let stages = module_data
        .iter()
        .map(|(info, module)| {
            match info.get_shader_stage() {
                ReflectShaderStageFlags::VERTEX => Ok(vk::ShaderStageFlags::VERTEX),
                ReflectShaderStageFlags::FRAGMENT => Ok(vk::ShaderStageFlags::FRAGMENT),
                ReflectShaderStageFlags::GEOMETRY => Ok(vk::ShaderStageFlags::GEOMETRY),
                ReflectShaderStageFlags::TESSELLATION_CONTROL => {
                    Ok(vk::ShaderStageFlags::TESSELLATION_CONTROL)
                }
                ReflectShaderStageFlags::TESSELLATION_EVALUATION => {
                    Ok(vk::ShaderStageFlags::TESSELLATION_EVALUATION)
                }
                ReflectShaderStageFlags::COMPUTE => Ok(vk::ShaderStageFlags::COMPUTE),
                _ => Err("Invalid stage flags"),
            }
                .map(|stage| {
                    vk::PipelineShaderStageCreateInfo::builder()
                        .stage(stage)
                        .module(*module)
                        .name(&name)
                        .build()
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let fmts = [image_fmt];
    let mut render_info =
        vk::PipelineRenderingCreateInfo::builder().color_attachment_formats(&fmts).depth_attachment_format(depth_fmt);

    let (bindings, attributes) = Vertex::get_vertex_description();
    let vert_input = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attributes);

    let viewport = [vk::Viewport::builder()
        .x(0.)
        .y(0.)
        .width(extent.width as f32)
        .height(extent.height as f32)
        .min_depth(0.)
        .max_depth(1.)
        .build()];

    let scissor = [vk::Rect2D {
        offset: Default::default(),
        extent,
    }];

    let depth = vk::PipelineDepthStencilStateCreateInfo::builder()
        .depth_test_enable(true)
        .depth_write_enable(true)
        .depth_compare_op(vk::CompareOp::LESS)
        .depth_bounds_test_enable(false)
        .stencil_test_enable(false)
        .min_depth_bounds(0.)
        .max_depth_bounds(1.);

    let viewport = vk::PipelineViewportStateCreateInfo::builder()
        .viewports(&viewport)
        .scissors(&scissor);

    let input_asm = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .primitive_restart_enable(false)
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

    let raster = vk::PipelineRasterizationStateCreateInfo::builder()
        .depth_clamp_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false);

    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .min_sample_shading(1.)
        .alpha_to_coverage_enable(false)
        .alpha_to_one_enable(false);

    let color_attachment = [vk::PipelineColorBlendAttachmentState::builder()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false)
        .build()]; // todo alpha blend

    let color = vk::PipelineColorBlendStateCreateInfo::builder()
        .logic_op_enable(false)
        .attachments(&color_attachment);

    let desc = [global_descriptor_layout];
    let layout = create_layout(module_data.iter().map(|it| &it.0), device, &desc)?;

    let create_info = [vk::GraphicsPipelineCreateInfo::builder()
        .push_next(&mut render_info)
        .stages(&stages)
        .vertex_input_state(&vert_input)
        .viewport_state(&viewport)
        .input_assembly_state(&input_asm)
        .rasterization_state(&raster)
        .render_pass(vk::RenderPass::null())
        .multisample_state(&multisample)
        .color_blend_state(&color)
        .layout(layout)
        .depth_stencil_state(&depth)
        .build()];

    let cache = CACHE.get_or_try_init(|| load_cache(device))?;
    match unsafe { device.create_graphics_pipelines(*cache, &create_info, None) } {
        Ok(pipelines) => Ok((pipelines[0], layout)),
        Err((_, e)) => Err(e.into()),
    }
}

fn create_layout<'a, I>(iter: I, device: &ash::Device, set_layouts: &[vk::DescriptorSetLayout]) -> VkResult<vk::PipelineLayout>
    where
        I: Iterator<Item=&'a spirv_reflect::ShaderModule>,
{
    let ranges = [vk::PushConstantRange::builder()
        .size(std::mem::size_of::<nalgebra::Matrix4<f32>>() as u32)
        .offset(0)
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .build()];

    let create_info = vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&ranges).set_layouts(set_layouts);
    //todo descriptor sets from reflection data
    unsafe { device.create_pipeline_layout(&create_info, None) }
}

/// Loads the pipeline cache from a file or creates a new empty cache if the file could not be read
fn load_cache(device: &ash::Device) -> VkResult<vk::PipelineCache> {
    let path = DIRS.project.cache_dir().join("pipeline_cache");
    if let Ok(data) = fs::read(&path) {
        let create_info = vk::PipelineCacheCreateInfo::builder().initial_data(&data);
        info!("Loading pipeline cache from {}", path.to_string_lossy());
        unsafe { Ok(device.create_pipeline_cache(&create_info, None)?) }
    } else {
        info!("Loading empty pipeline cache");
        unsafe { Ok(device.create_pipeline_cache(&Default::default(), None)?) }
    }
}

/// Saves the pipeline cache to disk and then destroys it.
///
/// Does nothing if the cache was never initialized
pub fn cleanup_cache(device: &ash::Device) {
    if let Some(cache) = CACHE.get() {
        unsafe {
            if let Ok(data) = device.get_pipeline_cache_data(*cache) {
                let path = DIRS.project.cache_dir().join("pipeline_cache");
                if let Err(e) = fs::write(&path, &data) {
                    error!("Failed to write pipeline cache to {path:?}, Error: {e}");
                } else {
                    info!("Saved pipeline cache to {path:?}");
                }
            }
            device.destroy_pipeline_cache(*cache, None);
        }
    }
}

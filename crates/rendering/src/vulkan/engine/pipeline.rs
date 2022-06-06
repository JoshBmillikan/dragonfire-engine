use std::error::Error;
use std::ffi::CString;
use std::fs;
use std::fs::File;
use std::path::Path;

use ash::prelude::VkResult;
use ash::vk;
use log::{error, info};
use once_cell::sync::OnceCell;
use scopeguard::defer;

use engine::filesystem::DIRS;

static CACHE: OnceCell<vk::PipelineCache> = OnceCell::new();

pub fn create_graphics_pipeline(
    device: &ash::Device,
    image_fmt: vk::Format,
    extent: vk::Extent2D,
) -> Result<(vk::Pipeline, vk::PipelineLayout), Box<dyn Error>> {
    let cache = CACHE.get_or_try_init(|| load_cache(device))?;
    let vert = load_spv(device, "base.vert.spv")?;
    let frag = load_spv(device, "base.frag.spv")?;
    defer! {
        unsafe {
            device.destroy_shader_module(vert, None);
            device.destroy_shader_module(frag, None);
        }
    }
    let name = CString::new("main")?;
    let stage_info = [vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::VERTEX)
        .module(vert)
        .name(&name)
        .build(),
        vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag)
            .name(&name)
            .build()
        ];

    let fmts = [image_fmt];
    let mut render_info =
        vk::PipelineRenderingCreateInfo::builder().color_attachment_formats(&fmts);

    let vert_input = vk::PipelineVertexInputStateCreateInfo::builder()
    //todo
        ;

    let input_asm = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .primitive_restart_enable(false)
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

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

    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewports(&viewport)
        .scissors(&scissor);

    let raster = vk::PipelineRasterizationStateCreateInfo::builder()
        .depth_clamp_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::CLOCKWISE)
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

    let layout_info = vk::PipelineLayoutCreateInfo::builder();
    let layout = unsafe { device.create_pipeline_layout(&layout_info, None)? };

    let create_info = [vk::GraphicsPipelineCreateInfo::builder()
        .push_next(&mut render_info)
        .layout(layout)
        .stages(&stage_info)
        .vertex_input_state(&vert_input)
        .input_assembly_state(&input_asm)
        .viewport_state(&viewport_state)
        .rasterization_state(&raster)
        .multisample_state(&multisample)
        .color_blend_state(&color)
        .render_pass(vk::RenderPass::null())
        .build()];

    match unsafe { device.create_graphics_pipelines(*cache, &create_info, None) } {
        Ok(pipelines) => Ok((pipelines[0], layout)),
        Err((_, e)) => Err(e.into()),
    }
}

fn load_spv(
    device: &ash::Device,
    filename: impl AsRef<Path>,
) -> Result<vk::ShaderModule, Box<dyn Error>> {
    let shader_path = DIRS.asset.join("shaders").join(filename);
    let mut file = File::open(&shader_path)?;
    let spv = ash::util::read_spv(&mut file)?;
    info!("Loaded shader {shader_path:?}");
    let create_info = vk::ShaderModuleCreateInfo::builder().code(&spv);
    Ok(unsafe { device.create_shader_module(&create_info, None)? })
}

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

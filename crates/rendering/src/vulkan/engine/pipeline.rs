use std::fs;

use ash::prelude::VkResult;
use ash::vk;
use log::{error, info};
use once_cell::sync::OnceCell;

use engine::filesystem::DIRS;

static CACHE: OnceCell<vk::PipelineCache> = OnceCell::new();

pub fn create_graphics_pipeline(
    device: &ash::Device,
    image_fmt: vk::Format,
) -> VkResult<vk::Pipeline> {
    let cache = CACHE.get_or_try_init(|| load_cache(device))?;

    let fmts = [image_fmt];
    let mut render_info =
        vk::PipelineRenderingCreateInfo::builder().color_attachment_formats(&fmts);

    let create_info = [vk::GraphicsPipelineCreateInfo::builder()
        .push_next(&mut render_info)
        //todo
        .render_pass(vk::RenderPass::null())
        .build()];

    match unsafe { device.create_graphics_pipelines(*cache, &create_info, None) } {
        Ok(pipelines) => Ok(pipelines[0]),
        Err((_, e)) => Err(e),
    }
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
                    error!("Failed to write pipeline cache to disk, Error: {e}");
                } else {
                    info!("Saved pipeline cache to {}", path.to_string_lossy());
                }
            }
            device.destroy_pipeline_cache(*cache, None);
        }
    }
}

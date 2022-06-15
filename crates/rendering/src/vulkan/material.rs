use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Weak};
use ash::{Device, vk};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use crate::vulkan::material::creation::load_material;
use crate::vulkan::material::database::CONN;

mod database;
mod creation;

pub struct Material {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    pub device: Arc<Device>
}
static CACHE: Lazy<Mutex<HashMap<String, Weak<Material>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

impl Material {
    pub fn new(
        name: impl Into<String>,
        device: &ash::Device,
        image_fmt: vk::Format,
        extent: vk::Extent2D,
    ) -> Result<Arc<Self>, Box<dyn Error>> {
        let name = name.into();
        let cache = CACHE.lock();
        if let Some(Some(mat)) = cache.get(name.as_str()).map(Weak::upgrade) {
            return Ok(mat);
        }
        drop(cache);

        let material = CONN.with(|conn| load_material(&name, device, image_fmt, extent, conn))?;
        let mut cache = CACHE.lock();
        cache.insert(name, Arc::downgrade(&material));
        Ok(material)
    }

    pub(super) fn get_pipeline_layout(&self) -> vk::PipelineLayout {
        self.layout
    }

    pub(super) unsafe fn bind(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        device.cmd_bind_pipeline(cmd,  vk::PipelineBindPoint::GRAPHICS, self.pipeline);
    }

}

impl Drop for Material {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            self.device.destroy_pipeline_layout(self.layout, None);
            self.device.destroy_pipeline(self.pipeline, None);
        }
    }
}
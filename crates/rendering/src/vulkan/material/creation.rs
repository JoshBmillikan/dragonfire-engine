use std::error::Error;
use std::sync::Arc;

use ash::vk;

use crate::vulkan::material::Material;

pub fn load_material(
    name: &String,
    device: &ash::Device,
    image_fmt: vk::Format,
    extent: vk::Extent2D,
) -> Result<Arc<Material>, Box<dyn Error>> {
    todo!()
}

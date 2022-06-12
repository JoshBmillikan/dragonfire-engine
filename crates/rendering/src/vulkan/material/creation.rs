use std::error::Error;
use std::sync::Arc;
use ash::vk;
use rusqlite::Connection;
use crate::vulkan::material::Material;

pub fn load_material(
    name: &String,
    device: &ash::Device,
    image_fmt: vk::Format,
    extent: vk::Extent2D,
    conn: &Connection,
) -> Result<Arc<Material>, Box<dyn Error>> {
    let mut stmt = conn.prepare_cached(include_str!("material_info.sql"))?;
    let rows = stmt.query([name])?;

    todo!()
}

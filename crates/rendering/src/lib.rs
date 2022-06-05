use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use nalgebra::{Matrix4, Perspective3};
use raw_window_handle::HasRawWindowHandle;
use uom::si::f32::Angle;
use serde::{Serialize, Deserialize};
use uom::si::angle::degree;


#[cfg(feature = "vulkan")]
mod vulkan {
    pub mod engine;
    pub(super) mod mesh;
    pub(super) mod material;
}

#[cfg(feature = "vulkan")]
type Material = vulkan::material::Material;
#[cfg(feature = "vulkan")]
type Mesh = vulkan::mesh::Mesh;

pub trait RenderingEngine {
    fn begin_rendering(&mut self, view: &nalgebra::Transform3<f32>, projection: &nalgebra::Perspective3<f32>);
    fn render(&mut self, mesh: &Arc<Mesh>,material: &Arc<Material>, transform: &nalgebra::Transform3<f32>);
    fn end_rendering(&mut self);
    fn resize(&mut self, width: u32, height: u32);
    fn load_model(&mut self, path: &Path) -> Result<Arc<Mesh>, Box<dyn Error>>;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphicsSettings {
    pub resolution: [u32; 2],
    pub fov: Angle,
}

#[cfg(feature = "vulkan")]
pub fn create_rendering_engine(window: &dyn HasRawWindowHandle, settings: &GraphicsSettings) -> Box<vulkan::engine::Engine> {
    Box::new(unsafe { vulkan::engine::Engine::new(window, settings).expect("Failed to initialize rendering engine")})
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        GraphicsSettings {
            resolution: [1920, 1080],
            fov: Angle::new::<degree>(45.)
        }
    }
}

fn cull_test(mesh: &Mesh, model: &Matrix4<f32>, view: &Matrix4<f32>, projection: &Perspective3<f32>) -> bool {
    // todo
    true
}
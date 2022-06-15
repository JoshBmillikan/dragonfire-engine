use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use nalgebra::{Isometry3, Matrix4, Perspective3};
use raw_window_handle::HasRawWindowHandle;
use serde::{Deserialize, Serialize};
use uom::si::angle::degree;
use uom::si::f32::Angle;

#[cfg(feature = "vulkan")]
mod vulkan {
    pub mod engine;
    pub(super) mod material;
    pub(super) mod mesh;
}

#[cfg(feature = "vulkan")]
pub type Material = vulkan::material::Material;
#[cfg(feature = "vulkan")]
pub type Mesh = vulkan::mesh::Mesh;

pub trait RenderingEngine {
    fn begin_rendering(&mut self, view: &Matrix4<f32>, projection: &Perspective3<f32>);
    fn render(&mut self, mesh: &Arc<Mesh>, material: &Arc<Material>, transform: Matrix4<f32>);
    fn end_rendering(&mut self);
    fn resize(&mut self, width: u32, height: u32);
    fn load_model(&mut self, path: &Path) -> Result<Arc<Mesh>, Box<dyn Error>>;
    fn load_material(&mut self) -> Result<Arc<Material>, Box<dyn Error>>;
    fn wait(&self);
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphicsSettings {
    pub resolution: [u32; 2],
    pub fov: Angle,
}

pub struct Camera {
    pub view: Isometry3<f32>,
    pub projection: Perspective3<f32>,
}

impl Camera {
    pub fn new(settings: &GraphicsSettings) -> Self {
        let projection = Perspective3::new(
            settings.resolution[0] as f32 / settings.resolution[1] as f32,
            //settings.fov.value,
            0.785398f32,
            0.1,
            1000.,
        );
        Camera {
            view: Default::default(),
            projection,
        }
    }
}

#[cfg(feature = "vulkan")]
pub fn create_rendering_engine(
    window: &dyn HasRawWindowHandle,
    settings: &GraphicsSettings,
) -> Box<vulkan::engine::Engine> {
    Box::new(unsafe {
        vulkan::engine::Engine::new(window, settings)
            .expect("Failed to initialize rendering engine")
    })
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        GraphicsSettings {
            resolution: [1920, 1080],
            fov: Angle::new::<degree>(45.),
        }
    }
}

fn cull_test(
    mesh: &Mesh,
    model: &Matrix4<f32>,
    view: &Matrix4<f32>,
    projection: &Perspective3<f32>,
) -> bool {
    // todo
    true
}

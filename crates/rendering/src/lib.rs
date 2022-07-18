extern crate core;

use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use nalgebra::{Isometry3, Matrix4, Orthographic3, Perspective3};
use raw_window_handle::HasRawWindowHandle;
use serde::{Deserialize, Serialize};
use uom::si::angle::degree;
use uom::si::f32::Angle;

#[cfg(feature = "vulkan")]
mod vulkan {
    pub mod engine;
    pub(super) mod material;
    pub(super) mod mesh;
    pub(crate) mod texture;
}

#[cfg(feature = "vulkan")]
pub type Material = vulkan::material::Material;
#[cfg(feature = "vulkan")]
pub type Mesh = vulkan::mesh::Mesh;

pub trait RenderingEngine {
    fn begin_rendering(&mut self, camera: &Camera);
    fn render(&mut self, mesh: &Arc<Mesh>, material: &Arc<Material>, transform: Matrix4<f32>);
    fn end_rendering(&mut self);
    fn resize(&mut self, width: u32, height: u32);
    fn load_model(&mut self, path: &Path) -> Result<Arc<Mesh>, Box<dyn Error>>;
    fn load_material(&mut self) -> Result<Arc<Material>, Box<dyn Error>>;
    fn wait(&self);
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct GraphicsSettings {
    pub resolution: [u32; 2],
    pub fov: Angle,
    pub vsync: bool,
}

pub struct Camera {
    pub view: Isometry3<f32>,
    pub projection: Perspective3<f32>,
    pub orthographic: Orthographic3<f32>,
}

impl Camera {
    pub fn new(width:u32, height: u32, fov: Angle) -> Self {
        let projection = Perspective3::new(
            width as f32 / height as f32,
            fov.value,
            0.1,
            1000.,
        );
        let orthographic = Orthographic3::new(
            0.,
            width as f32,
            0.,
            height as f32,
            0.1,
            1000.,
        );
        Camera {
            view: Default::default(),
            projection,
            orthographic,
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
            resolution: [800, 600],
            fov: Angle::new::<degree>(45.),
            vsync: true,
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

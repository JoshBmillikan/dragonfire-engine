use raw_window_handle::HasRawWindowHandle;
use engine::uom::si::f32::Angle;
use serde::{Serialize, Deserialize};
use engine::uom::si::angle::degree;
use engine::nalgebra;

mod vulkan {
    pub mod engine;
}

pub trait RenderingEngine {
    fn begin_rendering(&mut self, view: &nalgebra::Transform3<f32>, projection: &nalgebra::Projective3<f32>);
    fn render(&mut self);
    fn end_rendering(&mut self);
    fn resize(&mut self, width: u32, height: u32);
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphicsSettings {
    pub resolution: [u32; 2],
    pub title: String,
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
            title: "Dragonfire Engine".to_string(),
            fov: Angle::new::<degree>(45.)
        }
    }
}
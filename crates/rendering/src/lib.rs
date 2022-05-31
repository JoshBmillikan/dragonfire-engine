use raw_window_handle::HasRawWindowHandle;
use engine::uom::si::f32::Angle;
use serde::{Serialize, Deserialize};

mod vulkan {
    pub mod engine;
}

pub trait RenderingEngine {

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
        todo!()
    }
}
use std::error::Error;
use ash::vk;

pub struct Material {

}

impl Material {
    pub(super) fn new() -> Result<Self, Box<dyn Error>> {

        todo!()
    }

    pub(super) fn get_pipeline_layout(&self) -> vk::PipelineLayout {
        todo!()
    }

    pub(super) unsafe fn bind(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        todo!()
    }

}
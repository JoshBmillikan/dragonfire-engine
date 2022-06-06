use std::sync::Arc;
use ash::{Device, vk};

pub struct Material {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    pub device: Arc<Device>
}

impl Material {

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
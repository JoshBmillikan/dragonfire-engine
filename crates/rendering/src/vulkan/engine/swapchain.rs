use std::sync::Arc;
use ash::prelude::VkResult;
use ash::vk;
use smallvec::SmallVec;

pub struct Swapchain {
    pub swapchain: vk::SwapchainKHR,
    pub loader: Arc<ash::extensions::khr::Swapchain>,
    pub images: SmallVec<[vk::Image;8]>,
    pub views: SmallVec<[vk::ImageView;8]>,
    pub extent: vk::Extent2D,
    pub current_image_index: usize,
    pub device: Arc<ash::Device>
}

impl Swapchain {
    pub unsafe fn next(&mut self, semaphore: vk::Semaphore) -> VkResult<bool> {
        self.loader.acquire_next_image(self.swapchain, u64::MAX, semaphore, vk::Fence::null())
            .map(|(index, suboptimal)| {
                self.current_image_index = index as usize;
                suboptimal
            })
    }

    #[inline]
    pub fn get_current_image(&self) -> vk::Image {
        self.images[self.current_image_index]
    }

    #[inline]
    pub fn get_current_image_view(&self) -> vk::ImageView {
        self.views[self.current_image_index]
    }
}

impl Drop for Swapchain {
    fn drop(&mut self) {
        unsafe {
            for view in &self.views {
                self.device.destroy_image_view(*view, None);
            }
            self.loader.destroy_swapchain(self.swapchain,None);
        }
    }
}
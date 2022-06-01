use std::error::Error;
use ash::prelude::VkResult;
use ash::vk;
use log::warn;
use once_cell::sync::OnceCell;
use vk_mem::{Allocator, AllocatorCreateInfo};

static mut ALLOCATOR: OnceCell<Allocator> = OnceCell::new();

pub(super) fn create_allocator(
    entry: &ash::Entry,
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
) -> VkResult<()> {
    let create_info = AllocatorCreateInfo {
        entry: entry.clone(),
        physical_device,
        device: device.clone(),
        instance: instance.clone(),
        flags: vk_mem::AllocatorCreateFlags::EXT_MEMORY_BUDGET,
        preferred_large_heap_block_size: 0,
        heap_size_limits: None,
        allocation_callbacks: None,
        vulkan_api_version: vk::API_VERSION_1_3,
    };
    unsafe {
        let alloc = Allocator::new(&create_info)?;
        ALLOCATOR.get_or_init(|| alloc);
    }
    Ok(())
}

/// Destroys the global allocator object
///
/// # Safety
/// Mutates a global variable
pub(super) unsafe fn destroy_allocator() {
    if let Some(mut alloc) = ALLOCATOR.take() {
        alloc.destroy();
    } else {
        warn!("Attempted to destroy allocator, but it was not initialized");
    }
}

pub struct Buffer {
    allocation: vk_mem::Allocation,
    info: vk_mem::AllocationInfo,
    buffer: vk::Buffer,
}

impl Buffer {
    pub fn new() -> Result<Buffer, Box<dyn Error>> {
        todo!()
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            ALLOCATOR.get().expect("Allocator not initialized").destroy_buffer(self.buffer, self.allocation);
        }
    }
}
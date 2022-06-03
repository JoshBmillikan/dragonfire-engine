use std::error::Error;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use ash::prelude::VkResult;
use ash::vk;
use ash::vk::DeviceSize;
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

#[derive(Debug, Clone)]
struct AllocData {
    allocation: vk_mem::Allocation,
    info: vk_mem::AllocationInfo,
}

unsafe impl Send for AllocData {}
unsafe impl Sync for AllocData {}

#[derive(Debug)]
pub struct Buffer {
    allocation: AllocData,
    buffer: vk::Buffer,
}

impl Buffer {
    pub unsafe fn new(
        create_info: &vk::BufferCreateInfo,
        alloc_info: &vk_mem::AllocationCreateInfo,
    ) -> Result<Buffer, Box<dyn Error>> {
        let (buffer, allocation, info) = ALLOCATOR
            .get()
            .ok_or("Allocator not initialized")?
            .create_buffer(create_info, alloc_info)?;

        Ok(Buffer {
            allocation: AllocData { allocation, info },
            buffer
        })
    }
    
    pub fn get_info(&self) -> &vk_mem::AllocationInfo {
        &self.allocation.info
    }
}

impl Deref for Buffer {
    type Target = vk::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            ALLOCATOR
                .get()
                .expect("Allocator not initialized")
                .destroy_buffer(self.buffer, self.allocation.allocation);
        }
    }
}

#[derive(Debug)]
pub struct GpuObject<T: Sized> {
    buffer: Buffer,
    _spooky: PhantomData<T>,
}

impl<T> GpuObject<T> {
    pub fn new(usage: vk::BufferUsageFlags) -> Result<Self, Box<dyn Error>>{
        let create_info = vk::BufferCreateInfo::builder()
            .size(std::mem::size_of::<T>() as DeviceSize)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .usage(usage);
        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuToGpu,
            flags: vk_mem::AllocationCreateFlags::MAPPED,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            ..Default::default()
        };
        let buffer = unsafe {Buffer::new(&create_info, &alloc_info)?};
        Ok(GpuObject {
            buffer,
            _spooky: Default::default()
        })
    }
}

impl<T> Deref for GpuObject<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.buffer.allocation.info.get_mapped_data() as *const T) }
    }
}

impl<T> DerefMut for GpuObject<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self.buffer.allocation.info.get_mapped_data() as *mut T) }
    }
}

use crate::vulkan::engine::alloc::{Buffer, Image};
use ash::vk;
use ash::vk::DeviceSize;
use png::Decoder;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use ash::prelude::VkResult;
use vk_mem::Allocator;
use anyhow::Result;

pub struct Texture {
    pub(super) image: Image,
    pub(super) view: vk::ImageView,
    pub(super) sampler: vk::Sampler,
    device: Arc<ash::Device>,
}

impl Texture {
    pub fn new(
        path: impl AsRef<Path>,
        device: Arc<ash::Device>,
        cmd: vk::CommandBuffer,
        queue: vk::Queue,
        anisotropy: f32,
        allocator: Arc<Allocator>,
    ) -> Result<Self> {
        let decoder = Decoder::new(File::open(path)?);
        let mut reader = decoder.read_info()?;
        let size = reader.output_buffer_size();
        let staging_info = vk::BufferCreateInfo::builder()
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .size(size as DeviceSize)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging_alloc_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuToGpu,
            flags: vk_mem::AllocationCreateFlags::MAPPED,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT,
            ..Default::default()
        };
        let staging_buffer =
            unsafe { Buffer::new(&staging_info, &staging_alloc_info, allocator.clone())? };
        let ptr = staging_buffer.get_info().get_mapped_data();
        let info = reader.next_frame(unsafe { std::slice::from_raw_parts_mut(ptr, size) })?;

        let ext = vk::Extent3D {
            width: info.width,
            height: info.height,
            depth: 1,
        };
        let create_info = vk::ImageCreateInfo::builder()
            .extent(ext)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_SRGB)
            .tiling(vk::ImageTiling::OPTIMAL)
            .mip_levels(1)
            .array_layers(1)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1);
        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::GpuOnly,
            required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
            ..Default::default()
        };
        let sub_range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        unsafe {
            let image = Image::new(&create_info, &alloc_info, allocator)?;
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device.begin_command_buffer(cmd, &begin_info)?;
            let barrier = [vk::ImageMemoryBarrier::builder()
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .image(*image)
                .subresource_range(sub_range)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .build()];
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barrier,
            );
            let cpy = [vk::BufferImageCopy::builder()
                .buffer_image_height(0)
                .buffer_offset(0)
                .buffer_row_length(0)
                .image_subresource(vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image_offset(vk::Offset3D::default())
                .image_extent(ext)
                .build()];
            let barrier = [vk::ImageMemoryBarrier::builder()
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image(*image)
                .subresource_range(sub_range)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .build()];
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &barrier,
            );
            device.cmd_copy_buffer_to_image(
                cmd,
                *staging_buffer,
                *image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &cpy,
            );

            device.end_command_buffer(cmd)?;
            let submit_info = [vk::SubmitInfo::builder().command_buffers(&[cmd]).build()];
            device.queue_submit(queue, &submit_info, vk::Fence::null())?;
            device.queue_wait_idle(queue)?;
            let view_info = vk::ImageViewCreateInfo::builder()
                .image(*image)
                .format(vk::Format::R8G8B8A8_SRGB)
                .view_type(vk::ImageViewType::TYPE_2D)
                .subresource_range(sub_range);
            let view = device.create_image_view(&view_info, None)?;
            let sampler = create_sampler(&device, anisotropy)?;
            Ok(Texture {
                image,
                view,
                sampler,
                device,
            })
        }
    }
}

unsafe fn create_sampler(device: &ash::Device, anisotropy: f32) -> VkResult<vk::Sampler> {
    let create_info = vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::REPEAT)
        .address_mode_v(vk::SamplerAddressMode::REPEAT)
        .address_mode_w(vk::SamplerAddressMode::REPEAT)
        .anisotropy_enable(true)
        .max_anisotropy(anisotropy)
        .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
        .unnormalized_coordinates(false)
        .compare_enable(false) // todo
        .compare_op(vk::CompareOp::ALWAYS)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .mip_lod_bias(0.)
        .min_lod(0.)
        .max_lod(0.); // todo mip mapping
    device.create_sampler(&create_info, None)
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_sampler(self.sampler, None);
            self.device.destroy_image_view(self.view, None);
        }
    }
}

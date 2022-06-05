use std::error::Error;
use std::ptr::copy_nonoverlapping;
use ash::vk;
use ash::vk::DeviceSize;
use crate::vulkan::engine::alloc::Buffer;

pub struct Mesh {
    indices: Vec<u32>,
    _vertices: Vec<Vertex>,
    vertex_buffer: Buffer,
    index_buffer: Buffer
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Vertex {
    pub position: nalgebra::Vector3<f32>,
    pub normal: nalgebra::UnitVector3<f32>,
    //uv: nalgebra::Vector2<f32>
}

impl Mesh {
    pub fn new(vertices: Vec<Vertex>, indices: Vec<u32>, device: &ash::Device, cmd: vk::CommandBuffer, queue: vk::Queue) -> Result<Self, Box<dyn Error>> {
        let vertex_size = std::mem::size_of::<Vertex>() * vertices.len();
        let index_size = std::mem::size_of::<u32>() * indices.len();

        let create_info = vk::BufferCreateInfo::builder()
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .size((vertex_size + index_size) as DeviceSize)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuToGpu,
            flags: vk_mem::AllocationCreateFlags::MAPPED,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            ..Default::default()
        };
        unsafe {
            let staging_buf = Buffer::new(&create_info, &alloc_info)?;
            let ptr = staging_buf.get_info().get_mapped_data();
            copy_nonoverlapping(vertices.as_ptr() as *const u8, ptr, vertex_size);
            copy_nonoverlapping(indices.as_ptr() as *const u8, ptr.add(vertex_size), index_size);

            let alloc_info = vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                ..Default::default()
            };

            let create_info = vk::BufferCreateInfo::builder()
                .usage(vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::VERTEX_BUFFER)
                .size(vertex_size as DeviceSize)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let vertex_buffer = Buffer::new(&create_info, &alloc_info)?;

            let create_info = vk::BufferCreateInfo::builder()
                .usage(vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::INDEX_BUFFER)
                .size(index_size as DeviceSize)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let index_buffer = Buffer::new(&create_info, &alloc_info)?;

            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device.begin_command_buffer(cmd, &begin_info)?;
            let cpy = [vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: vertex_size as DeviceSize
            }];
            device.cmd_copy_buffer(cmd, *staging_buf, *vertex_buffer, &cpy);
            let cpy = [vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: index_size as DeviceSize
            }];
            device.cmd_copy_buffer(cmd, *staging_buf, *index_buffer, &cpy);
            device.end_command_buffer(cmd)?;

            let submit_info = [
                vk::SubmitInfo::builder()
                    .command_buffers(&[cmd])
                    .build()
            ];
            device.queue_submit(queue, &submit_info, vk::Fence::null())?;
            device.queue_wait_idle(queue)?;

            Ok(Mesh {
                indices,
                _vertices: vertices,
                vertex_buffer,
                index_buffer
            })
        }
    }

    pub(super) unsafe fn bind(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        device.cmd_bind_index_buffer(cmd, *self.index_buffer, 0, vk::IndexType::UINT32);
        let bufs = [*self.vertex_buffer];
        device.cmd_bind_vertex_buffers(cmd, 0, &bufs, &[0]);
    }

    #[inline]
    pub(super) fn get_index_count(&self) -> u32 {
        self.indices.len() as u32
    }
}
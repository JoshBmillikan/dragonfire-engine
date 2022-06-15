use std::error::Error;
use std::ptr::copy_nonoverlapping;
use std::sync::Arc;

use ash::vk;
use ash::vk::DeviceSize;
use log::trace;
use memoffset::offset_of;
use vk_mem::Allocator;

use crate::vulkan::engine::alloc::Buffer;

pub struct Mesh {
    indices: Vec<u32>,
    _vertices: Vec<Vertex>,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct Vertex {
    pub position: nalgebra::Vector3<f32>,
    pub normal: nalgebra::UnitVector3<f32>,
    //uv: nalgebra::Vector2<f32>
}

impl Mesh {
    /// Creates a new mesh representing a 3d model.
    ///
    /// vertices and indices are immediately copied to the gpu,
    /// blocking until the queue submission is finished.
    ///
    /// # Arguments
    ///
    /// * `vertices`: vertices of the model
    /// * `indices`: model indices
    /// * `device`: device handle
    /// * `cmd`: command buffer to run the copy commands
    /// * `queue`: queue to submit the copy commands to
    /// * `allocator`: allocator to use when allocating the gpu buffers
    ///
    /// returns: Result<Mesh, Box<dyn Error, Global>>
    pub fn new(
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        queue: vk::Queue,
        allocator: Arc<Allocator>,
    ) -> Result<Self, Box<dyn Error>> {
        let vertex_size = std::mem::size_of::<Vertex>() * vertices.len();
        let index_size = std::mem::size_of::<u32>() * indices.len();

        let create_info = vk::BufferCreateInfo::builder()
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .size((vertex_size + index_size) as DeviceSize)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let alloc_info = vk_mem::AllocationCreateInfo {
            usage: vk_mem::MemoryUsage::CpuToGpu,
            flags: vk_mem::AllocationCreateFlags::MAPPED,
            required_flags: vk::MemoryPropertyFlags::HOST_VISIBLE
                | vk::MemoryPropertyFlags::HOST_COHERENT,
            ..Default::default()
        };
        unsafe {
            let staging_buf = Buffer::new(&create_info, &alloc_info, allocator.clone())?;
            let ptr = staging_buf.get_info().get_mapped_data();

            // copy vertices and indices into the staging buffer
            // Vertices are stored first, indices are stored immediately after in them buffer
            copy_nonoverlapping(vertices.as_ptr() as *const u8, ptr, vertex_size);
            copy_nonoverlapping(
                indices.as_ptr() as *const u8,
                ptr.add(vertex_size),
                index_size,
            );

            let alloc_info = vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::GpuOnly,
                required_flags: vk::MemoryPropertyFlags::DEVICE_LOCAL,
                ..Default::default()
            };

            let create_info = vk::BufferCreateInfo::builder()
                .usage(vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::VERTEX_BUFFER)
                .size(vertex_size as DeviceSize)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let vertex_buffer = Buffer::new(&create_info, &alloc_info, allocator.clone())?;

            let create_info = vk::BufferCreateInfo::builder()
                .usage(vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::INDEX_BUFFER)
                .size(index_size as DeviceSize)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);
            let index_buffer = Buffer::new(&create_info, &alloc_info, allocator)?;

            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            device.begin_command_buffer(cmd, &begin_info)?;
            // vertices copy
            let cpy = [vk::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: vertex_size as DeviceSize,
            }];
            device.cmd_copy_buffer(cmd, *staging_buf, *vertex_buffer, &cpy);
            // indices copy
            let cpy = [vk::BufferCopy {
                src_offset: vertex_size as DeviceSize,
                dst_offset: 0,
                size: index_size as DeviceSize,
            }];
            device.cmd_copy_buffer(cmd, *staging_buf, *index_buffer, &cpy);
            device.end_command_buffer(cmd)?;

            let submit_info = [vk::SubmitInfo::builder().command_buffers(&[cmd]).build()];
            device.queue_submit(queue, &submit_info, vk::Fence::null())?;
            device.queue_wait_idle(queue)?;

            trace!(
                "Loaded model with {} vertices, {} indices",
                vertices.len(),
                indices.len()
            );
            Ok(Mesh {
                indices,
                _vertices: vertices,
                vertex_buffer,
                index_buffer,
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

impl Vertex {
    /// Gets the vertex input and attribute descriptions
    pub(crate) fn get_vertex_description() -> (Vec<vk::VertexInputBindingDescription>, Vec<vk::VertexInputAttributeDescription>) {
        let input = vec![
            vk::VertexInputBindingDescription::builder()
                .binding(0)
                .stride(std::mem::size_of::<Vertex>() as u32)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build()
        ];

        let attributes = vec! [
            vk::VertexInputAttributeDescription::builder()
                .binding(0)
                .location(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(offset_of!(Vertex, position) as u32)
                .build(),

            vk::VertexInputAttributeDescription::builder()
                .binding(0)
                .location(1)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(offset_of!(Vertex, normal) as u32)
                .build()
        ];

        (input, attributes)
    }
}
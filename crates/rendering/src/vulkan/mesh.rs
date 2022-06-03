use ash::vk;
use crate::vulkan::engine::alloc::Buffer;

pub struct Mesh {
    indices: Vec<u32>,
    _vertices: Vec<Vertex>,
    vertex_buffer: Buffer,
    index_buffer: Buffer
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
struct Vertex {
    position: nalgebra::Vector3<f32>,
    uv: nalgebra::Vector2<f32>
}

impl Mesh {
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
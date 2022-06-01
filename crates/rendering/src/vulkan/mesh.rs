use ash::vk;

pub struct Mesh {
    pub indices: Vec<u32>,
    vertices: Vec<Vertex>
}

struct Vertex {

}

impl Mesh {
    pub(super) unsafe fn bind(&self, device: &ash::Device, cmd: vk::CommandBuffer) {
        todo!()
    }
}
use rustaria_core::mesh::Vertex;
use wgpu::util::DeviceExt;

// ─────────────────────────────────────────────
// GpuMesh : vertex buffer + index buffer uploadés sur le GPU
// Isole les ressources GPU du mesh du reste du GameState
// Permet plus tard de gérer plusieurs chunks facilement (Vec<GpuMesh>)
// ─────────────────────────────────────────────
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
}

impl GpuMesh {
    /// Upload des vertices et des indices sur le GPU
    pub fn new(device: &wgpu::Device, vertices: &[Vertex], indices: &[u32]) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let num_indices = indices.len() as u32;

        Self {
            vertex_buffer,
            index_buffer,
            num_indices,
        }
    }
}

use wgpu::BufferUsages;

use super::buffer::Buffer;
use super::pipeline::Vertex;

pub struct Mesh<V: Vertex> {
    vertices: Buffer<V>,
    indices: Buffer<u16>,
    vertex_size: usize,
    index_size: usize,
}

impl<V: Vertex> Mesh<V> {
    pub fn new(device: &wgpu::Device, vertex_capacity: usize, index_capacity: usize) -> Self {
        let vertices =
            Buffer::new(device, vertex_capacity, BufferUsages::VERTEX | BufferUsages::COPY_DST);
        let indices =
            Buffer::new(device, index_capacity, BufferUsages::INDEX | BufferUsages::COPY_DST);

        Self { vertices, indices, vertex_size: 0, index_size: 0 }
    }

    pub fn append(&mut self, queue: &wgpu::Queue, verts: &[V], idxs: &[u16]) {
        let vertex_offset = self.vertex_size as u16;
        let offset_idxs: Vec<u16> = idxs.iter().map(|i| i + vertex_offset).collect();

        self.vertices.write(queue, self.vertex_size, verts);
        self.indices.write(queue, self.index_size, &offset_idxs);

        self.vertex_size += verts.len();
        self.index_size += idxs.len();
    }

    pub fn vertices(&self) -> &Buffer<V> {
        &self.vertices
    }

    pub fn indices(&self) -> &Buffer<u16> {
        &self.indices
    }

    pub fn append_triangle(&mut self, queue: &wgpu::Queue, v0: V, v1: V, v2: V) {
        self.append(queue, &[v0, v1, v2], &[0, 1, 2]);
    }

    pub fn append_quad(&mut self, queue: &wgpu::Queue, v0: V, v1: V, v2: V, v3: V) {
        self.append(queue, &[v0, v1, v2, v3], &[0, 1, 2, 0, 2, 3]);
    }

    pub fn vertex_size(&self) -> usize {
        self.vertex_size
    }

    pub fn vertex_capacity(&self) -> usize {
        self.vertices.size()
    }

    pub fn index_size(&self) -> usize {
        self.index_size
    }

    pub fn index_capacity(&self) -> usize {
        self.indices.size()
    }

    pub fn reset(&mut self) {
        self.vertex_size = 0;
        self.index_size = 0;
    }
}

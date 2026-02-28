use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec4};

use super::pipeline::Vertex;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex2d {
    pub position: Vec2,
    pub uv: Vec2,
    pub color: Vec4,
}

impl Vertex2d {
    const ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
    ];
}

impl Vertex for Vertex2d {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRS,
        }
    }
}

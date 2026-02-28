use std::mem::size_of;
use std::num::NonZeroU64;
use std::ops::Range;

use anyhow::Result;
use bytemuck::Pod;

use super::buffer::Buffer;
use super::mesh::Mesh;
use super::pipeline::{ActiveRenderPass, Pipeline};
use super::texture::Texture;
use super::vertex2d::Vertex2d;

pub struct Pipeline2dTextured<U: Pod> {
    pipeline: Pipeline<Vertex2d>,
    bgl: wgpu::BindGroupLayout,
    uniform_buf: Buffer<U>,
    uniform_stride: u32,
}

impl<U: Pod> Pipeline2dTextured<U> {
    pub fn new(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        vs_entry: &str,
        fs_entry: &str,
        format: wgpu::TextureFormat,
        capacity: usize,
    ) -> Result<Self> {
        let alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
        let uniform_buf = Buffer::new_aligned(
            device,
            capacity,
            alignment,
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let uniform_stride = uniform_buf.stride() as u32;

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: Some(NonZeroU64::new(size_of::<U>() as u64).ok_or_else(|| anyhow::anyhow!("uniform type U has zero size"))?),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline =
            Pipeline::new(device, &pipeline_layout, shader, vs_entry, fs_entry, format);

        Ok(Self { pipeline, bgl, uniform_buf, uniform_stride })
    }

    pub fn begin_render_pass<'pass, 'res>(
        &'res self,
        device: &'res wgpu::Device,
        encoder: &'pass mut wgpu::CommandEncoder,
        view: &'pass wgpu::TextureView,
        clear_color: Option<wgpu::Color>,
    ) -> ActiveTexturedRenderPass<'pass, 'res, U> {
        let pass = self.pipeline.begin_render_pass(encoder, view, clear_color);
        ActiveTexturedRenderPass {
            pass,
            device,
            bgl: &self.bgl,
            uniform_buf: &self.uniform_buf,
            slot: 0,
            uniform_stride: self.uniform_stride,
        }
    }
}

pub struct ActiveTexturedRenderPass<'pass, 'res, U: Pod> {
    pass: ActiveRenderPass<'pass, Vertex2d>,
    device: &'res wgpu::Device,
    bgl: &'res wgpu::BindGroupLayout,
    uniform_buf: &'res Buffer<U>,
    slot: u32,
    uniform_stride: u32,
}

impl<'pass, 'res, U: Pod> ActiveTexturedRenderPass<'pass, 'res, U> {
    pub fn set_draw_state(&mut self, queue: &wgpu::Queue, uniforms: U, texture: &Texture) {
        self.uniform_buf.write(queue, self.slot as usize, &[uniforms]);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.uniform_buf.buffer,
                        offset: 0,
                        size: Some(NonZeroU64::new(size_of::<U>() as u64).expect("U is non-zero-sized")),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&texture.sampler),
                },
            ],
        });
        self.pass.set_bind_group(0, &bind_group, &[self.slot * self.uniform_stride]);
        self.slot += 1;
    }

    pub fn set_viewport(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.pass.set_viewport(x, y, w, h);
    }

    pub fn draw(&mut self, mesh: &Mesh<Vertex2d>, index_range: Range<u32>) {
        self.pass.draw(mesh, index_range);
    }
}

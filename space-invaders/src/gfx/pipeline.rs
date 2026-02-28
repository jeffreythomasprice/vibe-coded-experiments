use std::marker::PhantomData;
use std::ops::Range;

use bytemuck::Pod;

use super::mesh::Mesh;

pub trait Vertex: Pod {
    fn layout() -> wgpu::VertexBufferLayout<'static>;
}

pub struct Pipeline<V: Vertex> {
    pipeline: wgpu::RenderPipeline,
    _phantom: PhantomData<V>,
}

impl<V: Vertex> Pipeline<V> {
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::PipelineLayout,
        shader: &wgpu::ShaderModule,
        vs_entry: &str,
        fs_entry: &str,
        format: wgpu::TextureFormat,
    ) -> Self {
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: vs_entry,
                buffers: &[V::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: fs_entry,
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            _phantom: PhantomData,
        }
    }

    pub fn begin_render_pass<'pass>(
        &self,
        encoder: &'pass mut wgpu::CommandEncoder,
        view: &'pass wgpu::TextureView,
        clear_color: Option<wgpu::Color>,
    ) -> ActiveRenderPass<'pass, V> {
        let load = match clear_color {
            Some(color) => wgpu::LoadOp::Clear(color),
            None => wgpu::LoadOp::Load,
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);

        ActiveRenderPass {
            pass,
            _phantom: PhantomData,
        }
    }
}

pub struct ActiveRenderPass<'pass, V: Vertex> {
    pass: wgpu::RenderPass<'pass>,
    _phantom: PhantomData<V>,
}

impl<'pass, V: Vertex> ActiveRenderPass<'pass, V> {
    pub fn set_bind_group(&mut self, index: u32, bind_group: &wgpu::BindGroup, offsets: &[wgpu::DynamicOffset]) {
        self.pass.set_bind_group(index, bind_group, offsets);
    }

    pub fn set_viewport(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.pass.set_viewport(x, y, w, h, 0.0, 1.0);
    }

    pub fn draw(&mut self, mesh: &Mesh<V>, index_range: Range<u32>) {
        self.pass.set_vertex_buffer(0, mesh.vertices().buffer.slice(..));
        self.pass
            .set_index_buffer(mesh.indices().buffer.slice(..), wgpu::IndexFormat::Uint16);
        self.pass.draw_indexed(index_range, 0, 0..1);
    }
}

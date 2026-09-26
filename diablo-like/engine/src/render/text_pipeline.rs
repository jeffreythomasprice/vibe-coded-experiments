use wgpu::util::DeviceExt;

use crate::text::{Atlas, TextVertex};

/// Overlay-pass text pipeline: straight alpha-blended, unlit, drawn at
/// native resolution directly to the swapchain so bitmap glyphs are never
/// resampled by the world pass's render scale.
pub struct TextPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl TextPipeline {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        atlas_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/text.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text pipeline layout"),
            bind_group_layouts: &[Some(atlas_bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[TextVertex::layout()],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self { pipeline }
    }

    pub fn draw<'pass>(
        &'pass self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'pass>,
        atlas: &'pass Atlas,
        vertices: &[TextVertex],
    ) {
        if vertices.is_empty() {
            return;
        }

        // Rebuilt every call: HUD-scale text is a trivial amount of
        // geometry, so a per-frame buffer is far simpler than a persistent
        // one sized for the worst case.
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("text vertices"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }
}

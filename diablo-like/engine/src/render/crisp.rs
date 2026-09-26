use glam::Vec2;
use wgpu::util::DeviceExt;

use super::camera_ubo::CameraUbo;
use crate::geom::tess::Mesh;

/// A small filled-circle fan mesh centered at `center` — used for the
/// physics actor marker, rebuilt fresh each frame at its current position
/// (see app.rs). Deliberately hand-triangulated rather than routed through
/// `geom::contour`/`geom::tess`: a regular convex fan needs no boolean ops
/// or general polygon tessellation.
pub fn circle_marker_mesh(center: Vec2, radius: f32, segments: u32) -> Mesh {
    let mut vertices = Vec::with_capacity(segments as usize + 1);
    vertices.push(center);
    for i in 0..segments {
        let a = 2.0 * std::f32::consts::PI * i as f32 / segments as f32;
        vertices.push(center + Vec2::new(a.cos(), a.sin()) * radius);
    }

    let mut indices = Vec::with_capacity(segments as usize * 3);
    for i in 0..segments {
        let a = 1 + i;
        let b = 1 + (i + 1) % segments;
        indices.extend_from_slice(&[0, a, b]);
    }

    Mesh { vertices, indices }
}

/// A thin quad from `a` to `b` — debug visualization for a physics contact
/// normal (see app.rs), drawn fresh each frame from the current tick's
/// contact list.
pub fn line_segment_mesh(a: Vec2, b: Vec2, width: f32) -> Mesh {
    let dir = (b - a).normalize_or_zero();
    let side = Vec2::new(-dir.y, dir.x) * (width * 0.5);
    Mesh {
        vertices: vec![a - side, a + side, b + side, b - side],
        indices: vec![0, 1, 2, 0, 2, 3],
    }
}

/// A tessellated mesh (fill or stroke) uploaded to the GPU. Positions only —
/// crisp mode is deliberately flat-shaded, no per-vertex color.
pub struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl GpuMesh {
    pub fn new(device: &wgpu::Device, mesh: &Mesh) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("crisp mesh vertices"),
            contents: bytemuck::cast_slice(&mesh.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("crisp mesh indices"),
            contents: bytemuck::cast_slice(&mesh.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Self {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
        }
    }
}

/// Draws the exact boolean-composed geometry (see `geom::contour` /
/// `geom::tess`) flat-shaded and sharp-edged — crisp mode, and the exact
/// contour stroke shared with glow mode as the cross-check against the
/// baked field (see the design plan's Frame graph note).
pub struct CrispPipeline {
    fill_pipeline: wgpu::RenderPipeline,
    stroke_pipeline: wgpu::RenderPipeline,
}

impl CrispPipeline {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat, camera: &CameraUbo) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("crisp shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/crisp.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("crisp pipeline layout"),
            bind_group_layouts: &[Some(&camera.bind_group_layout)],
            immediate_size: 0,
        });

        let vertex_buffers = [wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vec2>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2],
        }];

        let fill_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("crisp fill pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_fill"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let stroke_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("crisp stroke pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_stroke"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            fill_pipeline,
            stroke_pipeline,
        }
    }

    // `mesh` is deliberately NOT tied to `'pass`: renderer-owned meshes
    // (the fixture's fill/stroke) live that long anyway, but the physics
    // debug markers (app.rs) are rebuilt fresh every frame as locals
    // scoped to the render call, shorter-lived than the pass itself — the
    // same reason `TextPipeline::draw`'s per-frame vertex buffer isn't
    // tied to `'pass` either.
    pub fn draw_fill<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass CameraUbo,
        mesh: &GpuMesh,
    ) {
        self.draw(pass, &self.fill_pipeline, camera, mesh);
    }

    pub fn draw_stroke<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        camera: &'pass CameraUbo,
        mesh: &GpuMesh,
    ) {
        self.draw(pass, &self.stroke_pipeline, camera, mesh);
    }

    fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        pipeline: &'pass wgpu::RenderPipeline,
        camera: &'pass CameraUbo,
        mesh: &GpuMesh,
    ) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &camera.bind_group, &[]);
        pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..mesh.index_count, 0, 0..1);
    }
}

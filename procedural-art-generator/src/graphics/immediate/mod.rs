mod vertex;

pub use vertex::{Color, ColorVertex2D, TextureVertex2D};

use glam::Mat4;

use super::wgpu_utils::{Buffer, Texture};

// ---------------------------------------------------------------------------
// ImmediateRenderer
// ---------------------------------------------------------------------------

pub struct ImmediateRenderer {
    color_pipeline: wgpu::RenderPipeline,
    color_bind_group_layout: wgpu::BindGroupLayout,
    texture_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: Buffer,
    color_vertex_buffer: Buffer,
    color_index_buffer: Buffer,
    texture_vertex_buffer: Buffer,
    texture_index_buffer: Buffer,
}

impl ImmediateRenderer {
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        // --- Color pipeline (position + color, no texture) ---

        let color_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("immediate_color_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("color_shader.wgsl").into()),
        });

        let color_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("immediate_color_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let color_pipeline = Self::create_pipeline(
            device,
            "immediate_color_pipeline",
            &color_bind_group_layout,
            &color_shader,
            ColorVertex2D::layout(),
            surface_format,
        );

        // --- Texture pipeline (position + texcoord + color) ---

        let texture_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("immediate_texture_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("texture_shader.wgsl").into()),
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("immediate_texture_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 4,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let texture_pipeline = Self::create_pipeline(
            device,
            "immediate_texture_pipeline",
            &texture_bind_group_layout,
            &texture_shader,
            TextureVertex2D::layout(),
            surface_format,
        );

        // --- Shared resources ---

        let uniform_buffer = Buffer::uniform(device, "immediate_uniforms", &[0u8; 16]);
        let color_vertex_buffer =
            Buffer::vertex_uninit(device, "immediate_color_vertices", 64 * 1024);
        let color_index_buffer =
            Buffer::index_uninit(device, "immediate_color_indices", 32 * 1024);
        let texture_vertex_buffer =
            Buffer::vertex_uninit(device, "immediate_texture_vertices", 64 * 1024);
        let texture_index_buffer =
            Buffer::index_uninit(device, "immediate_texture_indices", 32 * 1024);

        Self {
            color_pipeline,
            color_bind_group_layout,
            texture_pipeline,
            texture_bind_group_layout,
            uniform_buffer,
            color_vertex_buffer,
            color_index_buffer,
            texture_vertex_buffer,
            texture_index_buffer,
        }
    }

    fn create_pipeline(
        device: &wgpu::Device,
        label: &str,
        bind_group_layout: &wgpu::BindGroupLayout,
        shader: &wgpu::ShaderModule,
        vertex_layout: wgpu::VertexBufferLayout<'static>,
        surface_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &[bind_group_layout],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[vertex_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
    }

    pub fn begin<'a>(
        &'a mut self,
        device: &'a wgpu::Device,
        queue: &'a wgpu::Queue,
        target_view: &'a wgpu::TextureView,
        camera_uniforms: [f32; 4],
        clear_color: Option<wgpu::Color>,
    ) -> Frame<'a> {
        queue.write_buffer(
            &self.uniform_buffer.raw,
            0,
            bytemuck::cast_slice(&camera_uniforms),
        );

        Frame {
            renderer: self,
            device,
            queue,
            target_view,
            clear_color,
            batches: Vec::new(),
            current_color_vertices: Vec::new(),
            all_color_vertices: Vec::new(),
            current_color_indices: Vec::new(),
            all_color_indices: Vec::new(),
            current_texture_vertices: Vec::new(),
            all_texture_vertices: Vec::new(),
            current_texture_indices: Vec::new(),
            all_texture_indices: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Batch (internal)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum BatchKind {
    Color,
    Texture,
}

enum DrawCall {
    Draw {
        vertex_offset: u32,
        vertex_count: u32,
    },
    DrawIndexed {
        vertex_offset: i32,
        index_offset: u32,
        index_count: u32,
    },
}

struct Batch {
    kind: BatchKind,
    bind_group: wgpu::BindGroup,
    draw: DrawCall,
}

// ---------------------------------------------------------------------------
// Frame
// ---------------------------------------------------------------------------

pub struct Frame<'a> {
    renderer: &'a mut ImmediateRenderer,
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    target_view: &'a wgpu::TextureView,
    clear_color: Option<wgpu::Color>,
    batches: Vec<Batch>,
    current_color_vertices: Vec<ColorVertex2D>,
    all_color_vertices: Vec<ColorVertex2D>,
    current_color_indices: Vec<u16>,
    all_color_indices: Vec<u16>,
    current_texture_vertices: Vec<TextureVertex2D>,
    all_texture_vertices: Vec<TextureVertex2D>,
    current_texture_indices: Vec<u16>,
    all_texture_indices: Vec<u16>,
}

impl<'a> Frame<'a> {
    pub fn color_material(&mut self, color: Color, transform: Mat4) -> ColorMaterialGuard<'a, '_> {
        let material_buffer =
            Buffer::uniform(self.device, "immediate_material_color", bytemuck::bytes_of(&color));
        let modelview_buffer = Buffer::uniform(
            self.device,
            "immediate_modelview",
            bytemuck::bytes_of(&transform),
        );
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("immediate_color_bind_group"),
            layout: &self.renderer.color_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.renderer.uniform_buffer.raw.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: material_buffer.raw.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: modelview_buffer.raw.as_entire_binding(),
                },
            ],
        });

        ColorMaterialGuard {
            frame: self,
            bind_group: Some(bind_group),
        }
    }

    pub fn texture_material(
        &mut self,
        texture: &'a Texture,
        color: Color,
        transform: Mat4,
    ) -> TextureMaterialGuard<'a, '_> {
        let material_buffer =
            Buffer::uniform(self.device, "immediate_material_color", bytemuck::bytes_of(&color));
        let modelview_buffer = Buffer::uniform(
            self.device,
            "immediate_modelview",
            bytemuck::bytes_of(&transform),
        );
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("immediate_texture_bind_group"),
            layout: &self.renderer.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.renderer.uniform_buffer.raw.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: material_buffer.raw.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: modelview_buffer.raw.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&texture.sampler),
                },
            ],
        });

        TextureMaterialGuard {
            frame: self,
            bind_group: Some(bind_group),
        }
    }

    fn flush_color_batch(&mut self, bind_group: wgpu::BindGroup) {
        let has_indices = !self.current_color_indices.is_empty();
        let has_vertices = !self.current_color_vertices.is_empty();

        if !has_vertices {
            return;
        }

        let vertex_offset = self.all_color_vertices.len() as u32;
        self.all_color_vertices
            .append(&mut self.current_color_vertices);

        let draw = if has_indices {
            let index_offset = self.all_color_indices.len() as u32;
            let index_count = self.current_color_indices.len() as u32;
            self.all_color_indices
                .append(&mut self.current_color_indices);
            DrawCall::DrawIndexed {
                vertex_offset: vertex_offset as i32,
                index_offset,
                index_count,
            }
        } else {
            let vertex_count = self.all_color_vertices.len() as u32 - vertex_offset;
            DrawCall::Draw {
                vertex_offset,
                vertex_count,
            }
        };

        self.batches.push(Batch {
            kind: BatchKind::Color,
            bind_group,
            draw,
        });
    }

    fn flush_texture_batch(&mut self, bind_group: wgpu::BindGroup) {
        let has_indices = !self.current_texture_indices.is_empty();
        let has_vertices = !self.current_texture_vertices.is_empty();

        if !has_vertices {
            return;
        }

        let vertex_offset = self.all_texture_vertices.len() as u32;
        self.all_texture_vertices
            .append(&mut self.current_texture_vertices);

        let draw = if has_indices {
            let index_offset = self.all_texture_indices.len() as u32;
            let index_count = self.current_texture_indices.len() as u32;
            self.all_texture_indices
                .append(&mut self.current_texture_indices);
            DrawCall::DrawIndexed {
                vertex_offset: vertex_offset as i32,
                index_offset,
                index_count,
            }
        } else {
            let vertex_count = self.all_texture_vertices.len() as u32 - vertex_offset;
            DrawCall::Draw {
                vertex_offset,
                vertex_count,
            }
        };

        self.batches.push(Batch {
            kind: BatchKind::Texture,
            bind_group,
            draw,
        });
    }
}

impl Drop for Frame<'_> {
    fn drop(&mut self) {
        let has_color = !self.all_color_vertices.is_empty();
        let has_texture = !self.all_texture_vertices.is_empty();

        if !has_color && !has_texture {
            if let Some(clear_color) = self.clear_color {
                let mut encoder = self
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("immediate_encoder"),
                    });
                {
                    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("immediate_clear_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: self.target_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        ..Default::default()
                    });
                }
                self.queue.submit(std::iter::once(encoder.finish()));
            }
            return;
        }

        // Upload vertices and indices to GPU
        if has_color {
            self.renderer.color_vertex_buffer.write(
                self.device,
                self.queue,
                bytemuck::cast_slice(&self.all_color_vertices),
            );
            if !self.all_color_indices.is_empty() {
                self.renderer.color_index_buffer.write(
                    self.device,
                    self.queue,
                    bytemuck::cast_slice(&self.all_color_indices),
                );
            }
        }
        if has_texture {
            self.renderer.texture_vertex_buffer.write(
                self.device,
                self.queue,
                bytemuck::cast_slice(&self.all_texture_vertices),
            );
            if !self.all_texture_indices.is_empty() {
                self.renderer.texture_index_buffer.write(
                    self.device,
                    self.queue,
                    bytemuck::cast_slice(&self.all_texture_indices),
                );
            }
        }

        // Record render pass
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("immediate_encoder"),
            });

        {
            let load_op = match self.clear_color {
                Some(color) => wgpu::LoadOp::Clear(color),
                None => wgpu::LoadOp::Load,
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("immediate_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            let mut current_kind: Option<BatchKind> = None;

            for batch in &self.batches {
                if current_kind != Some(batch.kind) {
                    match batch.kind {
                        BatchKind::Color => {
                            pass.set_pipeline(&self.renderer.color_pipeline);
                            pass.set_vertex_buffer(
                                0,
                                self.renderer.color_vertex_buffer.raw.slice(..),
                            );
                        }
                        BatchKind::Texture => {
                            pass.set_pipeline(&self.renderer.texture_pipeline);
                            pass.set_vertex_buffer(
                                0,
                                self.renderer.texture_vertex_buffer.raw.slice(..),
                            );
                        }
                    }
                    current_kind = Some(batch.kind);
                }

                pass.set_bind_group(0, &batch.bind_group, &[]);

                match batch.draw {
                    DrawCall::Draw {
                        vertex_offset,
                        vertex_count,
                    } => {
                        pass.draw(vertex_offset..vertex_offset + vertex_count, 0..1);
                    }
                    DrawCall::DrawIndexed {
                        vertex_offset,
                        index_offset,
                        index_count,
                    } => {
                        let index_buffer = match batch.kind {
                            BatchKind::Color => &self.renderer.color_index_buffer,
                            BatchKind::Texture => &self.renderer.texture_index_buffer,
                        };
                        pass.set_index_buffer(
                            index_buffer.raw.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        pass.draw_indexed(
                            index_offset..index_offset + index_count,
                            vertex_offset,
                            0..1,
                        );
                    }
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }
}

// ---------------------------------------------------------------------------
// ColorMaterialGuard
// ---------------------------------------------------------------------------

pub struct ColorMaterialGuard<'a, 'f> {
    frame: &'f mut Frame<'a>,
    bind_group: Option<wgpu::BindGroup>,
}

#[allow(dead_code)]
impl ColorMaterialGuard<'_, '_> {
    pub fn triangle(&mut self, a: ColorVertex2D, b: ColorVertex2D, c: ColorVertex2D) {
        self.push(a);
        self.push(b);
        self.push(c);
    }

    pub fn quad(
        &mut self,
        a: ColorVertex2D,
        b: ColorVertex2D,
        c: ColorVertex2D,
        d: ColorVertex2D,
    ) {
        self.push(a);
        self.push(b);
        self.push(c);
        self.push(a);
        self.push(c);
        self.push(d);
    }

    pub fn triangle_list(&mut self, vertices: &[ColorVertex2D]) {
        assert!(
            vertices.len().is_multiple_of(3),
            "triangle_list requires a multiple of 3 vertices"
        );
        for v in vertices {
            self.push(*v);
        }
    }

    pub fn triangle_fan(&mut self, vertices: &[ColorVertex2D]) {
        if vertices.len() < 3 {
            return;
        }
        let center = vertices[0];
        for i in 1..vertices.len() - 1 {
            self.push(center);
            self.push(vertices[i]);
            self.push(vertices[i + 1]);
        }
    }

    pub fn indexed_triangle_list(&mut self, vertices: &[ColorVertex2D], indices: &[u16]) {
        let base = self.frame.current_color_vertices.len() as u16;
        self.frame.current_color_vertices.extend_from_slice(vertices);
        self.frame
            .current_color_indices
            .extend(indices.iter().map(|i| i + base));
    }

    fn push(&mut self, vertex: ColorVertex2D) {
        self.frame.current_color_vertices.push(vertex);
    }
}

impl Drop for ColorMaterialGuard<'_, '_> {
    fn drop(&mut self) {
        if let Some(bind_group) = self.bind_group.take() {
            self.frame.flush_color_batch(bind_group);
        }
    }
}

// ---------------------------------------------------------------------------
// TextureMaterialGuard
// ---------------------------------------------------------------------------

pub struct TextureMaterialGuard<'a, 'f> {
    frame: &'f mut Frame<'a>,
    bind_group: Option<wgpu::BindGroup>,
}

#[allow(dead_code)]
impl TextureMaterialGuard<'_, '_> {
    pub fn triangle(&mut self, a: TextureVertex2D, b: TextureVertex2D, c: TextureVertex2D) {
        self.push(a);
        self.push(b);
        self.push(c);
    }

    pub fn quad(
        &mut self,
        a: TextureVertex2D,
        b: TextureVertex2D,
        c: TextureVertex2D,
        d: TextureVertex2D,
    ) {
        self.push(a);
        self.push(b);
        self.push(c);
        self.push(a);
        self.push(c);
        self.push(d);
    }

    pub fn triangle_list(&mut self, vertices: &[TextureVertex2D]) {
        assert!(
            vertices.len().is_multiple_of(3),
            "triangle_list requires a multiple of 3 vertices"
        );
        for v in vertices {
            self.push(*v);
        }
    }

    pub fn triangle_fan(&mut self, vertices: &[TextureVertex2D]) {
        if vertices.len() < 3 {
            return;
        }
        let center = vertices[0];
        for i in 1..vertices.len() - 1 {
            self.push(center);
            self.push(vertices[i]);
            self.push(vertices[i + 1]);
        }
    }

    pub fn indexed_triangle_list(&mut self, vertices: &[TextureVertex2D], indices: &[u16]) {
        let base = self.frame.current_texture_vertices.len() as u16;
        self.frame
            .current_texture_vertices
            .extend_from_slice(vertices);
        self.frame
            .current_texture_indices
            .extend(indices.iter().map(|i| i + base));
    }

    fn push(&mut self, vertex: TextureVertex2D) {
        self.frame.current_texture_vertices.push(vertex);
    }
}

impl Drop for TextureMaterialGuard<'_, '_> {
    fn drop(&mut self) {
        if let Some(bind_group) = self.bind_group.take() {
            self.frame.flush_texture_batch(bind_group);
        }
    }
}

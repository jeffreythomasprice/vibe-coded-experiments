mod atlas;
mod batch;

use std::mem::size_of;

use bytemuck::{Pod, bytes_of, cast_slice};
use web_sys::HtmlCanvasElement;

use self::atlas::Atlas;
use self::batch::{Batch, GlyphInst, HexInst, MeshVertex, RectInst};
use crate::error::AppError;
use crate::hex::Point;
use crate::prim::IconPrim;
use crate::scene::{GridUniforms, HexInstance};
use crate::text::Fonts;
use crate::ui::UiFrame;

const MSAA_SAMPLES: u32 = 4;

pub struct Frame<'a> {
    pub grid: GridUniforms,
    pub hexes: &'a [HexInstance],
    pub world_icons: &'a [IconPrim<'a>],
    pub ui: &'a UiFrame<'a>,
    pub fonts: &'a Fonts,
    pub scale: f64,
}

struct GrowBuffer {
    buffer: wgpu::Buffer,
    capacity: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl GrowBuffer {
    fn new(device: &wgpu::Device, label: &'static str, usage: wgpu::BufferUsages) -> Self {
        let capacity = 4096;
        GrowBuffer {
            buffer: Self::create(device, label, usage, capacity),
            capacity,
            usage,
            label,
        }
    }

    fn create(
        device: &wgpu::Device,
        label: &'static str,
        usage: wgpu::BufferUsages,
        size: u64,
    ) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn upload<T: Pod>(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[T]) {
        let bytes: &[u8] = cast_slice(data);
        if bytes.is_empty() {
            return;
        }
        let needed = bytes.len() as u64;
        if needed > self.capacity {
            self.capacity = needed.next_power_of_two();
            self.buffer = Self::create(device, self.label, self.usage, self.capacity);
        }
        queue.write_buffer(&self.buffer, 0, bytes);
    }
}

pub struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    samples: u32,
    msaa_view: Option<wgpu::TextureView>,
    globals_buffer: wgpu::Buffer,
    grid_buffer: wgpu::Buffer,
    globals_group: wgpu::BindGroup,
    grid_group: wgpu::BindGroup,
    text_group: wgpu::BindGroup,
    atlas: Atlas,
    grid_pipeline: wgpu::RenderPipeline,
    hex_pipeline: wgpu::RenderPipeline,
    mesh_pipeline: wgpu::RenderPipeline,
    rect_pipeline: wgpu::RenderPipeline,
    text_pipeline: wgpu::RenderPipeline,
    hex_buffer: GrowBuffer,
    vertex_buffer: GrowBuffer,
    index_buffer: GrowBuffer,
    rect_buffer: GrowBuffer,
    glyph_buffer: GrowBuffer,
}

impl Gpu {
    pub async fn new(canvas: HtmlCanvasElement, width: u32, height: u32) -> Result<Self, AppError> {
        let instance = wgpu::util::new_instance_with_webgpu_detection(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        })
        .await;
        let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or(AppError::SurfaceConfig)?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .ok_or(AppError::SurfaceConfig)?,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let samples = if adapter
            .get_texture_format_features(format)
            .flags
            .sample_count_supported(MSAA_SAMPLES)
        {
            MSAA_SAMPLES
        } else {
            1
        };
        log::info!("gpu: {:?}, {samples}x msaa, {format:?}", adapter.get_info());

        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atlas layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let uniform_buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let globals_buffer = uniform_buffer("globals", size_of::<[f32; 4]>() as u64);
        let grid_buffer = uniform_buffer("grid", size_of::<[[f32; 4]; 4]>() as u64);
        let uniform_group = |label, buffer: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            })
        };
        let globals_group = uniform_group("globals group", &globals_buffer);
        let grid_group = uniform_group("grid group", &grid_buffer);

        let atlas = Atlas::new(&device);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let text_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas group"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(atlas.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let uniform_only = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("uniform pipeline layout"),
            bind_group_layouts: &[Some(&uniform_layout)],
            immediate_size: 0,
        });
        let with_atlas = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text pipeline layout"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let make = |label: &str,
                    source: &str,
                    layout: &wgpu::PipelineLayout,
                    buffers: &[Option<wgpu::VertexBufferLayout>]| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: samples,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview_mask: None,
                cache: None,
            })
        };

        let hex_attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];
        let mesh_attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
        let rect_attrs = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4
        ];
        let glyph_attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];

        let grid_pipeline = make(
            "grid",
            include_str!("shaders/grid.wgsl"),
            &uniform_only,
            &[],
        );
        let hex_pipeline = make(
            "hex",
            include_str!("shaders/hex.wgsl"),
            &uniform_only,
            &[instanced(size_of::<HexInst>(), &hex_attrs)],
        );
        let mesh_pipeline = make(
            "mesh",
            include_str!("shaders/mesh.wgsl"),
            &uniform_only,
            &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<MeshVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &mesh_attrs,
            })],
        );
        let rect_pipeline = make(
            "rect",
            include_str!("shaders/rect.wgsl"),
            &uniform_only,
            &[instanced(size_of::<RectInst>(), &rect_attrs)],
        );
        let text_pipeline = make(
            "text",
            include_str!("shaders/text.wgsl"),
            &with_atlas,
            &[instanced(size_of::<GlyphInst>(), &glyph_attrs)],
        );

        let vertex = wgpu::BufferUsages::VERTEX;
        let mut gpu = Gpu {
            surface,
            msaa_view: None,
            hex_buffer: GrowBuffer::new(&device, "hexes", vertex),
            vertex_buffer: GrowBuffer::new(&device, "mesh vertices", vertex),
            index_buffer: GrowBuffer::new(&device, "mesh indices", wgpu::BufferUsages::INDEX),
            rect_buffer: GrowBuffer::new(&device, "rects", vertex),
            glyph_buffer: GrowBuffer::new(&device, "glyphs", vertex),
            device,
            queue,
            config,
            samples,
            globals_buffer,
            grid_buffer,
            globals_group,
            grid_group,
            text_group,
            atlas,
            grid_pipeline,
            hex_pipeline,
            mesh_pipeline,
            rect_pipeline,
            text_pipeline,
        };
        gpu.rebuild_msaa();
        Ok(gpu)
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if (width, height) == self.size() {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.rebuild_msaa();
    }

    fn rebuild_msaa(&mut self) {
        self.msaa_view = (self.samples > 1).then(|| {
            self.device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("msaa"),
                    size: wgpu::Extent3d {
                        width: self.config.width,
                        height: self.config.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: self.samples,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.config.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        });
    }

    pub fn render(&mut self, frame: &Frame) {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => t,
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => {
                drop(t);
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            other => {
                log::debug!("skipping frame: {other:?}");
                return;
            }
        };

        let scale = frame.scale as f32;
        let viewport = frame.grid.viewport;
        self.queue.write_buffer(
            &self.globals_buffer,
            0,
            bytes_of(&[viewport.x as f32, viewport.y as f32, scale, 0.0]),
        );
        self.queue.write_buffer(
            &self.grid_buffer,
            0,
            bytes_of(&grid_data(&frame.grid, scale)),
        );

        let mut batch = Batch::default();
        for hex in frame.hexes {
            batch.push_hex(hex);
        }
        for icon in frame.world_icons {
            batch.push_icon(icon);
        }
        let world_indices = 0..batch.indices.len() as u32;
        let layers: Vec<_> = frame
            .ui
            .layers
            .iter()
            .map(|layer| batch.push_layer(layer, &mut self.atlas, &self.queue, frame.fonts, scale))
            .collect();

        self.hex_buffer
            .upload(&self.device, &self.queue, &batch.hexes);
        self.vertex_buffer
            .upload(&self.device, &self.queue, &batch.vertices);
        self.index_buffer
            .upload(&self.device, &self.queue, &batch.indices);
        self.rect_buffer
            .upload(&self.device, &self.queue, &batch.rects);
        self.glyph_buffer
            .upload(&self.device, &self.queue, &batch.glyphs);

        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (view, resolve_target) = match &self.msaa_view {
            Some(msaa) => (msaa, Some(&surface_view)),
            None => (&surface_view, None),
        };
        let bg = frame.grid.background;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(bg.r),
                            g: f64::from(bg.g),
                            b: f64::from(bg.b),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.grid_pipeline);
            pass.set_bind_group(0, &self.grid_group, &[]);
            pass.draw(0..3, 0..1);

            pass.set_bind_group(0, &self.globals_group, &[]);
            if !batch.hexes.is_empty() {
                pass.set_pipeline(&self.hex_pipeline);
                pass.set_vertex_buffer(0, self.hex_buffer.buffer.slice(..));
                pass.draw(0..6, 0..batch.hexes.len() as u32);
            }

            let draw_mesh = |pass: &mut wgpu::RenderPass, range: std::ops::Range<u32>| {
                if range.is_empty() {
                    return;
                }
                pass.set_pipeline(&self.mesh_pipeline);
                pass.set_vertex_buffer(0, self.vertex_buffer.buffer.slice(..));
                pass.set_index_buffer(
                    self.index_buffer.buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                pass.draw_indexed(range, 0, 0..1);
            };
            draw_mesh(&mut pass, world_indices);

            for ranges in layers {
                if !ranges.rects.is_empty() {
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_vertex_buffer(0, self.rect_buffer.buffer.slice(..));
                    pass.draw(0..6, ranges.rects);
                }
                draw_mesh(&mut pass, ranges.indices);
                if !ranges.glyphs.is_empty() {
                    pass.set_pipeline(&self.text_pipeline);
                    pass.set_bind_group(1, &self.text_group, &[]);
                    pass.set_vertex_buffer(0, self.glyph_buffer.buffer.slice(..));
                    pass.draw(0..6, ranges.glyphs);
                }
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(surface_texture);
    }
}

fn instanced(
    stride: usize,
    attributes: &[wgpu::VertexAttribute],
) -> Option<wgpu::VertexBufferLayout<'_>> {
    Some(wgpu::VertexBufferLayout {
        array_stride: stride as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes,
    })
}

fn grid_data(grid: &GridUniforms, scale: f32) -> [[f32; 4]; 4] {
    let Point { x: vx, y: vy } = grid.viewport;
    [
        [
            grid.center.x as f32,
            grid.center.y as f32,
            vx as f32,
            vy as f32,
        ],
        [
            grid.zoom as f32,
            grid.hex_size as f32,
            grid.border_width as f32,
            scale,
        ],
        grid.fill.to_array(),
        grid.border.to_array(),
    ]
}

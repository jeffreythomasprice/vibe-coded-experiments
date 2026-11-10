use std::borrow::Cow;

use wgpu::util::{BufferInitDescriptor, DeviceExt};

use crate::error::RenderError;
use crate::scene::Vertex;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    faces: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    lines_on_top: wgpu::RenderPipeline,
    id_faces: wgpu::RenderPipeline,
    adapter: String,
}

pub struct DrawLists<'a> {
    pub tris: &'a [Vertex],
    pub lines: &'a [Vertex],
    pub on_top: &'a [Vertex],
}

pub struct DrawParams {
    pub view_proj: [[f32; 4]; 4],
    pub width: u32,
    pub height: u32,
    pub clear: [f64; 3],
    pub clip_z: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    clip: [f32; 4],
}

impl Renderer {
    /// Opens a headless device. `WGPU_BACKEND`, `WGPU_ADAPTER_NAME` and `WGPU_POWER_PREF` pick the adapter.
    pub fn new() -> Result<Self, RenderError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let mut fallback = false;
        let adapter = loop {
            let options = wgpu::RequestAdapterOptions {
                force_fallback_adapter: fallback,
                ..Default::default()
            };
            match pollster::block_on(instance.request_adapter(&options)) {
                Ok(a) => break a,
                Err(_) if !fallback => fallback = true,
                Err(e) => return Err(RenderError::NoAdapter(e.to_string())),
            }
        };
        let info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("level-render"),
            ..Default::default()
        }))
        .map_err(|e| RenderError::Device(e.to_string()))?;

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("level-render shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("shader.wgsl"))),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let make = |topology, compare, write, bias: wgpu::DepthBiasState, samples: u32| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(write),
                    depth_compare: Some(compare),
                    stencil: Default::default(),
                    bias,
                }),
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(COLOR_FORMAT.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let push_back = wgpu::DepthBiasState {
            constant: 2,
            slope_scale: 1.5,
            clamp: 0.0,
        };
        let faces = make(
            wgpu::PrimitiveTopology::TriangleList,
            wgpu::CompareFunction::LessEqual,
            true,
            push_back,
            SAMPLES,
        );
        let lines = make(
            wgpu::PrimitiveTopology::LineList,
            wgpu::CompareFunction::LessEqual,
            true,
            Default::default(),
            SAMPLES,
        );
        let lines_on_top = make(
            wgpu::PrimitiveTopology::LineList,
            wgpu::CompareFunction::Always,
            false,
            Default::default(),
            SAMPLES,
        );
        let id_faces = make(
            wgpu::PrimitiveTopology::TriangleList,
            wgpu::CompareFunction::LessEqual,
            true,
            Default::default(),
            1,
        );

        Ok(Self {
            device,
            queue,
            bind_group_layout,
            faces,
            lines,
            lines_on_top,
            id_faces,
            adapter: format!("{} ({:?})", info.name, info.backend),
        })
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter
    }

    /// Renders and returns tightly packed RGBA8 rows.
    pub fn draw(&self, params: &DrawParams, lists: &DrawLists) -> Result<Vec<u8>, RenderError> {
        self.render(
            params,
            SAMPLES,
            &[
                (&self.faces, lists.tris),
                (&self.lines, lists.lines),
                (&self.lines_on_top, lists.on_top),
            ],
        )
    }

    /// Single-sample, unlit triangles whose vertex colours are read back exactly (object id buffer).
    pub fn draw_ids(&self, params: &DrawParams, tris: &[Vertex]) -> Result<Vec<u8>, RenderError> {
        self.render(params, 1, &[(&self.id_faces, tris)])
    }

    fn render(
        &self,
        params: &DrawParams,
        samples: u32,
        batches: &[(&wgpu::RenderPipeline, &[Vertex])],
    ) -> Result<Vec<u8>, RenderError> {
        let (w, h) = (params.width, params.height);
        let size = wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        };
        let texture = |label, samples, format, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let resolve_tex = texture(
            "resolve",
            1,
            COLOR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let resolve_view = resolve_tex.create_view(&Default::default());
        let msaa_view = (samples > 1).then(|| {
            texture(
                "msaa",
                samples,
                COLOR_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
            .create_view(&Default::default())
        });
        let depth_view = texture(
            "depth",
            samples,
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        )
        .create_view(&Default::default());

        let globals = Globals {
            view_proj: params.view_proj,
            clip: [params.clip_z, 0.0, 0.0, 0.0],
        };
        let globals = self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("globals"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &self.bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let buffers: Vec<_> = batches
            .iter()
            .map(|(pipeline, data)| {
                let buffer = (!data.is_empty()).then(|| {
                    self.device.create_buffer_init(&BufferInitDescriptor {
                        label: None,
                        contents: bytemuck::cast_slice(data),
                        usage: wgpu::BufferUsages::VERTEX,
                    })
                });
                (*pipeline, buffer, data.len())
            })
            .collect();

        let row_bytes = w * 4;
        let padded = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let [r, g, b] = params.clear;
            let (view, resolve_target) = match &msaa_view {
                Some(msaa) => (msaa, Some(&resolve_view)),
                None => (&resolve_view, None),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                        store: if samples > 1 {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &bind_group, &[]);
            for (pipeline, buffer, count) in &buffers {
                if let Some(buffer) = buffer {
                    pass.set_pipeline(pipeline);
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.draw(0..*count as u32, 0..1);
                }
            }
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &resolve_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            size,
        );
        self.queue.submit([encoder.finish()]);

        let (tx, rx) = std::sync::mpsc::channel();
        readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        rx.recv()
            .map_err(|e| RenderError::Readback(e.to_string()))?
            .map_err(|e| RenderError::Readback(e.to_string()))?;

        let mapped = readback
            .slice(..)
            .get_mapped_range()
            .map_err(|e| RenderError::Readback(e.to_string()))?;
        let mut rgba = Vec::with_capacity((row_bytes * h) as usize);
        for row in mapped.chunks(padded as usize) {
            rgba.extend_from_slice(&row[..row_bytes as usize]);
        }
        Ok(rgba)
    }
}

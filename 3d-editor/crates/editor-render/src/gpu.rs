use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use wgpu::util::{BufferInitDescriptor, DeviceExt};

use crate::error::RenderError;
use crate::scene::{FaceBatch, Vertex};
use crate::texture::{RenderMaterial, TextureCache, TextureData};

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    material_layout: wgpu::BindGroupLayout,
    faces: wgpu::RenderPipeline,
    faces_blend: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    lines_on_top: wgpu::RenderPipeline,
    id_faces: wgpu::RenderPipeline,
    adapter: String,
    textures: TextureCache,
    uploaded: Mutex<HashMap<u64, Arc<wgpu::TextureView>>>,
    white: Arc<wgpu::TextureView>,
    linear: wgpu::Sampler,
    nearest: wgpu::Sampler,
}

pub struct DrawLists<'a> {
    pub faces: &'a [FaceBatch],
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
struct MaterialUniform {
    tint: [f32; 4],
    emissive: [f32; 4],
}

/// One thing to draw, in pass order.
enum Item<'a> {
    Faces {
        material: Option<&'a RenderMaterial>,
        data: &'a [Vertex],
    },
    Plain {
        pipeline: &'a wgpu::RenderPipeline,
        data: &'a [Vertex],
    },
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
            label: Some("editor-render"),
            ..Default::default()
        }))
        .map_err(|e| RenderError::Device(e.to_string()))?;
        Ok(Self::from_device(
            device,
            queue,
            format!("{} ({:?})", info.name, info.backend),
        ))
    }

    /// Builds a renderer on a device the caller owns, so views can be drawn straight into its textures.
    pub fn from_device(device: wgpu::Device, queue: wgpu::Queue, adapter: String) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("editor-render shader"),
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
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
        let flat_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("flat layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let face_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("face layout"),
            bind_group_layouts: &[Some(&bind_group_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let make = |layout: &wgpu::PipelineLayout,
                    entry: &str,
                    blend: Option<wgpu::BlendState>,
                    topology,
                    compare,
                    write,
                    bias: wgpu::DepthBiasState,
                    samples: u32| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2],
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
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
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
        let over = wgpu::BlendState {
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
        };
        let faces = make(
            &face_layout,
            "fs_face",
            None,
            wgpu::PrimitiveTopology::TriangleList,
            wgpu::CompareFunction::LessEqual,
            true,
            push_back,
            SAMPLES,
        );
        let faces_blend = make(
            &face_layout,
            "fs_face_blend",
            Some(over),
            wgpu::PrimitiveTopology::TriangleList,
            wgpu::CompareFunction::LessEqual,
            false,
            push_back,
            SAMPLES,
        );
        let lines = make(
            &flat_layout,
            "fs_main",
            None,
            wgpu::PrimitiveTopology::LineList,
            wgpu::CompareFunction::LessEqual,
            true,
            Default::default(),
            SAMPLES,
        );
        let lines_on_top = make(
            &flat_layout,
            "fs_main",
            None,
            wgpu::PrimitiveTopology::LineList,
            wgpu::CompareFunction::Always,
            false,
            Default::default(),
            SAMPLES,
        );
        let id_faces = make(
            &flat_layout,
            "fs_main",
            None,
            wgpu::PrimitiveTopology::TriangleList,
            wgpu::CompareFunction::LessEqual,
            true,
            Default::default(),
            1,
        );

        let sampler = |filter, aniso| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("material sampler"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: filter,
                min_filter: filter,
                mipmap_filter: match filter {
                    wgpu::FilterMode::Nearest => wgpu::MipmapFilterMode::Nearest,
                    wgpu::FilterMode::Linear => wgpu::MipmapFilterMode::Linear,
                },
                anisotropy_clamp: aniso,
                ..Default::default()
            })
        };
        let linear = sampler(wgpu::FilterMode::Linear, 8);
        let nearest = sampler(wgpu::FilterMode::Nearest, 1);
        let white = Arc::new(upload_texture(&device, &queue, 1, 1, &[vec![255; 4]]));

        Self {
            device,
            queue,
            bind_group_layout,
            material_layout,
            faces,
            faces_blend,
            lines,
            lines_on_top,
            id_faces,
            adapter,
            textures: TextureCache::default(),
            uploaded: Mutex::new(HashMap::new()),
            white,
            linear,
            nearest,
        }
    }

    /// Decoded material images, shared by every scene built for this renderer.
    pub fn textures(&self) -> &TextureCache {
        &self.textures
    }

    fn gpu_texture(&self, data: &TextureData) -> Arc<wgpu::TextureView> {
        let mut map = self.uploaded.lock().expect("texture uploads");
        map.entry(data.key)
            .or_insert_with(|| {
                Arc::new(upload_texture(
                    &self.device,
                    &self.queue,
                    data.width,
                    data.height,
                    &mip_chain(data),
                ))
            })
            .clone()
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter
    }

    /// Renders and returns tightly packed RGBA8 rows.
    pub fn draw(&self, params: &DrawParams, lists: &DrawLists) -> Result<Vec<u8>, RenderError> {
        self.render(params, SAMPLES, &self.items(lists))
    }

    /// Single-sample, unlit triangles whose vertex colours are read back exactly (object id buffer).
    pub fn draw_ids(&self, params: &DrawParams, tris: &[Vertex]) -> Result<Vec<u8>, RenderError> {
        self.render(
            params,
            1,
            &[Item::Plain {
                pipeline: &self.id_faces,
                data: tris,
            }],
        )
    }

    /// Renders into `target`, an `Rgba8Unorm` texture view with `RENDER_ATTACHMENT` usage of size `params.width` x `params.height`.
    pub fn draw_to_view(&self, params: &DrawParams, lists: &DrawLists, target: &wgpu::TextureView) {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.record(&mut encoder, params, SAMPLES, &self.items(lists), target);
        self.queue.submit([encoder.finish()]);
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub const COLOR_FORMAT: wgpu::TextureFormat = COLOR_FORMAT;

    fn items<'a>(&'a self, lists: &DrawLists<'a>) -> Vec<Item<'a>> {
        let faces = |blended: bool| {
            lists
                .faces
                .iter()
                .filter(move |b| b.material.as_ref().is_some_and(|m| m.blended) == blended)
                .map(|b| Item::Faces {
                    material: b.material.as_deref(),
                    data: &b.verts,
                })
        };
        let mut items: Vec<Item> = faces(false).collect();
        items.push(Item::Plain {
            pipeline: &self.lines,
            data: lists.lines,
        });
        items.extend(faces(true));
        items.push(Item::Plain {
            pipeline: &self.lines_on_top,
            data: lists.on_top,
        });
        items
    }

    fn texture(
        &self,
        label: &str,
        (width, height): (u32, u32),
        samples: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }

    fn record(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        params: &DrawParams,
        samples: u32,
        items: &[Item],
        target: &wgpu::TextureView,
    ) {
        let dims = (params.width, params.height);
        let msaa_view = (samples > 1).then(|| {
            self.texture(
                "msaa",
                dims,
                samples,
                COLOR_FORMAT,
                wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
            .create_view(&Default::default())
        });
        let depth_view = self
            .texture(
                "depth",
                dims,
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
        struct Prepared<'a> {
            pipeline: &'a wgpu::RenderPipeline,
            buffer: wgpu::Buffer,
            count: u32,
            material: Option<wgpu::BindGroup>,
        }
        let prepared: Vec<Prepared> = items
            .iter()
            .filter_map(|item| {
                let (pipeline, data, material) = match item {
                    Item::Plain { pipeline, data } => (*pipeline, *data, None),
                    Item::Faces { material, data } => {
                        let pipeline = if material.is_some_and(|m| m.blended) {
                            &self.faces_blend
                        } else {
                            &self.faces
                        };
                        (pipeline, *data, Some(self.material_group(*material)))
                    }
                };
                if data.is_empty() {
                    return None;
                }
                let buffer = self.device.create_buffer_init(&BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(data),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                Some(Prepared {
                    pipeline,
                    buffer,
                    count: data.len() as u32,
                    material,
                })
            })
            .collect();

        let [r, g, b] = params.clear;
        let (view, resolve_target) = match &msaa_view {
            Some(msaa) => (msaa, Some(target)),
            None => (target, None),
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
        for p in &prepared {
            pass.set_pipeline(p.pipeline);
            if let Some(group) = &p.material {
                pass.set_bind_group(1, group, &[]);
            }
            pass.set_vertex_buffer(0, p.buffer.slice(..));
            pass.draw(0..p.count, 0..1);
        }
    }

    fn material_group(&self, material: Option<&RenderMaterial>) -> wgpu::BindGroup {
        let uniform = match material {
            Some(m) => MaterialUniform {
                tint: m.tint,
                emissive: [m.emissive[0], m.emissive[1], m.emissive[2], 0.0],
            },
            None => MaterialUniform {
                tint: [1.0; 4],
                emissive: [0.0; 4],
            },
        };
        let buffer = self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("material"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let view = match material.and_then(|m| m.texture.as_ref()) {
            Some(data) => self.gpu_texture(data),
            None => self.white.clone(),
        };
        let sampler = if material.is_some_and(|m| m.nearest) {
            &self.nearest
        } else {
            &self.linear
        };
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material"),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    fn render(
        &self,
        params: &DrawParams,
        samples: u32,
        items: &[Item],
    ) -> Result<Vec<u8>, RenderError> {
        let (w, h) = (params.width, params.height);
        let resolve_tex = self.texture(
            "resolve",
            (w, h),
            1,
            COLOR_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let resolve_view = resolve_tex.create_view(&Default::default());

        let row_bytes = w * 4;
        let padded = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.record(&mut encoder, params, samples, items, &resolve_view);
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
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
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

fn mip_chain(data: &TextureData) -> Vec<Vec<u8>> {
    let mut levels = vec![data.rgba.clone()];
    let (mut w, mut h) = (data.width, data.height);
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let prev = image::RgbaImage::from_raw(w, h, levels.last().expect("one level").clone())
            .expect("level matches its size");
        levels.push(
            image::imageops::resize(&prev, nw, nh, image::imageops::FilterType::Triangle)
                .into_raw(),
        );
        (w, h) = (nw, nh);
    }
    levels
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    levels: &[Vec<u8>],
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let (mut w, mut h) = (width, height);
    for (level, bytes) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        (w, h) = ((w / 2).max(1), (h / 2).max(1));
    }
    texture.create_view(&Default::default())
}

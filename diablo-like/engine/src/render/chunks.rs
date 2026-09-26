use std::collections::{HashMap, VecDeque};

use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use wgpu::util::DeviceExt;

use super::camera_ubo::CameraUbo;
use crate::geom::field::{self, BakeJob, ChunkCoord};
use crate::geom::parts::Level;

/// Half the guaranteed WebGL2 floor of 256 array layers — see the design
/// plan's Chunked field section.
const MAX_LAYERS: u32 = 128;
const VOID_LAYER: u32 = 0;
/// Chunks load within this many chunks of the camera...
const LOAD_RADIUS: i32 = 3;
/// ...and are only evicted past this much larger radius. The band between
/// the two is never touched by either direction, so a camera oscillating
/// across a boundary can't thrash chunks in and out.
const EVICT_RADIUS: i32 = 5;
const LRU_CAPACITY: usize = 64;

struct ResidentChunk {
    layer: u32,
    /// Kept around for real (non-void) chunks so eviction can hand the
    /// buffer to the LRU cache instead of discarding it — re-baking is the
    /// expensive part, re-uploading a kept buffer is nearly free.
    data: Option<Vec<u16>>,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct ChunkStats {
    pub resident: usize,
    pub queued: usize,
    pub evicted_total: u64,
    pub last_bake_ms: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct ChunkInstance {
    origin: [f32; 2],
    layer: f32,
}

/// Owns the GPU-resident chunk field atlas (a `Texture2DArray`, one layer
/// per resident chunk) and the streaming state that decides which chunks
/// are loaded, queued, or evicted. See the design plan's Chunked field
/// section — this is the M6 deliverable.
pub struct ChunkAtlas {
    texture: wgpu::Texture,
    pub bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    free_layers: Vec<u32>,
    resident: HashMap<ChunkCoord, ResidentChunk>,
    pending: Vec<ChunkCoord>,
    current_job: Option<BakeJob>,
    lru: VecDeque<(ChunkCoord, Vec<u16>)>,
    evicted_total: u64,
    last_bake_ms: f32,
}

impl ChunkAtlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("chunk field atlas"),
            size: wgpu::Extent3d {
                width: field::STORED_RES,
                height: field::STORED_RES,
                depth_or_array_layers: MAX_LAYERS,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let array_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("chunk field atlas view"),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("chunk field sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chunk atlas bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chunk atlas bind group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let atlas = Self {
            texture,
            bind_group_layout,
            bind_group,
            free_layers: (1..MAX_LAYERS).rev().collect(),
            resident: HashMap::new(),
            pending: Vec::new(),
            current_job: None,
            lru: VecDeque::new(),
            evicted_total: 0,
            last_bake_ms: 0.0,
        };
        atlas.upload(queue, VOID_LAYER, &field::void_tile());
        atlas
    }

    /// Discovers newly-in-range chunks, evicts far ones, and spends up to
    /// `budget` on baking — all driven from `focus`, which should be the
    /// player's *predicted* position (current position extrapolated a
    /// short lookahead by velocity) so chunks are ready before they're
    /// visible rather than after.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        level: &Level,
        focus: Vec2,
        budget: std::time::Duration,
    ) -> ChunkStats {
        let _ = device;
        let focus_chunk = field::world_to_chunk(focus);

        for dy in -LOAD_RADIUS..=LOAD_RADIUS {
            for dx in -LOAD_RADIUS..=LOAD_RADIUS {
                let coord = (focus_chunk.0 + dx, focus_chunk.1 + dy);
                self.request(queue, level, coord);
            }
        }

        let mut to_evict = Vec::new();
        for &coord in self.resident.keys() {
            if field::chebyshev_distance(coord, focus_chunk) > EVICT_RADIUS {
                to_evict.push(coord);
            }
        }
        for coord in to_evict {
            self.evict(coord);
        }
        self.pending
            .retain(|&coord| field::chebyshev_distance(coord, focus_chunk) <= EVICT_RADIUS);
        if let Some(job) = &self.current_job
            && field::chebyshev_distance(job.coord, focus_chunk) > EVICT_RADIUS
        {
            self.current_job = None;
        }

        let start = web_time::Instant::now();
        loop {
            if self.current_job.is_none() {
                let Some(coord) = self.next_pending(focus_chunk) else {
                    break;
                };
                self.current_job = Some(BakeJob::new(coord));
            }
            let job = self.current_job.as_mut().expect("just ensured Some");
            job.step(level);
            if job.is_done() {
                let job = self.current_job.take().expect("checked Some above");
                let coord = job.coord;
                self.pending.retain(|&c| c != coord);
                self.finish_bake(queue, coord, job.into_data());
            }
            if start.elapsed() >= budget {
                break;
            }
        }
        self.last_bake_ms = start.elapsed().as_secs_f32() * 1000.0;

        ChunkStats {
            resident: self.resident.len(),
            queued: self.pending.len() + usize::from(self.current_job.is_some()),
            evicted_total: self.evicted_total,
            last_bake_ms: self.last_bake_ms,
        }
    }

    fn request(&mut self, queue: &wgpu::Queue, level: &Level, coord: ChunkCoord) {
        if self.resident.contains_key(&coord) || self.pending.contains(&coord) {
            return;
        }
        if self.current_job.as_ref().is_some_and(|j| j.coord == coord) {
            return;
        }

        if let Some(data) = self.take_lru(coord) {
            self.finish_bake(queue, coord, data);
            return;
        }

        let rect = field::chunk_world_rect(coord).expand(field::CLAMP_RANGE);
        if !level.any_part_overlaps(rect) {
            self.resident.insert(
                coord,
                ResidentChunk {
                    layer: VOID_LAYER,
                    data: None,
                },
            );
            return;
        }

        self.pending.push(coord);
    }

    fn finish_bake(&mut self, queue: &wgpu::Queue, coord: ChunkCoord, data: Vec<u16>) {
        let Some(layer) = self.free_layers.pop() else {
            // No capacity right now; drop it and let it be requested again
            // once something else frees a layer. Not expected to trigger in
            // this milestone's fixture, but must not panic if it does.
            return;
        };
        self.upload(queue, layer, &data);
        self.resident.insert(
            coord,
            ResidentChunk {
                layer,
                data: Some(data),
            },
        );
    }

    fn evict(&mut self, coord: ChunkCoord) {
        let Some(chunk) = self.resident.remove(&coord) else {
            return;
        };
        if chunk.layer != VOID_LAYER {
            self.free_layers.push(chunk.layer);
            if let Some(data) = chunk.data {
                self.lru.push_back((coord, data));
                if self.lru.len() > LRU_CAPACITY {
                    self.lru.pop_front();
                }
            }
        }
        self.evicted_total += 1;
    }

    fn take_lru(&mut self, coord: ChunkCoord) -> Option<Vec<u16>> {
        let idx = self.lru.iter().position(|(c, _)| *c == coord)?;
        self.lru.remove(idx).map(|(_, data)| data)
    }

    fn next_pending(&mut self, focus_chunk: ChunkCoord) -> Option<ChunkCoord> {
        let (idx, _) = self
            .pending
            .iter()
            .enumerate()
            .min_by_key(|&(_, &c)| field::chebyshev_distance(c, focus_chunk))?;
        Some(self.pending.remove(idx))
    }

    fn upload(&self, queue: &wgpu::Queue, layer: u32, data: &[u16]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(field::STORED_RES * 2),
                rows_per_image: Some(field::STORED_RES),
            },
            wgpu::Extent3d {
                width: field::STORED_RES,
                height: field::STORED_RES,
                depth_or_array_layers: 1,
            },
        );
    }

    fn instances(&self) -> Vec<ChunkInstance> {
        self.resident
            .iter()
            .map(|(&coord, chunk)| ChunkInstance {
                origin: field::chunk_origin(coord).into(),
                layer: chunk.layer as f32,
            })
            .collect()
    }
}

pub struct ChunkPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl ChunkPipeline {
    pub fn new(
        device: &wgpu::Device,
        target_format: wgpu::TextureFormat,
        camera: &CameraUbo,
        atlas_bind_group_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("chunk glow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/chunk_glow.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("chunk pipeline layout"),
            bind_group_layouts: &[Some(&camera.bind_group_layout), Some(atlas_bind_group_layout)],
            immediate_size: 0,
        });

        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ChunkInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("chunk pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[instance_layout],
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
                    blend: Some(wgpu::BlendState::REPLACE),
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
        camera: &'pass CameraUbo,
        atlas: &'pass ChunkAtlas,
    ) {
        let instances = atlas.instances();
        if instances.is_empty() {
            return;
        }

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("chunk instances"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &camera.bind_group, &[]);
        pass.set_bind_group(1, &atlas.bind_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..6, 0..instances.len() as u32);
    }
}

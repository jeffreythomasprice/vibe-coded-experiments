use std::sync::Arc;

use anyhow::{Context, Result};

use crate::bvh::GpuBvhNode;
use crate::camera::CameraUniforms;
use crate::overlay::Texture;
use crate::overlay_renderer::OverlayRenderer;
use crate::world::GpuChunkInfo;
use winit::window::Window;

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    voxel_mega_buffer: wgpu::Buffer,
    chunk_info_buffer: wgpu::Buffer,
    chunk_count_buffer: wgpu::Buffer,
    bvh_node_buffer: wgpu::Buffer,
    bvh_count_buffer: wgpu::Buffer,
    chunk_bind_group: wgpu::BindGroup,
    chunk_bgl: wgpu::BindGroupLayout,
    overlay_renderer: OverlayRenderer,
    atlas_bind_group_layout: wgpu::BindGroupLayout,
    atlas_bind_group: Option<wgpu::BindGroup>,
    voxel_atlas_bytes: u64,
}

pub struct GpuMemoryStats {
    pub voxel_buffer_bytes: u64,
    pub chunk_info_bytes: u64,
    pub chunk_count_bytes: u64,
    pub bvh_node_bytes: u64,
    pub bvh_count_bytes: u64,
    pub voxel_atlas_bytes: u64,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window)
            .context("failed to create surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .context("failed to find adapter")?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor::default(),
        ))
        .context("failed to create device")?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_uniform"),
            size: std::mem::size_of::<CameraUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let voxel_mega_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("voxel_mega_buffer"),
            size: 4096,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let chunk_info_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk_info_buffer"),
            size: std::mem::size_of::<GpuChunkInfo>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let chunk_count_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk_count_buffer"),
            size: 16, // u32 padded to 16 bytes for uniform alignment
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bvh_node_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bvh_node_buffer"),
            size: 32, // minimum size: one GpuBvhNode (32 bytes)
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bvh_count_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bvh_count_buffer"),
            size: 16, // u32 padded to 16 bytes for uniform alignment
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let chunk_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chunk_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let chunk_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chunk_bg"),
            layout: &chunk_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: voxel_mega_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: chunk_info_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: chunk_count_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: bvh_node_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: bvh_count_buffer.as_entire_binding(),
                },
            ],
        });

        let atlas_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atlas_bgl"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout, &chunk_bgl, &atlas_bgl],
            immediate_size: 0,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("voxels.wgsl").into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let mut overlay_renderer = OverlayRenderer::new(&device, &queue, format)?;
        overlay_renderer.resize(&queue, config.width, config.height);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            camera_buffer,
            camera_bind_group,
            voxel_mega_buffer,
            chunk_info_buffer,
            chunk_count_buffer,
            bvh_node_buffer,
            bvh_count_buffer,
            chunk_bind_group,
            chunk_bgl,
            overlay_renderer,
            atlas_bind_group_layout: atlas_bgl,
            atlas_bind_group: None,
            voxel_atlas_bytes: 0,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.overlay_renderer.resize(&self.queue, width, height);
    }

    pub fn upload_world(&mut self, voxel_data: &[u8], chunk_infos: &[GpuChunkInfo], chunk_count: u32, bvh_nodes: &[GpuBvhNode]) {
        let voxel_size = (voxel_data.len() as u64).max(4);
        if self.voxel_mega_buffer.size() < voxel_size {
            self.voxel_mega_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("voxel_mega_buffer"),
                size: voxel_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let info_size = (std::mem::size_of_val(chunk_infos) as u64).max(16);
        if self.chunk_info_buffer.size() < info_size {
            self.chunk_info_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("chunk_info_buffer"),
                size: info_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let bvh_size = (std::mem::size_of_val(bvh_nodes) as u64).max(32);
        if self.bvh_node_buffer.size() < bvh_size {
            self.bvh_node_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bvh_node_buffer"),
                size: bvh_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        self.chunk_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chunk_bg"),
            layout: &self.chunk_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.voxel_mega_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.chunk_info_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.chunk_count_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.bvh_node_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.bvh_count_buffer.as_entire_binding(),
                },
            ],
        });

        self.queue.write_buffer(&self.voxel_mega_buffer, 0, voxel_data);
        self.queue.write_buffer(&self.chunk_info_buffer, 0, bytemuck::cast_slice(chunk_infos));
        let count_padded: [u32; 4] = [chunk_count, 0, 0, 0];
        self.queue.write_buffer(&self.chunk_count_buffer, 0, bytemuck::cast_slice(&count_padded));

        if !bvh_nodes.is_empty() {
            self.queue.write_buffer(&self.bvh_node_buffer, 0, bytemuck::cast_slice(bvh_nodes));
        }
        let bvh_count_padded: [u32; 4] = [bvh_nodes.len() as u32, 0, 0, 0];
        self.queue.write_buffer(&self.bvh_count_buffer, 0, bytemuck::cast_slice(&bvh_count_padded));
    }

    pub fn upload_voxel_atlas(&mut self, atlas_texture: &Texture, uv_map: &[[f32; 4]; 256]) {
        let tex_size = wgpu::Extent3d {
            width: atlas_texture.width(),
            height: atlas_texture.height(),
            depth_or_array_layers: 1,
        };

        let gpu_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("voxel_atlas_texture"),
            size: tex_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(atlas_texture.data()),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * atlas_texture.width()),
                rows_per_image: Some(atlas_texture.height()),
            },
            tex_size,
        );

        let texture_view = gpu_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("voxel_atlas_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let uv_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("voxel_atlas_uv_map"),
            size: (256 * 4 * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        self.queue
            .write_buffer(&uv_buffer, 0, bytemuck::cast_slice(uv_map));

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas_bg"),
            layout: &self.atlas_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uv_buffer.as_entire_binding(),
                },
            ],
        });

        self.atlas_bind_group = Some(bind_group);
        self.voxel_atlas_bytes = (atlas_texture.width() as u64) * (atlas_texture.height() as u64) * 4;
    }

    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height as f32
    }

    pub fn screen_width(&self) -> f32 {
        self.config.width as f32
    }

    pub fn screen_height(&self) -> f32 {
        self.config.height as f32
    }

    pub fn begin_frame(&mut self) -> Option<(wgpu::SurfaceTexture, wgpu::TextureView)> {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                return None;
            }
            Err(_) => return None,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        Some((frame, view))
    }

    pub fn render_voxels(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        camera: &CameraUniforms,
    ) {
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(camera));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("voxel_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_bind_group(1, &self.chunk_bind_group, &[]);
        if let Some(ref atlas_bg) = self.atlas_bind_group {
            pass.set_bind_group(2, atlas_bg, &[]);
        }
        pass.draw(0..3, 0..1);
    }

    pub fn create_encoder(&self) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default())
    }

    pub fn submit(&self, encoder: wgpu::CommandEncoder) {
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn overlay(&self) -> &OverlayRenderer {
        &self.overlay_renderer
    }

    pub fn gpu_memory_stats(&self) -> GpuMemoryStats {
        GpuMemoryStats {
            voxel_buffer_bytes: self.voxel_mega_buffer.size(),
            chunk_info_bytes: self.chunk_info_buffer.size(),
            chunk_count_bytes: self.chunk_count_buffer.size(),
            bvh_node_bytes: self.bvh_node_buffer.size(),
            bvh_count_bytes: self.bvh_count_buffer.size(),
            voxel_atlas_bytes: self.voxel_atlas_bytes,
        }
    }

    pub fn render_overlay(
        &mut self,
        view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        draw_list: &crate::overlay::DrawList,
        texture_bind_group: &wgpu::BindGroup,
    ) {
        self.overlay_renderer
            .render(&self.device, &self.queue, view, encoder, draw_list, texture_bind_group);
    }
}

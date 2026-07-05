//! [`PixelBufferRenderer`]: the generic adapter for systems that own their video
//! memory as a grid of pixels. It converts a [`PixelBuffer`] to RGBA8, uploads it to
//! a texture, and blits a nearest-sampled, aspect-correct, letterboxed quad.

use common::{ScaleMode, Transform};

use crate::format::{to_rgba8, PixelBuffer};
use crate::scale::Background;
use crate::{RenderTarget, VideoError, VideoOutput};

/// The uploaded source frame and its per-size GPU resources. Rebuilt when the
/// source dimensions change.
struct FrameTexture {
    width: u32,
    height: u32,
    bind_group: wgpu::BindGroup,
}

/// Blits a CPU pixel array to the surface as a scaled, centered, letterboxed image.
///
/// Build once with [`PixelBufferRenderer::new`], feed frames with
/// [`PixelBufferRenderer::update`], and draw via the [`VideoOutput`] impl.
pub struct PixelBufferRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    texture: Option<wgpu::Texture>,
    frame: Option<FrameTexture>,
    scratch: Vec<u8>,
    scale_mode: ScaleMode,
    background: Background,
    internal_format: wgpu::TextureFormat,
}

impl PixelBufferRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        // Match the internal texture's sRGB-ness to the surface so 8-bit palette
        // colors round-trip exactly: the sampler's sRGB->linear decode cancels the
        // surface's linear->sRGB encode. On a linear surface, keep the texture linear.
        let internal_format = if target_format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("pixelbuf-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pixelbuf-bgl"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pixelbuf-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("pixelbuf-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pixelbuf-nearest"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pixelbuf-transform"),
            size: std::mem::size_of::<Transform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bind_group_layout,
            sampler,
            uniform,
            texture: None,
            frame: None,
            scratch: Vec::new(),
            scale_mode: ScaleMode::Fit,
            background: Background::default(),
            internal_format,
        }
    }

    pub fn set_scale_mode(&mut self, mode: ScaleMode) {
        self.scale_mode = mode;
    }

    pub fn set_background(&mut self, background: Background) {
        self.background = background;
    }

    pub fn scale_mode(&self) -> ScaleMode {
        self.scale_mode
    }

    /// Convert `buf` to RGBA8 and upload it. Recreates the GPU texture (and its bind
    /// group) when the source dimensions change; otherwise reuses them.
    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        buf: &PixelBuffer,
    ) -> Result<(), VideoError> {
        to_rgba8(buf, &mut self.scratch)?;

        let needs_new = self
            .frame
            .as_ref()
            .map(|f| f.width != buf.width || f.height != buf.height)
            .unwrap_or(true);

        if needs_new {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("pixelbuf-source"),
                size: wgpu::Extent3d {
                    width: buf.width,
                    height: buf.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.internal_format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pixelbuf-bind-group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.uniform.as_entire_binding(),
                    },
                ],
            });
            self.texture = Some(texture);
            self.frame = Some(FrameTexture {
                width: buf.width,
                height: buf.height,
                bind_group,
            });
            tracing::debug!(width = buf.width, height = buf.height, "created source texture");
        }

        // `write_texture` accepts arbitrary `bytes_per_row` (no 256-byte alignment,
        // which only applies to buffer<->texture copies), so upload tightly packed.
        let texture = self.texture.as_ref().expect("texture set above");
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &self.scratch,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(buf.width * 4),
                rows_per_image: Some(buf.height),
            },
            wgpu::Extent3d {
                width: buf.width,
                height: buf.height,
                depth_or_array_layers: 1,
            },
        );
        tracing::trace!(bytes = self.scratch.len(), "uploaded frame");

        Ok(())
    }
}

impl VideoOutput for PixelBufferRenderer {
    fn render(&mut self, target: RenderTarget<'_>) -> Result<(), VideoError> {
        let mut encoder =
            target
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("pixelbuf-encoder"),
                });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pixelbuf-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.background.0),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // With no frame uploaded yet, the clear alone paints the background.
            if let Some(frame) = self.frame.as_ref() {
                let transform = self.scale_mode.transform(
                    frame.width,
                    frame.height,
                    target.surface.width,
                    target.surface.height,
                );
                target
                    .queue
                    .write_buffer(&self.uniform, 0, bytemuck::bytes_of(&transform));

                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &frame.bind_group, &[]);
                pass.draw(0..4, 0..1);
            }
        }

        target.queue.submit(std::iter::once(encoder.finish()));
        Ok(())
    }
}

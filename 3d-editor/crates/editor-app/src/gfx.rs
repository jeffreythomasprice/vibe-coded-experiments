use std::sync::Arc;

use egui_wgpu::ScreenDescriptor;
use winit::window::Window;

use crate::error::AppError;

pub struct Gfx {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub egui: egui_wgpu::Renderer,
    pub level: level_render::Renderer,
    pub adapter: String,
}

pub struct Target {
    pub view: wgpu::TextureView,
    pub id: egui::TextureId,
    pub size: (u32, u32),
    _texture: wgpu::Texture,
}

impl Gfx {
    pub fn new(window: Arc<Window>) -> Result<Self, AppError> {
        let instance = wgpu::Instance::new(
            wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(window.clone())),
        );
        let surface = instance.create_surface(window.clone())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let info = adapter.get_info();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("editor-app"),
                ..Default::default()
            }))?;

        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or(AppError::SurfaceConfig)?;
        let caps = surface.get_capabilities(&adapter);
        if let Some(format) = caps.formats.iter().find(|f| !f.is_srgb()) {
            config.format = *format;
        }
        surface.configure(&device, &config);

        let egui = egui_wgpu::Renderer::new(
            &device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );
        let adapter_name = format!("{} ({:?})", info.name, info.backend);
        let level = level_render::Renderer::from_device(
            device.clone(),
            queue.clone(),
            adapter_name.clone(),
        );
        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            egui,
            level,
            adapter: adapter_name,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn create_target(&mut self, size: (u32, u32)) -> Target {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: level_render::Renderer::COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let id = self
            .egui
            .register_native_texture(&self.device, &view, wgpu::FilterMode::Linear);
        Target {
            view,
            id,
            size,
            _texture: texture,
        }
    }

    pub fn free_target(&mut self, target: Target) {
        self.egui.free_texture(&target.id);
    }

    pub fn present(
        &mut self,
        paint_jobs: &[egui::ClippedPrimitive],
        mut textures: egui::TexturesDelta,
        pixels_per_point: f32,
        clear: egui::Color32,
    ) {
        for (id, deltas) in std::mem::take(&mut textures.set) {
            for delta in &deltas {
                self.egui
                    .update_texture(&self.device, &self.queue, id, delta);
            }
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let screen = ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point,
        };
        let extra =
            self.egui
                .update_buffers(&self.device, &self.queue, &mut encoder, paint_jobs, &screen);
        {
            let [r, g, b, _] = clear.to_normalized_gamma_f32();
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: f64::from(r),
                                g: f64::from(g),
                                b: f64::from(b),
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.egui.render(&mut pass, paint_jobs, &screen);
        }
        self.queue
            .submit(extra.into_iter().chain([encoder.finish()]));
        self.queue.present(frame);
        for id in &textures.free {
            self.egui.free_texture(id);
        }
    }
}

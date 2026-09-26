use std::sync::Arc;

use thiserror::Error;
use winit::window::Window;

#[derive(Debug, Error)]
pub enum GpuError {
    #[error("failed to create surface: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    #[error("failed to find a graphics adapter: {0}")]
    Adapter(#[from] wgpu::RequestAdapterError),
    #[error("failed to request a device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
    #[error("surface reports no usable texture format")]
    NoSurfaceFormat,
}

/// Capabilities that are decided at startup by probing the adapter, since they
/// differ between the Vulkan and WebGL2 backends and must not be assumed at
/// compile time. See the "Frame graph" section of the design plan.
#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    /// `Some(Rgba16Float)` if usable as a render attachment (gated by
    /// `EXT_color_buffer_half_float`/`_float` on WebGL2); `None` means the
    /// offscreen HDR target must fall back to `Rgba8Unorm`.
    pub hdr_format: Option<wgpu::TextureFormat>,
    /// The offscreen render-scale target must be clamped to this — on
    /// WebGL2 it can be far smaller than a native adapter's, and a render
    /// scale above 1x can otherwise exceed it before the swapchain itself
    /// would.
    pub max_texture_dimension_2d: u32,
}

pub struct Gpu {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub caps: Capabilities,
}

impl Gpu {
    pub async fn new(window: Arc<Window>) -> Result<Self, GpuError> {
        let size = window.inner_size();

        // WebGPU is deliberately excluded on wasm32, not just left unrequested:
        // with the `webgpu` cargo feature compiled in, `Instance::new` commits
        // to the WebGPU-only JS backend whenever `navigator.gpu` merely exists
        // (even if it can't produce an adapter) and never falls back to
        // WebGL2. See the wasm32 dependency comment in engine/Cargo.toml.
        #[cfg(not(target_arch = "wasm32"))]
        let backends = wgpu::Backends::PRIMARY;
        #[cfg(target_arch = "wasm32")]
        let backends = wgpu::Backends::GL;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let surface = instance.create_surface(window)?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await?;

        let info = adapter.get_info();
        let adapter_limits = adapter.limits();
        let caps = probe_capabilities(&adapter, &adapter_limits);

        tracing::info!(
            adapter = %info.name,
            backend = ?info.backend,
            max_texture_dimension_2d = adapter_limits.max_texture_dimension_2d,
            max_texture_array_layers = adapter_limits.max_texture_array_layers,
            hdr_render_attachment = caps.hdr_format.is_some(),
            "gpu capability probe"
        );

        // The WebGL2 downlevel defaults are the floor this project is built
        // against on both backends; `using_resolution` raises anything the
        // real adapter can do better (native almost always can).
        let required_limits =
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter_limits);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("engine device"),
                required_limits,
                ..Default::default()
            })
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        // Deliberately not `surface_caps.formats[0]`: on WebGL2 that is a
        // non-sRGB Rgba8Unorm, while native Vulkan commonly reports an sRGB
        // format first, which would make the two targets differ in gamma.
        let format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| surface_caps.formats.first().copied())
            .ok_or(GpuError::NoSurfaceFormat)?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            caps,
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
}

fn probe_capabilities(adapter: &wgpu::Adapter, limits: &wgpu::Limits) -> Capabilities {
    let hdr_features = adapter.get_texture_format_features(wgpu::TextureFormat::Rgba16Float);
    let hdr_format = hdr_features
        .allowed_usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        .then_some(wgpu::TextureFormat::Rgba16Float);

    Capabilities {
        hdr_format,
        max_texture_dimension_2d: limits.max_texture_dimension_2d,
    }
}

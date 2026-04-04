use std::sync::Arc;

use wgpu::{
    Adapter, Device, Instance, InstanceDescriptor, PresentMode, Queue, RequestAdapterOptions,
    Surface, SurfaceConfiguration, TextureUsages,
};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

#[allow(dead_code)]
pub struct Buffer {
    pub raw: wgpu::Buffer,
    pub size: u64,
    usage: wgpu::BufferUsages,
    label: String,
}

#[allow(dead_code)]
impl Buffer {
    pub fn vertex(device: &Device, label: &str, data: &[u8]) -> Self {
        let usage = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST;
        let raw = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: data,
            usage,
        });
        Self {
            size: data.len() as u64,
            raw,
            usage,
            label: label.to_owned(),
        }
    }

    pub fn index_uninit(device: &Device, label: &str, byte_size: u64) -> Self {
        let usage = wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST;
        let raw = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: byte_size,
            usage,
            mapped_at_creation: false,
        });
        Self {
            size: byte_size,
            raw,
            usage,
            label: label.to_owned(),
        }
    }

    pub fn vertex_uninit(device: &Device, label: &str, byte_size: u64) -> Self {
        let usage = wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST;
        let raw = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: byte_size,
            usage,
            mapped_at_creation: false,
        });
        Self {
            size: byte_size,
            raw,
            usage,
            label: label.to_owned(),
        }
    }

    pub fn uniform(device: &Device, label: &str, data: &[u8]) -> Self {
        let usage = wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST;
        let raw = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: data,
            usage,
        });
        Self {
            size: data.len() as u64,
            raw,
            usage,
            label: label.to_owned(),
        }
    }

    /// Write data into the buffer. If data exceeds current capacity the buffer
    /// is recreated with 2x the required size.
    pub fn write(&mut self, device: &Device, queue: &Queue, data: &[u8]) {
        let needed = data.len() as u64;
        if needed > self.size {
            let new_size = needed.next_power_of_two();
            self.raw = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&self.label),
                size: new_size,
                usage: self.usage,
                mapped_at_creation: false,
            });
            self.size = new_size;
        }
        queue.write_buffer(&self.raw, 0, data);
    }
}

pub struct Texture {
    #[allow(dead_code)]
    pub raw: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    pub fn from_rgba(
        device: &Device,
        queue: &Queue,
        width: u32,
        height: u32,
        data: &[u8],
        label: &str,
    ) -> Self {
        assert_eq!(
            data.len(),
            (width * height * 4) as usize,
            "RGBA data length must equal width * height * 4"
        );

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let raw = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &raw,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );

        let view = raw.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some(&format!("{label}_sampler")),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Self { raw, view, sampler }
    }

    pub fn white_1x1(device: &Device, queue: &Queue) -> Self {
        Self::from_rgba(device, queue, 1, 1, &[255, 255, 255, 255], "white_1x1")
    }
}

pub struct GpuState {
    pub surface: Surface<'static>,
    pub device: Device,
    pub queue: Queue,
    pub config: SurfaceConfiguration,
    #[allow(dead_code)]
    adapter: Adapter,
}

impl GpuState {
    pub fn new(window: Arc<Window>) -> Self {
        tracing::debug!("Creating wgpu instance");
        let instance = Instance::new(&InstanceDescriptor::default());

        tracing::debug!("Creating surface");
        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create surface");

        tracing::debug!("Requesting adapter");
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("failed to find a suitable GPU adapter");

        tracing::info!(adapter = %adapter.get_info().name, "Selected GPU adapter");

        tracing::debug!("Requesting device and queue");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))
                .expect("failed to create device");

        let size = window.inner_size();
        let surface_caps = surface.get_capabilities(&adapter);
        let format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        tracing::debug!(?format, "Selected surface format");

        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        tracing::info!(
            width = config.width,
            height = config.height,
            "GPU state initialized"
        );

        Self {
            surface,
            device,
            queue,
            config,
            adapter,
        }
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            tracing::debug!(
                width = new_size.width,
                height = new_size.height,
                "Resizing surface"
            );
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

}

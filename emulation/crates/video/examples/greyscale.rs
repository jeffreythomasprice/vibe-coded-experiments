//! Renders a random 64x64 four-color greyscale image (2 bits per pixel) with a
//! random palette, scaled to fit the window while preserving aspect ratio.
//!
//! Run with: `cargo run -p video --example greyscale`

use std::error::Error;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use video::{
    BitOrder, Background, Color, PixelBuffer, PixelBufferRenderer, PixelFormat, RenderTarget,
    ScaleMode, SurfaceLayout, VideoOutput,
};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const IMAGE_W: u32 = 64;
const IMAGE_H: u32 = 64;

/// Minimal xorshift64* PRNG so the example needs no `rand` dependency.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed | 1) // avoid the all-zero state
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn next_u8(&mut self) -> u8 {
        (self.next_u64() >> 33) as u8
    }
}

/// Random palette of four colors and a 64x64 Grey2 image packed 4 pixels per byte.
fn random_frame() -> ([Color; 4], Vec<u8>) {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    let mut rng = Rng::new(seed);

    let palette = [
        Color::rgb(rng.next_u8(), rng.next_u8(), rng.next_u8()),
        Color::rgb(rng.next_u8(), rng.next_u8(), rng.next_u8()),
        Color::rgb(rng.next_u8(), rng.next_u8(), rng.next_u8()),
        Color::rgb(rng.next_u8(), rng.next_u8(), rng.next_u8()),
    ];

    // 4 pixels per byte -> 16 bytes per row, MSB-first packing.
    let row_bytes = (IMAGE_W / 4) as usize;
    let mut data = vec![0u8; row_bytes * IMAGE_H as usize];
    for y in 0..IMAGE_H {
        for x in 0..IMAGE_W {
            let index = (rng.next_u64() & 0b11) as u8;
            let shift = (3 - (x % 4)) * 2; // pixel 0 in the high bits
            data[(y * IMAGE_W / 4 + x / 4) as usize] |= index << shift;
        }
    }

    (palette, data)
}

struct Gpu {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: PixelBufferRenderer,
}

impl Gpu {
    async fn new(window: Arc<Window>) -> Result<Self, Box<dyn Error>> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .ok_or("no suitable GPU adapter found")?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("greyscale-example-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .or_else(|| caps.formats.first().copied())
            .ok_or("surface is not compatible with the selected adapter")?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: caps.present_modes[0],
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let mut renderer = PixelBufferRenderer::new(&device, format);
        renderer.set_scale_mode(ScaleMode::Fit);
        renderer.set_background(Background::default());

        let (palette, data) = random_frame();
        renderer.update(
            &device,
            &queue,
            &PixelBuffer {
                data: &data,
                width: IMAGE_W,
                height: IMAGE_H,
                stride: (IMAGE_W / 4) as usize,
                format: PixelFormat::Grey2 {
                    palette,
                    order: BitOrder::MsbFirst,
                },
            },
        )?;
        tracing::info!(?palette, "generated random 64x64 grey2 frame");

        Ok(Self {
            surface,
            device,
            queue,
            config,
            renderer,
        })
    }

    fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let layout = SurfaceLayout {
            width: self.config.width,
            height: self.config.height,
            format: self.config.format,
        };
        if let Err(err) = self.renderer.render(RenderTarget {
            device: &self.device,
            queue: &self.queue,
            view: &view,
            surface: layout,
        }) {
            tracing::error!(%err, "render failed");
        }
        frame.present();
        Ok(())
    }
}

#[derive(Default)]
struct App {
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // A non-square window so the aspect-preserving Fit produces visible bars.
        let attrs = Window::default_attributes()
            .with_title("video: random 64x64 grey2 (Fit)")
            .with_inner_size(LogicalSize::new(512, 384));
        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                tracing::error!(%err, "failed to create window");
                event_loop.exit();
                return;
            }
        };

        match pollster::block_on(Gpu::new(window.clone())) {
            Ok(gpu) => {
                self.gpu = Some(gpu);
                self.window = Some(window);
            }
            Err(err) => {
                tracing::error!(%err, "failed to initialize graphics");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                gpu.resize(size);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => match gpu.render() {
                Ok(()) => {}
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    let size = winit::dpi::PhysicalSize::new(gpu.config.width, gpu.config.height);
                    gpu.resize(size);
                }
                Err(wgpu::SurfaceError::OutOfMemory) => {
                    tracing::error!("surface out of memory, exiting");
                    event_loop.exit();
                }
                Err(err) => tracing::warn!(%err, "dropped frame"),
            },
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    common::init()?;
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    Ok(())
}

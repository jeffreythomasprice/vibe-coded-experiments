//! Run a Game Boy test ROM to a pass/fail verdict, headless or in a live window.
//!
//! Usage:
//!   cargo run -p gameboy --example run_rom -- <rom> [--headless] [--screenshot[=path]]
//!
//! `<rom>` is a path relative to the test-ROM checkout ([`TEST_ROMS_DIR`]) or an
//! absolute/working-directory path. Both modes run the emulator flat-out (no
//! real-time pacing); the window just mirrors what the harness sees each frame.
//!
//!   # windowed (default), watching live video:
//!   cargo run -p gameboy --example run_rom -- "cpu_instrs/individual/06-ld r,r.gb"
//!   # headless, saving a screenshot of the final frame:
//!   cargo run -p gameboy --example run_rom -- cpu_instrs/cpu_instrs.gb --headless --screenshot

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gameboy::testrom::{self, Screenshot, TestConfig, Verdict, TEST_ROMS_DIR};
use gameboy::{Cartridge, Emulator, SCREEN_HEIGHT, SCREEN_WIDTH};
use video::{
    Background, BitOrder, Color, PixelBuffer, PixelBufferRenderer, PixelFormat, RenderTarget,
    ScaleMode, SurfaceLayout, VideoOutput,
};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// Classic DMG four-shade palette, lightest (index 0) to darkest (index 3).
/// Mirrors `emulator::gfx::DMG_PALETTE` (not exported from that binary crate).
const DMG_PALETTE: [Color; 4] = [
    Color::rgb(0x9b, 0xbc, 0x0f),
    Color::rgb(0x8b, 0xac, 0x0f),
    Color::rgb(0x30, 0x62, 0x30),
    Color::rgb(0x0f, 0x38, 0x0f),
];

struct Args {
    rom: PathBuf,
    headless: bool,
    screenshot: Option<Option<PathBuf>>, // Some(None) = derive default path
}

fn parse_args() -> Result<Args, String> {
    let mut rom: Option<PathBuf> = None;
    let mut headless = false;
    let mut screenshot: Option<Option<PathBuf>> = None;

    for arg in std::env::args().skip(1) {
        if arg == "--headless" {
            headless = true;
        } else if arg == "--screenshot" {
            screenshot = Some(None);
        } else if let Some(path) = arg.strip_prefix("--screenshot=") {
            screenshot = Some(Some(PathBuf::from(path)));
        } else if arg.starts_with("--") {
            return Err(format!("unknown flag: {arg}"));
        } else if rom.is_none() {
            rom = Some(PathBuf::from(arg));
        } else {
            return Err(format!("unexpected extra argument: {arg}"));
        }
    }

    let rom = rom.ok_or_else(|| {
        "missing <rom> argument (path relative to the test-ROM dir, or absolute)".to_string()
    })?;
    Ok(Args {
        rom,
        headless,
        screenshot,
    })
}

/// Resolve `<rom>` as given (if it exists) or relative to [`TEST_ROMS_DIR`].
fn resolve_rom(rom: &Path) -> PathBuf {
    if rom.exists() {
        rom.to_path_buf()
    } else {
        Path::new(TEST_ROMS_DIR).join(rom)
    }
}

/// Where to write a screenshot: the given path, or `<rom-stem>.png` in the cwd.
fn screenshot_path(requested: Option<PathBuf>, rom: &Path) -> PathBuf {
    requested.unwrap_or_else(|| {
        let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("frame");
        PathBuf::from(format!("{stem}.png"))
    })
}

/// Encode a [`Screenshot`]'s shade indices through the DMG palette and write a
/// PNG. Uses the `png` dev-dependency; kept in the example so the core stays
/// image-free.
fn save_screenshot(path: &Path, shot: &Screenshot) -> Result<(), Box<dyn Error>> {
    let mut rgb = Vec::with_capacity(shot.shades.len() * 3);
    for &shade in &shot.shades {
        let color = DMG_PALETTE[(shade & 0x3) as usize];
        rgb.extend_from_slice(&[color.r, color.g, color.b]);
    }

    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), shot.width, shot.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgb)?;
    tracing::info!(path = %path.display(), "wrote screenshot");
    Ok(())
}

/// Print the verdict and serial log, returning the process exit code.
fn report(result: &Result<testrom::TestOutcome, testrom::TestError>) -> i32 {
    match result {
        Ok(outcome) => {
            if !outcome.serial_text.is_empty() {
                println!("--- serial output ---\n{}", outcome.serial_text.trim_end());
            }
            println!("PASSED in {} frames", outcome.frames);
            0
        }
        Err(err) => {
            let serial = err.serial_text();
            if !serial.is_empty() {
                println!("--- serial output ---\n{}", serial.trim_end());
            }
            println!("FAILED: {err}");
            1
        }
    }
}

fn run_headless(rom: &Path, screenshot: Option<Option<PathBuf>>) -> Result<i32, Box<dyn Error>> {
    let config = TestConfig {
        capture_screenshot: screenshot.is_some(),
        ..Default::default()
    };
    let result = testrom::run_rom_file(rom, &config);

    if let Some(requested) = screenshot {
        let shot = match &result {
            Ok(outcome) => outcome.screenshot.as_ref(),
            Err(err) => err.screenshot(),
        };
        if let Some(shot) = shot {
            save_screenshot(&screenshot_path(requested, rom), shot)?;
        }
    }
    Ok(report(&result))
}

// --- Windowed mode -----------------------------------------------------------

/// Pack `SCREEN_WIDTH * SCREEN_HEIGHT` shade indices into the `Grey2` layout the
/// video adapter wants (4 px/byte, MSB-first). Mirrors `emulator::gfx::pack_grey2`.
fn pack_grey2(shades: &[u8]) -> Vec<u8> {
    let w = SCREEN_WIDTH as usize;
    let h = SCREEN_HEIGHT as usize;
    let stride = w / 4;
    let mut data = vec![0u8; stride * h];
    for y in 0..h {
        for x in 0..w {
            let index = shades[y * w + x] & 0x3;
            let shift = (3 - (x % 4)) * 2;
            data[y * stride + x / 4] |= index << shift;
        }
    }
    data
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
                    label: Some("run_rom-device"),
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

        Ok(Self {
            surface,
            device,
            queue,
            config,
            renderer,
        })
    }

    fn upload(&mut self, shades: &[u8]) {
        let packed = pack_grey2(shades);
        if let Err(err) = self.renderer.update(
            &self.device,
            &self.queue,
            &PixelBuffer {
                data: &packed,
                width: SCREEN_WIDTH,
                height: SCREEN_HEIGHT,
                stride: (SCREEN_WIDTH / 4) as usize,
                format: PixelFormat::Grey2 {
                    palette: DMG_PALETTE,
                    order: BitOrder::MsbFirst,
                },
            },
        ) {
            tracing::error!(%err, "failed to upload frame");
        }
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

struct App {
    rom: PathBuf,
    screenshot: Option<Option<PathBuf>>,
    emu: Emulator,
    serial: Vec<u8>,
    done: bool,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
}

impl App {
    fn new(rom: PathBuf, screenshot: Option<Option<PathBuf>>, cart: Cartridge) -> App {
        App {
            rom,
            screenshot,
            emu: Emulator::new(cart),
            serial: Vec::new(),
            done: false,
            window: None,
            gpu: None,
        }
    }

    /// Advance one frame and fold in serial output; returns any fresh verdict.
    fn step_frame(&mut self) -> Option<Verdict> {
        let result = self.emu.run_frame();
        self.serial.extend(self.emu.take_serial_output());
        if let Some(fault) = result.fault {
            return Some(Verdict::Failed(format!("cpu faulted: {fault}")));
        }
        let text = String::from_utf8_lossy(&self.serial);
        testrom::detect(&text, self.emu.cartridge())
    }

    fn finish(&mut self, verdict: Verdict) {
        self.done = true;
        let serial = String::from_utf8_lossy(&self.serial);
        if !serial.trim().is_empty() {
            println!("--- serial output ---\n{}", serial.trim_end());
        }
        match &verdict {
            Verdict::Passed => println!("PASSED"),
            Verdict::Failed(message) => println!("FAILED: {message}"),
        }

        if let Some(requested) = self.screenshot.take() {
            let shot = Screenshot {
                width: SCREEN_WIDTH,
                height: SCREEN_HEIGHT,
                shades: self.emu.framebuffer().to_vec(),
            };
            if let Err(err) = save_screenshot(&screenshot_path(requested, &self.rom), &shot) {
                tracing::error!(%err, "failed to save screenshot");
            }
        }
        println!("verdict reached; close the window to exit");
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("run_rom")
            .with_inner_size(LogicalSize::new(SCREEN_WIDTH * 4, SCREEN_HEIGHT * 4));
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_none() {
            return;
        }
        if !self.done {
            // Run a batch of frames per wake-up so the emulator races ahead of the
            // ~60 Hz redraw rather than being throttled to it.
            for _ in 0..16 {
                if let Some(verdict) = self.step_frame() {
                    self.finish(verdict);
                    break;
                }
            }
            if let Some(gpu) = self.gpu.as_mut() {
                gpu.upload(self.emu.framebuffer());
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        // Keep polling while running; idle (Wait) once finished so we don't spin.
        event_loop.set_control_flow(if self.done {
            ControlFlow::Wait
        } else {
            ControlFlow::Poll
        });
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

fn run_windowed(rom: &Path, screenshot: Option<Option<PathBuf>>) -> Result<(), Box<dyn Error>> {
    let bytes = std::fs::read(rom)?;
    let cart = Cartridge::from_bytes(&bytes)?;
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(rom.to_path_buf(), screenshot, cart);
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    common::init()?;
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!(
                "usage: run_rom <rom> [--headless] [--screenshot[=path]]\n\
                 <rom> is relative to {TEST_ROMS_DIR} or an absolute/cwd path"
            );
            std::process::exit(2);
        }
    };

    let rom = resolve_rom(&args.rom);
    if !rom.exists() {
        eprintln!("error: ROM not found: {}", rom.display());
        std::process::exit(2);
    }
    tracing::info!(rom = %rom.display(), headless = args.headless, "running test ROM");

    if args.headless {
        let code = run_headless(&rom, args.screenshot)?;
        std::process::exit(code);
    } else {
        run_windowed(&rom, args.screenshot)?;
        Ok(())
    }
}

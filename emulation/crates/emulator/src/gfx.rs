use std::sync::Arc;

use common::ScaleMode;
use thiserror::Error;
use video::{
    Background, BitOrder, Color, EguiPaint, EguiRenderer, PixelBuffer, PixelBufferRenderer,
    PixelFormat, RenderTarget, SurfaceLayout, VideoOutput,
};
use winit::window::Window;

use gameboy::{SCREEN_HEIGHT, SCREEN_WIDTH};

/// Classic DMG four-shade palette, lightest (index 0) to darkest (index 3).
const DMG_PALETTE: [Color; 4] = [
    Color::rgb(0x9b, 0xbc, 0x0f),
    Color::rgb(0x8b, 0xac, 0x0f),
    Color::rgb(0x30, 0x62, 0x30),
    Color::rgb(0x0f, 0x38, 0x0f),
];

#[derive(Debug, Error)]
pub enum GfxError {
    #[error("failed to create surface: {0}")]
    CreateSurface(#[from] wgpu::CreateSurfaceError),
    #[error("no suitable GPU adapter found")]
    NoAdapter,
    #[error("failed to request device: {0}")]
    RequestDevice(#[from] wgpu::RequestDeviceError),
    #[error("surface is not compatible with the selected adapter")]
    IncompatibleSurface,
    #[error("failed to upload frame: {0}")]
    Video(#[from] video::VideoError),
}

/// Owns all the wgpu machinery plus the [`PixelBufferRenderer`] that scales an
/// emulator framebuffer onto the surface.
///
/// There is no live PPU output yet, so the renderer is fed a static placeholder
/// frame; this is the seam where the emulator's real framebuffer will be uploaded
/// each frame once the core produces one. The scale mode comes from settings.
pub struct Gfx {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: PixelBufferRenderer,
    egui: EguiRenderer,
    /// Linear (non-sRGB) view format for the egui overlay pass.
    egui_format: wgpu::TextureFormat,
}

impl Gfx {
    pub async fn new(window: Arc<Window>, scale_mode: ScaleMode) -> Result<Self, GfxError> {
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
            .ok_or(GfxError::NoAdapter)?;

        tracing::info!(adapter = ?adapter.get_info(), "selected GPU adapter");

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("emulator-device"),
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
            .ok_or(GfxError::IncompatibleSurface)?;

        // egui renders best into a non-sRGB target (it applies gamma itself); the
        // game wants the sRGB surface for correct colors. So the egui pass draws
        // through a linear (non-sRGB) *view* of the same surface texture, which
        // requires that view format to be allowed on the surface.
        let egui_format = format.remove_srgb_suffix();
        let view_formats = if egui_format != format {
            vec![egui_format]
        } else {
            vec![]
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: caps.present_modes[0],
            alpha_mode: caps.alpha_modes[0],
            view_formats,
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let mut renderer = PixelBufferRenderer::new(&device, format);
        renderer.set_scale_mode(scale_mode);
        renderer.set_background(Background::default());
        tracing::info!(?scale_mode, "configured video scale mode");

        blit_shades(&mut renderer, &device, &queue, &placeholder_shades())?;

        let egui = EguiRenderer::new(&device, egui_format);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            renderer,
            egui,
            egui_format,
        })
    }

    /// Upload a live PPU frame: `shades` is `SCREEN_WIDTH * SCREEN_HEIGHT` shade
    /// indices (`0-3`, row-major) as produced by the Game Boy PPU. This is the
    /// seam a run loop calls each time the core completes a frame, replacing the
    /// startup placeholder.
    pub fn update_frame(&mut self, shades: &[u8]) -> Result<(), GfxError> {
        blit_shades(&mut self.renderer, &self.device, &self.queue, shades)
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        tracing::trace!(?new_size, "reconfigured surface");
    }

    /// Present the last-uploaded emulator frame, optionally with an egui overlay
    /// (the menu) drawn on top. The overlay is loaded onto the game image, so it
    /// composits over it rather than clearing it.
    pub fn render(&mut self, egui: Option<&EguiPaint>) -> Result<(), wgpu::SurfaceError> {
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
            tracing::error!(%err, "frame render failed");
        }

        if let Some(paint) = egui {
            // Draw egui through a linear view of the same texture (see `egui_format`).
            let egui_view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(self.egui_format),
                ..Default::default()
            });
            self.egui.render(
                RenderTarget {
                    device: &self.device,
                    queue: &self.queue,
                    view: &egui_view,
                    surface: layout,
                },
                paint,
            );
        }

        frame.present();
        Ok(())
    }

    pub fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }
}

/// Pack a `SCREEN_WIDTH * SCREEN_HEIGHT` buffer of 2-bit shade indices into the
/// `Grey2` layout the video adapter wants: 4 pixels per byte, MSB-first (pixel 0
/// in the high bits), `SCREEN_WIDTH / 4` bytes per row.
fn pack_grey2(shades: &[u8]) -> Vec<u8> {
    let w = SCREEN_WIDTH as usize;
    let h = SCREEN_HEIGHT as usize;
    debug_assert_eq!(
        shades.len(),
        w * h,
        "shade buffer must be one byte per pixel"
    );
    let stride = w / 4;
    let mut data = vec![0u8; stride * h];
    for y in 0..h {
        for x in 0..w {
            let index = shades[y * w + x] & 0x3;
            let shift = (3 - (x % 4)) * 2; // pixel 0 in the high bits
            data[y * stride + x / 4] |= index << shift;
        }
    }
    data
}

/// Pack `shades` and upload them through the renderer as a DMG-palette `Grey2`
/// frame. Shared by the startup placeholder and live frame uploads.
fn blit_shades(
    renderer: &mut PixelBufferRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    shades: &[u8],
) -> Result<(), GfxError> {
    let packed = pack_grey2(shades);
    renderer.update(
        device,
        queue,
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
    )?;
    Ok(())
}

/// A static 160x144 shade-index placeholder: a one-pixel border in the darkest
/// shade framing the lightest shade, so the configured scale mode is visible
/// before a live PPU frame arrives. One byte per pixel (`0-3`).
fn placeholder_shades() -> Vec<u8> {
    let w = SCREEN_WIDTH as usize;
    let h = SCREEN_HEIGHT as usize;
    let mut data = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let border = x == 0 || y == 0 || x == w - 1 || y == h - 1;
            data[y * w + x] = if border { 3 } else { 0 };
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_grey2_packs_four_pixels_per_byte_msb_first() {
        let mut shades = vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize];
        // First four pixels of row 0: indices 0,1,2,3 → 0b00_01_10_11.
        shades[0] = 0;
        shades[1] = 1;
        shades[2] = 2;
        shades[3] = 3;
        let packed = pack_grey2(&shades);
        assert_eq!(packed[0], 0b00_01_10_11);
        assert_eq!(packed.len(), (SCREEN_WIDTH / 4 * SCREEN_HEIGHT) as usize);
    }
}

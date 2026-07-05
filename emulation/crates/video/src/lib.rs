//! Generic wgpu video output for emulator cores.
//!
//! [`VideoOutput`] is the system-agnostic trait a UI shell drives to present a frame.
//! [`PixelBufferRenderer`] is the first (and, for now, only) implementation: a generic
//! adapter for systems that own their video memory as a grid of pixels. It accepts a
//! [`PixelBuffer`] in any supported [`PixelFormat`], converts it to RGBA8, and blits it
//! to the surface scaled per a [`ScaleMode`] with a configurable letterbox [`Background`].

mod format;
mod pixel_buffer;
mod scale;

pub use common::{ScaleMode, Transform};
pub use format::{BitOrder, Color, FormatError, PixelBuffer, PixelFormat};
pub use pixel_buffer::PixelBufferRenderer;
pub use scale::Background;

/// How the physical drawable describes itself to a [`VideoOutput`]: the size and
/// format of the surface texture being rendered into.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceLayout {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
}

/// Everything a [`VideoOutput`] needs to draw one frame into the acquired surface
/// texture. The host is responsible for acquiring the surface texture and presenting;
/// the video output records and submits its own draw commands against `view`.
pub struct RenderTarget<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    pub surface: SurfaceLayout,
}

/// Any emulator-specific system that can emit a frame to the display via wgpu.
///
/// The trait covers only drawing. How a frame's pixels are supplied is left to the
/// concrete type (e.g. [`PixelBufferRenderer::update`]), so a shell can drive
/// `render` uniformly while system-specific code feeds pixels.
pub trait VideoOutput {
    fn render(&mut self, target: RenderTarget<'_>) -> Result<(), VideoError>;
}

#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    #[error("failed to convert source pixels: {0}")]
    Format(#[from] FormatError),
}

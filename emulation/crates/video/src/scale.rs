//! The letterbox / pillarbox fill color.
//!
//! The scale modes and placement math ([`common::ScaleMode`], [`common::Transform`])
//! live in the `common` crate so they can be both the renderer's runtime type and
//! the value persisted in settings. Only the fill color stays here, because it
//! wraps a [`wgpu::Color`] and would otherwise pull `wgpu` into `common`.

/// The fill for surface area not covered by the frame (letterbox / pillarbox bars).
///
/// Wraps a [`wgpu::Color`], whose components are interpreted in linear space by the
/// render pass clear. Defaults to opaque black.
#[derive(Clone, Copy, Debug)]
pub struct Background(pub wgpu::Color);

impl Default for Background {
    fn default() -> Self {
        Background(wgpu::Color::BLACK)
    }
}

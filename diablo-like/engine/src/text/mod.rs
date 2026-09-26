mod atlas;

pub use atlas::Atlas;

use bytemuck::{Pod, Zeroable};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TextError {
    #[error("failed to parse font: {0}")]
    FontParse(&'static str),
}

const FONT_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../assets/fonts/tr2n/Tr2n.ttf"
));

pub struct Font {
    inner: fontdue::Font,
}

impl Font {
    pub fn load() -> Result<Self, TextError> {
        let inner = fontdue::Font::from_bytes(FONT_BYTES, fontdue::FontSettings::default())
            .map_err(TextError::FontParse)?;
        Ok(Self { inner })
    }

    pub fn build_atlas(&self, device: &wgpu::Device, queue: &wgpu::Queue, px: f32) -> Atlas {
        atlas::build(device, queue, &self.inner, px)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TextVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl TextVertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<TextVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Lays out `text` as a left-aligned, top-anchored run starting at
/// `(origin_x, origin_y)` in screen pixels (origin top-left, y down), and
/// emits its glyph quads directly as NDC-space triangle-list vertices sized
/// for `(screen_w, screen_h)`. Rebuilt fresh each frame; HUD-scale text is
/// far too little geometry to justify caching.
pub fn layout(
    atlas: &Atlas,
    screen_w: f32,
    screen_h: f32,
    origin_x: f32,
    origin_y: f32,
    text: &str,
) -> Vec<TextVertex> {
    let mut vertices = Vec::with_capacity(text.len() * 6);
    let baseline_y = origin_y + atlas.ascent;
    let mut pen_x = origin_x;

    let to_ndc = |x: f32, y: f32| -> [f32; 2] {
        [(x / screen_w) * 2.0 - 1.0, 1.0 - (y / screen_h) * 2.0]
    };

    for ch in text.chars() {
        let Some(glyph) = atlas.glyph(ch) else {
            continue;
        };

        let [w, h] = glyph.size_px;
        if w > 0.0 && h > 0.0 {
            let [bx, by] = glyph.bearing;
            let left = pen_x + bx;
            let top = baseline_y - (by + h);
            let right = left + w;
            let bottom = top + h;

            let tl = to_ndc(left, top);
            let tr = to_ndc(right, top);
            let bl = to_ndc(left, bottom);
            let br = to_ndc(right, bottom);

            let [u0, v0] = glyph.uv_min;
            let [u1, v1] = glyph.uv_max;

            vertices.push(TextVertex { position: tl, uv: [u0, v0] });
            vertices.push(TextVertex { position: bl, uv: [u0, v1] });
            vertices.push(TextVertex { position: tr, uv: [u1, v0] });
            vertices.push(TextVertex { position: tr, uv: [u1, v0] });
            vertices.push(TextVertex { position: bl, uv: [u0, v1] });
            vertices.push(TextVertex { position: br, uv: [u1, v1] });
        }

        pen_x += glyph.advance;
    }

    vertices
}

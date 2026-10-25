use std::ops::Range;

use bytemuck::{Pod, Zeroable};

use super::atlas::Atlas;
use crate::config::theme::Color;
use crate::hex::Point;
use crate::prim::{IconPrim, Layer, RectPrim, TextPrim};
use crate::scene::HexInstance;
use crate::text::Fonts;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct HexInst {
    pub center: [f32; 2],
    pub radius_width: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct MeshVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct RectInst {
    pub rect: [f32; 4],
    pub fill: [f32; 4],
    pub border: [f32; 4],
    pub params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GlyphInst {
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub color: [f32; 4],
}

#[derive(Debug, Clone, Default)]
pub struct LayerRanges {
    pub rects: Range<u32>,
    pub indices: Range<u32>,
    pub glyphs: Range<u32>,
}

#[derive(Default)]
pub struct Batch {
    pub hexes: Vec<HexInst>,
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
    pub rects: Vec<RectInst>,
    pub glyphs: Vec<GlyphInst>,
}

const OUTLINE_OFFSETS: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (0.707, 0.707),
    (-0.707, 0.707),
    (0.707, -0.707),
    (-0.707, -0.707),
];

fn f32x2(p: Point) -> [f32; 2] {
    [p.x as f32, p.y as f32]
}

impl Batch {
    pub fn push_hex(&mut self, hex: &HexInstance) {
        self.hexes.push(HexInst {
            center: f32x2(hex.center),
            radius_width: [hex.radius as f32, hex.border_width as f32],
            color: hex.color.to_array(),
        });
    }

    pub fn push_icon(&mut self, icon: &IconPrim) {
        let base = self.vertices.len() as u32;
        let center = f32x2(icon.center);
        let radius = icon.radius as f32;
        self.vertices
            .extend(icon.mesh.vertices.iter().map(|(q, color)| {
                let color = icon.tint.unwrap_or(*color).scale_alpha(icon.opacity);
                MeshVertex {
                    position: [
                        center[0] + q.x as f32 * radius,
                        center[1] + q.y as f32 * radius,
                    ],
                    color: color.to_array(),
                }
            }));
        self.indices
            .extend(icon.mesh.indices.iter().map(|i| base + i));
    }

    pub fn push_rect(&mut self, prim: &RectPrim) {
        self.rects.push(RectInst {
            rect: [
                prim.rect.min.x as f32,
                prim.rect.min.y as f32,
                prim.rect.size.x as f32,
                prim.rect.size.y as f32,
            ],
            fill: prim.fill.to_array(),
            border: prim.border.to_array(),
            params: [prim.border_width as f32, prim.radius as f32, 0.0, 0.0],
        });
    }

    pub fn push_text(
        &mut self,
        prim: &TextPrim,
        atlas: &mut Atlas,
        queue: &wgpu::Queue,
        fonts: &Fonts,
        scale: f32,
    ) {
        if let Some((color, radius)) = prim.outline {
            for (dx, dy) in OUTLINE_OFFSETS {
                let offset = (dx * radius as f32, dy * radius as f32);
                self.push_glyphs(prim, color, offset, atlas, queue, fonts, scale);
            }
        }
        self.push_glyphs(prim, prim.color, (0.0, 0.0), atlas, queue, fonts, scale);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_glyphs(
        &mut self,
        prim: &TextPrim,
        color: Color,
        offset: (f32, f32),
        atlas: &mut Atlas,
        queue: &wgpu::Queue,
        fonts: &Fonts,
        scale: f32,
    ) {
        for glyph in &prim.glyphs {
            let Some(entry) = atlas.glyph(queue, fonts, glyph.weight, glyph.id, glyph.px * scale)
            else {
                continue;
            };
            let pen_x = ((glyph.pos.x as f32 + offset.0) * scale).round();
            let pen_y = ((glyph.pos.y as f32 + offset.1) * scale).round();
            let x = (pen_x + entry.offset[0]) / scale;
            let y = (pen_y + entry.offset[1]) / scale;
            self.glyphs.push(GlyphInst {
                rect: [x, y, entry.size[0] / scale, entry.size[1] / scale],
                uv: [
                    entry.uv_min[0],
                    entry.uv_min[1],
                    entry.uv_max[0],
                    entry.uv_max[1],
                ],
                color: color.to_array(),
            });
        }
    }

    pub fn push_layer(
        &mut self,
        layer: &Layer,
        atlas: &mut Atlas,
        queue: &wgpu::Queue,
        fonts: &Fonts,
        scale: f32,
    ) -> LayerRanges {
        let rects = self.rects.len() as u32;
        let indices = self.indices.len() as u32;
        let glyphs = self.glyphs.len() as u32;
        for rect in &layer.rects {
            self.push_rect(rect);
        }
        for icon in &layer.icons {
            self.push_icon(icon);
        }
        for text in &layer.texts {
            self.push_text(text, atlas, queue, fonts, scale);
        }
        LayerRanges {
            rects: rects..self.rects.len() as u32,
            indices: indices..self.indices.len() as u32,
            glyphs: glyphs..self.glyphs.len() as u32,
        }
    }
}

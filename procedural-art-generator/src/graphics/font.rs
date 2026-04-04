use std::collections::HashMap;

use fontdue::{Font, FontSettings};
use glam::Vec2;

use super::atlas::{AtlasImage, TextureAtlas};
use super::immediate::{Color, Frame, TextureMaterialGuard, TextureVertex2D};
use super::vector::Mesh;

#[derive(Debug, Clone, Copy)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy)]
pub enum VAlign {
    Top,
    Center,
    Bottom,
}

struct GlyphInfo {
    metrics: fontdue::Metrics,
}

pub struct TextureAtlasFont {
    font: Font,
    atlas: TextureAtlas,
    glyphs: HashMap<char, GlyphInfo>,
    font_size: f32,
    ascent: f32,
    line_height: f32,
}

impl TextureAtlasFont {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        font_data: &[u8],
        font_size: f32,
        chars: &[char],
    ) -> Self {
        let font = Font::from_bytes(font_data, FontSettings::default()).expect("failed to load font");

        let line_metrics = font
            .horizontal_line_metrics(font_size)
            .expect("font has no horizontal line metrics");
        let ascent = line_metrics.ascent;
        let line_height = line_metrics.ascent - line_metrics.descent + line_metrics.line_gap;

        let mut glyphs = HashMap::new();
        let mut atlas_images = HashMap::new();

        for &ch in chars {
            let (metrics, bitmap) = font.rasterize(ch, font_size);

            if metrics.width > 0 && metrics.height > 0 {
                let mut rgba = Vec::with_capacity(bitmap.len() * 4);
                for &coverage in &bitmap {
                    rgba.push(255);
                    rgba.push(255);
                    rgba.push(255);
                    rgba.push(coverage);
                }

                atlas_images.insert(
                    ch.to_string(),
                    AtlasImage {
                        width: metrics.width as u32,
                        height: metrics.height as u32,
                        rgba,
                    },
                );
            }

            glyphs.insert(ch, GlyphInfo { metrics });
        }

        let atlas = TextureAtlas::new(device, queue, atlas_images);

        Self {
            font,
            atlas,
            glyphs,
            font_size,
            ascent,
            line_height,
        }
    }

    pub fn from_ascii(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        font_data: &[u8],
        font_size: f32,
    ) -> Self {
        let chars: Vec<char> = (32u8..=126u8).map(|b| b as char).collect();
        Self::new(device, queue, font_data, font_size, &chars)
    }

    pub fn material<'a, 'f>(&'a self, frame: &'f mut Frame<'a>) -> FontMaterialGuard<'a, 'f> {
        let guard = frame.texture_material(&self.atlas.texture, Color::WHITE);
        FontMaterialGuard { font: self, guard }
    }

    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    pub fn measure(&self, text: &str) -> Vec2 {
        let lines: Vec<&str> = text.split('\n').collect();
        let max_width = lines.iter().map(|l| self.measure_line(l)).fold(0.0f32, f32::max);
        let total_height = lines.len() as f32 * self.line_height;
        Vec2::new(max_width, total_height)
    }

    fn measure_line(&self, line: &str) -> f32 {
        let mut width = 0.0f32;
        let mut prev: Option<char> = None;
        for ch in line.chars() {
            if let Some(prev_ch) = prev {
                if let Some(kern) = self.font.horizontal_kern(prev_ch, ch, self.font_size) {
                    width += kern;
                }
            }
            if let Some(glyph) = self.glyphs.get(&ch) {
                width += glyph.metrics.advance_width;
            }
            prev = Some(ch);
        }
        width
    }

    pub fn mesh(
        &self,
        text: &str,
        rect_pos: Vec2,
        rect_size: Vec2,
        h_align: HAlign,
        v_align: VAlign,
    ) -> Mesh<TextureVertex2D> {
        let lines: Vec<&str> = text.split('\n').collect();
        let line_widths: Vec<f32> = lines.iter().map(|l| self.measure_line(l)).collect();
        let total_height = lines.len() as f32 * self.line_height;

        let y_start = match v_align {
            VAlign::Top => rect_pos.y,
            VAlign::Center => rect_pos.y + (rect_size.y - total_height) / 2.0,
            VAlign::Bottom => rect_pos.y + rect_size.y - total_height,
        };

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for (line_idx, line) in lines.iter().enumerate() {
            let x_start = match h_align {
                HAlign::Left => rect_pos.x,
                HAlign::Center => rect_pos.x + (rect_size.x - line_widths[line_idx]) / 2.0,
                HAlign::Right => rect_pos.x + rect_size.x - line_widths[line_idx],
            };

            let mut cursor_x = x_start;
            let cursor_y = y_start + line_idx as f32 * self.line_height;
            let baseline_y = cursor_y + self.ascent;
            let mut prev: Option<char> = None;

            for ch in line.chars() {
                if let Some(prev_ch) = prev {
                    if let Some(kern) = self.font.horizontal_kern(prev_ch, ch, self.font_size) {
                        cursor_x += kern;
                    }
                }

                if let Some(glyph) = self.glyphs.get(&ch) {
                    let m = &glyph.metrics;

                    if m.width > 0 && m.height > 0 {
                        if let Some(entry) = self.atlas.get(&ch.to_string()) {
                            let glyph_x = cursor_x + m.xmin as f32;
                            let glyph_y = baseline_y - m.height as f32 - m.ymin as f32;
                            let glyph_w = m.width as f32;
                            let glyph_h = m.height as f32;

                            let base = vertices.len() as u16;

                            vertices.push(TextureVertex2D::new(
                                Vec2::new(glyph_x, glyph_y),
                                Vec2::new(entry.uv.u0, entry.uv.v0),
                                Color::WHITE,
                            ));
                            vertices.push(TextureVertex2D::new(
                                Vec2::new(glyph_x + glyph_w, glyph_y),
                                Vec2::new(entry.uv.u1, entry.uv.v0),
                                Color::WHITE,
                            ));
                            vertices.push(TextureVertex2D::new(
                                Vec2::new(glyph_x + glyph_w, glyph_y + glyph_h),
                                Vec2::new(entry.uv.u1, entry.uv.v1),
                                Color::WHITE,
                            ));
                            vertices.push(TextureVertex2D::new(
                                Vec2::new(glyph_x, glyph_y + glyph_h),
                                Vec2::new(entry.uv.u0, entry.uv.v1),
                                Color::WHITE,
                            ));

                            indices.extend_from_slice(&[
                                base,
                                base + 1,
                                base + 2,
                                base,
                                base + 2,
                                base + 3,
                            ]);
                        }
                    }

                    cursor_x += m.advance_width;
                }

                prev = Some(ch);
            }
        }

        Mesh { vertices, indices }
    }

}

pub struct FontMaterialGuard<'a, 'f> {
    font: &'a TextureAtlasFont,
    guard: TextureMaterialGuard<'a, 'f>,
}

impl FontMaterialGuard<'_, '_> {
    pub fn draw(
        &mut self,
        text: &str,
        rect_pos: Vec2,
        rect_size: Vec2,
        h_align: HAlign,
        v_align: VAlign,
    ) {
        let mesh = self.font.mesh(text, rect_pos, rect_size, h_align, v_align);
        if !mesh.vertices.is_empty() {
            self.guard.indexed_triangle_list(&mesh.vertices, &mesh.indices);
        }
    }
}

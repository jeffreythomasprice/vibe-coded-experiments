use std::sync::Arc;

use glam::{Vec2, Vec4};

use crate::gfx::{Mesh, Texture, Vertex2d};

pub struct Text {
    font: ab_glyph::FontArc,
    scale: f32,
    pub texture: Arc<Texture>,
}

impl Text {
    pub fn new(device: &wgpu::Device, font: ab_glyph::FontArc, scale: f32) -> Self {
        let texture = Arc::new(Texture::new_with_size(device, 1, 1));
        Self {
            font,
            scale,
            texture,
        }
    }

    pub fn ascent(&self) -> f32 {
        use ab_glyph::{Font, PxScale, ScaleFont};
        self.font.as_scaled(PxScale::from(self.scale)).ascent()
    }

    pub fn measure_width(&self, s: &str) -> f32 {
        use ab_glyph::{Font, PxScale, ScaleFont};
        let px_scale = PxScale::from(self.scale);
        let scaled = self.font.as_scaled(px_scale);
        let mut pen_x = 0.0f32;
        let mut prev_id: Option<ab_glyph::GlyphId> = None;
        for c in s.chars() {
            let id = self.font.glyph_id(c);
            if let Some(prev) = prev_id {
                pen_x += scaled.kern(prev, id);
            }
            pen_x += scaled.h_advance(id);
            prev_id = Some(id);
        }
        pen_x
    }

    pub fn render_text(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mesh: &mut Mesh<Vertex2d>, s: &str) {
        use ab_glyph::{Font, PxScale, ScaleFont};

        let px_scale = PxScale::from(self.scale);
        let scaled = self.font.as_scaled(px_scale);
        let line_height = scaled.ascent() - scaled.descent() + scaled.line_gap();

        let mut outlines: Vec<ab_glyph::OutlinedGlyph> = Vec::new();
        let mut pen_x = 0.0f32;
        let mut pen_y = 0.0f32;
        let mut prev_id: Option<ab_glyph::GlyphId> = None;

        for c in s.chars() {
            if c == '\n' {
                pen_x = 0.0;
                pen_y += line_height;
                prev_id = None;
                continue;
            }
            let id = self.font.glyph_id(c);
            if let Some(prev) = prev_id {
                pen_x += scaled.kern(prev, id);
            }
            let glyph = id.with_scale_and_position(px_scale, ab_glyph::point(pen_x, pen_y));
            pen_x += scaled.h_advance(id);
            prev_id = Some(id);
            if let Some(og) = self.font.outline_glyph(glyph) {
                outlines.push(og);
            }
        }

        if outlines.is_empty() {
            return;
        }

        let mut tex_offsets: Vec<u32> = Vec::with_capacity(outlines.len());
        let mut atlas_x = 0u32;
        let mut atlas_height = 0u32;
        for og in &outlines {
            let b = og.px_bounds();
            let w = b.width().ceil() as u32;
            let h = b.height().ceil() as u32;
            tex_offsets.push(atlas_x);
            atlas_x += w;
            atlas_height = atlas_height.max(h);
        }
        let atlas_width = atlas_x;

        let mut pixels = vec![0u8; (atlas_width * atlas_height * 4) as usize];
        for (og, &tx) in outlines.iter().zip(tex_offsets.iter()) {
            og.draw(|x, y, c| {
                let i = ((y * atlas_width + tx + x) * 4) as usize;
                pixels[i] = 255;
                pixels[i + 1] = 255;
                pixels[i + 2] = 255;
                pixels[i + 3] = (c * 255.0) as u8;
            });
        }

        if atlas_width <= self.texture.width() && atlas_height <= self.texture.height() {
            self.texture.update_with_image_bytes(queue, 0, 0, atlas_width, atlas_height, &pixels);
        } else {
            self.texture = Arc::new(Texture::new_with_image_bytes(device, queue, atlas_width, atlas_height, &pixels));
        }

        let needed_verts = outlines.len() * 4;
        let needed_idxs = outlines.len() * 6;
        if needed_verts <= mesh.vertex_capacity() && needed_idxs <= mesh.index_capacity() {
            mesh.reset();
        } else {
            *mesh = Mesh::new(device, needed_verts, needed_idxs);
        }

        let tex_w = self.texture.width() as f32;
        let tex_h = self.texture.height() as f32;
        for (og, &tx) in outlines.iter().zip(tex_offsets.iter()) {
            let b = og.px_bounds();
            let gw = b.width().ceil() as u32;
            let gh = b.height().ceil() as u32;
            let u0 = tx as f32 / tex_w;
            let u1 = (tx + gw) as f32 / tex_w;
            let v0 = 0.0f32;
            let v1 = gh as f32 / tex_h;
            mesh.append_quad(
                queue,
                Vertex2d { position: Vec2::new(b.min.x, b.min.y), uv: Vec2::new(u0, v0), color: Vec4::ONE },
                Vertex2d { position: Vec2::new(b.max.x, b.min.y), uv: Vec2::new(u1, v0), color: Vec4::ONE },
                Vertex2d { position: Vec2::new(b.max.x, b.max.y), uv: Vec2::new(u1, v1), color: Vec4::ONE },
                Vertex2d { position: Vec2::new(b.min.x, b.max.y), uv: Vec2::new(u0, v1), color: Vec4::ONE },
            );
        }
    }
}

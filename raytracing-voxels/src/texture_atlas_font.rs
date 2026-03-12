use std::collections::HashMap;

use ab_glyph::{Font, FontArc, ScaleFont};
use anyhow::{Context, Result};

use crate::overlay::{DrawList, Rgba, Texture};
use crate::texture_atlas::{TextureAtlas, TextureAtlasBuilder};

pub struct GlyphInfo {
    pub atlas_name: String,
    pub advance_width: f32,
    pub left_side_bearing: f32,
    pub ascent_offset: f32,
    pub width: f32,
    pub height: f32,
}

pub struct TextureAtlasFont {
    atlas: TextureAtlas,
    glyphs: HashMap<char, GlyphInfo>,
    line_height: f32,
    ascent: f32,
    font: FontArc,
    px_size: f32,
}

impl TextureAtlasFont {
    pub fn new(font_data: &[u8], px_size: f32, charset: &str) -> Result<Self> {
        let font =
            FontArc::try_from_vec(font_data.to_vec()).context("failed to parse font")?;
        let scaled = font.as_scaled(px_size);

        let mut builder = TextureAtlasBuilder::new();
        let mut glyphs = HashMap::new();

        for ch in charset.chars() {
            let glyph_id = font.glyph_id(ch);
            let h_advance = scaled.h_advance(glyph_id);
            let h_side_bearing = scaled.h_side_bearing(glyph_id);

            let glyph = glyph_id.with_scale(px_size);
            if let Some(outlined) = scaled.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let w = bounds.width() as u32;
                let h = bounds.height() as u32;

                if w > 0 && h > 0 {
                    let mut tex = Texture::new(w, h);
                    outlined.draw(|px, py, coverage| {
                        let a = (coverage * 255.0) as u8;
                        tex.set_pixel(px, py, Rgba::new(255, 255, 255, a));
                    });

                    let name = ch.to_string();
                    builder.add(name.clone(), tex);

                    glyphs.insert(
                        ch,
                        GlyphInfo {
                            atlas_name: name,
                            advance_width: h_advance,
                            left_side_bearing: h_side_bearing,
                            ascent_offset: bounds.min.y,
                            width: w as f32,
                            height: h as f32,
                        },
                    );
                    continue;
                }
            }

            glyphs.insert(
                ch,
                GlyphInfo {
                    atlas_name: String::new(),
                    advance_width: h_advance,
                    left_side_bearing: 0.0,
                    ascent_offset: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
            );
        }

        let atlas = builder.build().context("failed to build font atlas")?;
        let line_height = scaled.height();
        let ascent = scaled.ascent();

        Ok(Self {
            atlas,
            glyphs,
            line_height,
            ascent,
            font,
            px_size,
        })
    }

    pub fn atlas(&self) -> &TextureAtlas {
        &self.atlas
    }

    #[allow(dead_code)]
    pub fn line_height(&self) -> f32 {
        self.line_height
    }

    pub fn draw_text(&self, draw_list: &mut DrawList, text: &str, x: f32, y: f32, color: Rgba) {
        let scaled = self.font.as_scaled(self.px_size);
        let mut cursor_x = x;
        let baseline_y = y + self.ascent;
        let mut prev_glyph_id = None;

        for ch in text.chars() {
            let glyph_id = self.font.glyph_id(ch);

            if let Some(prev) = prev_glyph_id {
                cursor_x += scaled.kern(prev, glyph_id);
            }

            if let Some(info) = self.glyphs.get(&ch) {
                if info.width > 0.0
                    && info.height > 0.0
                    && let Some((uv_min, uv_max)) = self.atlas.uv_rect(&info.atlas_name)
                {
                    let gx = cursor_x + info.left_side_bearing;
                    let gy = baseline_y + info.ascent_offset;
                    draw_list.rect(gx, gy, info.width, info.height, uv_min, uv_max, color);
                }
                cursor_x += info.advance_width;
            }

            prev_glyph_id = Some(glyph_id);
        }
    }

    pub fn text_width(&self, text: &str) -> f32 {
        let scaled = self.font.as_scaled(self.px_size);
        let mut width = 0.0f32;
        let mut prev_glyph_id = None;

        for ch in text.chars() {
            let glyph_id = self.font.glyph_id(ch);

            if let Some(prev) = prev_glyph_id {
                width += scaled.kern(prev, glyph_id);
            }

            if let Some(info) = self.glyphs.get(&ch) {
                width += info.advance_width;
            }

            prev_glyph_id = Some(glyph_id);
        }

        width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT_DATA: &[u8] = include_bytes!("../resources/Minecraft.otf");

    #[test]
    fn single_char_font_has_region() {
        let font = TextureAtlasFont::new(FONT_DATA, 32.0, "A").unwrap();
        assert!(font.atlas().region("A").is_some());
    }

    #[test]
    fn line_height_and_ascent_are_positive() {
        let font = TextureAtlasFont::new(FONT_DATA, 32.0, "A").unwrap();
        assert!(font.line_height > 0.0);
        assert!(font.ascent > 0.0);
    }

    #[test]
    fn draw_text_produces_geometry() {
        let font = TextureAtlasFont::new(FONT_DATA, 32.0, "AB").unwrap();
        let mut dl = DrawList::new();
        font.draw_text(&mut dl, "AB", 0.0, 0.0, Rgba::WHITE);
        assert!(dl.vertex_data().len() > 0);
    }

    #[test]
    fn text_width_empty_is_zero() {
        let font = TextureAtlasFont::new(FONT_DATA, 32.0, "A").unwrap();
        assert_eq!(font.text_width(""), 0.0);
    }

    #[test]
    fn text_width_single_char_equals_advance() {
        let font = TextureAtlasFont::new(FONT_DATA, 32.0, "A").unwrap();
        let advance = font.glyphs.get(&'A').unwrap().advance_width;
        assert_eq!(font.text_width("A"), advance);
    }
}

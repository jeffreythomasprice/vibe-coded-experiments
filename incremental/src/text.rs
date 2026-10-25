use ab_glyph::{Font, FontRef, GlyphId, PxScale, ScaleFont, VariableFont};

use crate::error::AppError;
use crate::hex::Point;

const FONT_BYTES: &[u8] = include_bytes!("../assets/NotoSerif-VariableFont_wdth,wght.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    Regular,
    Bold,
}

impl Weight {
    fn wght(self) -> f32 {
        match self {
            Weight::Regular => 400.0,
            Weight::Bold => 700.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphQuad {
    pub id: GlyphId,
    pub weight: Weight,
    pub px: f32,
    pub pos: Point,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Text {
    pub glyphs: Vec<GlyphQuad>,
    pub size: Point,
}

pub struct Fonts {
    regular: FontRef<'static>,
    bold: FontRef<'static>,
}

impl Fonts {
    pub fn load() -> Result<Self, AppError> {
        let styled = |weight: Weight| -> Result<FontRef<'static>, AppError> {
            let mut font = FontRef::try_from_slice(FONT_BYTES)?;
            font.set_variation(b"wght", weight.wght());
            Ok(font)
        };
        Ok(Fonts {
            regular: styled(Weight::Regular)?,
            bold: styled(Weight::Bold)?,
        })
    }

    pub fn font(&self, weight: Weight) -> &FontRef<'static> {
        match weight {
            Weight::Regular => &self.regular,
            Weight::Bold => &self.bold,
        }
    }

    pub fn line_height(&self, px: f64, weight: Weight) -> f64 {
        let scaled = self.font(weight).as_scaled(PxScale::from(px as f32));
        f64::from(scaled.ascent() - scaled.descent())
    }

    pub fn measure(&self, text: &str, px: f64, weight: Weight) -> Point {
        let scaled = self.font(weight).as_scaled(PxScale::from(px as f32));
        let mut width = 0.0f32;
        let mut previous: Option<GlyphId> = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = previous {
                width += scaled.kern(p, id);
            }
            width += scaled.h_advance(id);
            previous = Some(id);
        }
        Point::new(f64::from(width), self.line_height(px, weight))
    }

    /// Glyph pen positions for a single line whose top-left corner is `origin`.
    pub fn layout(&self, text: &str, px: f64, weight: Weight, origin: Point) -> Text {
        let scaled = self.font(weight).as_scaled(PxScale::from(px as f32));
        let baseline = origin.y + f64::from(scaled.ascent());
        let mut pen = 0.0f32;
        let mut previous: Option<GlyphId> = None;
        let mut glyphs = Vec::with_capacity(text.len());
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = previous {
                pen += scaled.kern(p, id);
            }
            glyphs.push(GlyphQuad {
                id,
                weight,
                px: px as f32,
                pos: Point::new(origin.x + f64::from(pen), baseline),
            });
            pen += scaled.h_advance(id);
            previous = Some(id);
        }
        Text {
            glyphs,
            size: Point::new(f64::from(pen), self.line_height(px, weight)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Fonts {
        Fonts::load().unwrap()
    }

    #[test]
    fn widths_are_positive() {
        let f = fonts();
        assert!(f.measure("Hex (0, 0)", 13.0, Weight::Regular).x > 0.0);
    }

    #[test]
    fn empty_string_has_no_width() {
        let f = fonts();
        assert_eq!(f.measure("", 13.0, Weight::Regular).x, 0.0);
        assert!(
            f.layout("", 13.0, Weight::Regular, Point::default())
                .glyphs
                .is_empty()
        );
    }

    #[test]
    fn bold_is_wider_than_regular() {
        let f = fonts();
        let regular = f.measure("Resources", 16.0, Weight::Regular).x;
        let bold = f.measure("Resources", 16.0, Weight::Bold).x;
        assert!(bold > regular);
    }

    #[test]
    fn layout_matches_measure() {
        let f = fonts();
        let text = "+12 gathered";
        let laid = f.layout(text, 18.0, Weight::Bold, Point::new(5.0, 7.0));
        assert_eq!(laid.glyphs.len(), text.chars().count());
        assert!((laid.size.x - f.measure(text, 18.0, Weight::Bold).x).abs() < 1e-3);
        assert_eq!(laid.glyphs[0].pos.x, 5.0);
        assert!(laid.glyphs[0].pos.y > 7.0);
    }

    #[test]
    fn size_scales_with_px() {
        let f = fonts();
        let small = f.measure("abc", 10.0, Weight::Regular).x;
        let large = f.measure("abc", 20.0, Weight::Regular).x;
        assert!((large / small - 2.0).abs() < 0.05);
    }
}

use crate::config::theme::Color;
use crate::hex::Point;
use crate::icon::Mesh;
use crate::text::GlyphQuad;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub min: Point,
    pub size: Point,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rect {
            min: Point::new(x, y),
            size: Point::new(w, h),
        }
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min.x
            && p.x < self.min.x + self.size.x
            && p.y >= self.min.y
            && p.y < self.min.y + self.size.y
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectPrim {
    pub rect: Rect,
    pub fill: Color,
    pub border: Color,
    pub border_width: f64,
    pub radius: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct IconPrim<'a> {
    pub mesh: &'a Mesh,
    pub center: Point,
    pub radius: f64,
    pub opacity: f32,
    pub tint: Option<Color>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextPrim {
    pub glyphs: Vec<GlyphQuad>,
    pub color: Color,
    pub outline: Option<(Color, f64)>,
}

#[derive(Debug, Default)]
pub struct Layer<'a> {
    pub rects: Vec<RectPrim>,
    pub icons: Vec<IconPrim<'a>>,
    pub texts: Vec<TextPrim>,
}

use std::fmt;

use glam::Vec2;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl From<Color> for wgpu::Color {
    fn from(c: Color) -> Self {
        Self {
            r: c.r as f64,
            g: c.g as f64,
            b: c.b as f64,
            a: c.a as f64,
        }
    }
}

#[allow(dead_code)]
impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn from_rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    pub const TRANSPARENT: Self = Self::from_rgba8(0, 0, 0, 0);
    pub const ALICE_BLUE: Self = Self::from_rgba8(240, 248, 255, 255);
    pub const ANTIQUE_WHITE: Self = Self::from_rgba8(250, 235, 215, 255);
    pub const AQUA: Self = Self::from_rgba8(0, 255, 255, 255);
    pub const AQUAMARINE: Self = Self::from_rgba8(127, 255, 212, 255);
    pub const AZURE: Self = Self::from_rgba8(240, 255, 255, 255);
    pub const BEIGE: Self = Self::from_rgba8(245, 245, 220, 255);
    pub const BISQUE: Self = Self::from_rgba8(255, 228, 196, 255);
    pub const BLACK: Self = Self::from_rgba8(0, 0, 0, 255);
    pub const BLANCHED_ALMOND: Self = Self::from_rgba8(255, 235, 205, 255);
    pub const BLUE: Self = Self::from_rgba8(0, 0, 255, 255);
    pub const BLUE_VIOLET: Self = Self::from_rgba8(138, 43, 226, 255);
    pub const BROWN: Self = Self::from_rgba8(165, 42, 42, 255);
    pub const BURLY_WOOD: Self = Self::from_rgba8(222, 184, 135, 255);
    pub const CADET_BLUE: Self = Self::from_rgba8(95, 158, 160, 255);
    pub const CHARTREUSE: Self = Self::from_rgba8(127, 255, 0, 255);
    pub const CHOCOLATE: Self = Self::from_rgba8(210, 105, 30, 255);
    pub const CORAL: Self = Self::from_rgba8(255, 127, 80, 255);
    pub const CORNFLOWER_BLUE: Self = Self::from_rgba8(100, 149, 237, 255);
    pub const CORNSILK: Self = Self::from_rgba8(255, 248, 220, 255);
    pub const CRIMSON: Self = Self::from_rgba8(220, 20, 60, 255);
    pub const CYAN: Self = Self::from_rgba8(0, 255, 255, 255);
    pub const DARK_BLUE: Self = Self::from_rgba8(0, 0, 139, 255);
    pub const DARK_CYAN: Self = Self::from_rgba8(0, 139, 139, 255);
    pub const DARK_GOLDENROD: Self = Self::from_rgba8(184, 134, 11, 255);
    pub const DARK_GRAY: Self = Self::from_rgba8(169, 169, 169, 255);
    pub const DARK_GREEN: Self = Self::from_rgba8(0, 100, 0, 255);
    pub const DARK_KHAKI: Self = Self::from_rgba8(189, 183, 107, 255);
    pub const DARK_MAGENTA: Self = Self::from_rgba8(139, 0, 139, 255);
    pub const DARK_OLIVE_GREEN: Self = Self::from_rgba8(85, 107, 47, 255);
    pub const DARK_ORANGE: Self = Self::from_rgba8(255, 140, 0, 255);
    pub const DARK_ORCHID: Self = Self::from_rgba8(153, 50, 204, 255);
    pub const DARK_RED: Self = Self::from_rgba8(139, 0, 0, 255);
    pub const DARK_SALMON: Self = Self::from_rgba8(233, 150, 122, 255);
    pub const DARK_SEA_GREEN: Self = Self::from_rgba8(143, 188, 143, 255);
    pub const DARK_SLATE_BLUE: Self = Self::from_rgba8(72, 61, 139, 255);
    pub const DARK_SLATE_GRAY: Self = Self::from_rgba8(47, 79, 79, 255);
    pub const DARK_TURQUOISE: Self = Self::from_rgba8(0, 206, 209, 255);
    pub const DARK_VIOLET: Self = Self::from_rgba8(148, 0, 211, 255);
    pub const DEEP_PINK: Self = Self::from_rgba8(255, 20, 147, 255);
    pub const DEEP_SKY_BLUE: Self = Self::from_rgba8(0, 191, 255, 255);
    pub const DIM_GRAY: Self = Self::from_rgba8(105, 105, 105, 255);
    pub const DODGER_BLUE: Self = Self::from_rgba8(30, 144, 255, 255);
    pub const FIREBRICK: Self = Self::from_rgba8(178, 34, 34, 255);
    pub const FLORAL_WHITE: Self = Self::from_rgba8(255, 250, 240, 255);
    pub const FOREST_GREEN: Self = Self::from_rgba8(34, 139, 34, 255);
    pub const FUCHSIA: Self = Self::from_rgba8(255, 0, 255, 255);
    pub const GAINSBORO: Self = Self::from_rgba8(220, 220, 220, 255);
    pub const GHOST_WHITE: Self = Self::from_rgba8(248, 248, 255, 255);
    pub const GOLD: Self = Self::from_rgba8(255, 215, 0, 255);
    pub const GOLDENROD: Self = Self::from_rgba8(218, 165, 32, 255);
    pub const GRAY: Self = Self::from_rgba8(128, 128, 128, 255);
    pub const GREEN: Self = Self::from_rgba8(0, 128, 0, 255);
    pub const GREEN_YELLOW: Self = Self::from_rgba8(173, 255, 47, 255);
    pub const HONEYDEW: Self = Self::from_rgba8(240, 255, 240, 255);
    pub const HOT_PINK: Self = Self::from_rgba8(255, 105, 180, 255);
    pub const INDIAN_RED: Self = Self::from_rgba8(205, 92, 92, 255);
    pub const INDIGO: Self = Self::from_rgba8(75, 0, 130, 255);
    pub const IVORY: Self = Self::from_rgba8(255, 255, 240, 255);
    pub const KHAKI: Self = Self::from_rgba8(240, 230, 140, 255);
    pub const LAVENDER: Self = Self::from_rgba8(230, 230, 250, 255);
    pub const LAVENDER_BLUSH: Self = Self::from_rgba8(255, 240, 245, 255);
    pub const LAWN_GREEN: Self = Self::from_rgba8(124, 252, 0, 255);
    pub const LEMON_CHIFFON: Self = Self::from_rgba8(255, 250, 205, 255);
    pub const LIGHT_BLUE: Self = Self::from_rgba8(173, 216, 230, 255);
    pub const LIGHT_CORAL: Self = Self::from_rgba8(240, 128, 128, 255);
    pub const LIGHT_CYAN: Self = Self::from_rgba8(224, 255, 255, 255);
    pub const LIGHT_GOLDENROD_YELLOW: Self = Self::from_rgba8(250, 250, 210, 255);
    pub const LIGHT_GRAY: Self = Self::from_rgba8(211, 211, 211, 255);
    pub const LIGHT_GREEN: Self = Self::from_rgba8(144, 238, 144, 255);
    pub const LIGHT_PINK: Self = Self::from_rgba8(255, 182, 193, 255);
    pub const LIGHT_SALMON: Self = Self::from_rgba8(255, 160, 122, 255);
    pub const LIGHT_SEA_GREEN: Self = Self::from_rgba8(32, 178, 170, 255);
    pub const LIGHT_SKY_BLUE: Self = Self::from_rgba8(135, 206, 250, 255);
    pub const LIGHT_SLATE_GRAY: Self = Self::from_rgba8(119, 136, 153, 255);
    pub const LIGHT_STEEL_BLUE: Self = Self::from_rgba8(176, 196, 222, 255);
    pub const LIGHT_YELLOW: Self = Self::from_rgba8(255, 255, 224, 255);
    pub const LIME: Self = Self::from_rgba8(0, 255, 0, 255);
    pub const LIME_GREEN: Self = Self::from_rgba8(50, 205, 50, 255);
    pub const LINEN: Self = Self::from_rgba8(250, 240, 230, 255);
    pub const MAGENTA: Self = Self::from_rgba8(255, 0, 255, 255);
    pub const MAROON: Self = Self::from_rgba8(128, 0, 0, 255);
    pub const MEDIUM_AQUAMARINE: Self = Self::from_rgba8(102, 205, 170, 255);
    pub const MEDIUM_BLUE: Self = Self::from_rgba8(0, 0, 205, 255);
    pub const MEDIUM_ORCHID: Self = Self::from_rgba8(186, 85, 211, 255);
    pub const MEDIUM_PURPLE: Self = Self::from_rgba8(147, 112, 219, 255);
    pub const MEDIUM_SEA_GREEN: Self = Self::from_rgba8(60, 179, 113, 255);
    pub const MEDIUM_SLATE_BLUE: Self = Self::from_rgba8(123, 104, 238, 255);
    pub const MEDIUM_SPRING_GREEN: Self = Self::from_rgba8(0, 250, 154, 255);
    pub const MEDIUM_TURQUOISE: Self = Self::from_rgba8(72, 209, 204, 255);
    pub const MEDIUM_VIOLET_RED: Self = Self::from_rgba8(199, 21, 133, 255);
    pub const MIDNIGHT_BLUE: Self = Self::from_rgba8(25, 25, 112, 255);
    pub const MINT_CREAM: Self = Self::from_rgba8(245, 255, 250, 255);
    pub const MISTY_ROSE: Self = Self::from_rgba8(255, 228, 225, 255);
    pub const MOCCASIN: Self = Self::from_rgba8(255, 228, 181, 255);
    pub const NAVAJO_WHITE: Self = Self::from_rgba8(255, 222, 173, 255);
    pub const NAVY: Self = Self::from_rgba8(0, 0, 128, 255);
    pub const OLD_LACE: Self = Self::from_rgba8(253, 245, 230, 255);
    pub const OLIVE: Self = Self::from_rgba8(128, 128, 0, 255);
    pub const OLIVE_DRAB: Self = Self::from_rgba8(107, 142, 35, 255);
    pub const ORANGE: Self = Self::from_rgba8(255, 165, 0, 255);
    pub const ORANGE_RED: Self = Self::from_rgba8(255, 69, 0, 255);
    pub const ORCHID: Self = Self::from_rgba8(218, 112, 214, 255);
    pub const PALE_GOLDENROD: Self = Self::from_rgba8(238, 232, 170, 255);
    pub const PALE_GREEN: Self = Self::from_rgba8(152, 251, 152, 255);
    pub const PALE_TURQUOISE: Self = Self::from_rgba8(175, 238, 238, 255);
    pub const PALE_VIOLET_RED: Self = Self::from_rgba8(219, 112, 147, 255);
    pub const PAPAYA_WHIP: Self = Self::from_rgba8(255, 239, 213, 255);
    pub const PEACH_PUFF: Self = Self::from_rgba8(255, 218, 185, 255);
    pub const PERU: Self = Self::from_rgba8(205, 133, 63, 255);
    pub const PINK: Self = Self::from_rgba8(255, 192, 203, 255);
    pub const PLUM: Self = Self::from_rgba8(221, 160, 221, 255);
    pub const POWDER_BLUE: Self = Self::from_rgba8(176, 224, 230, 255);
    pub const PURPLE: Self = Self::from_rgba8(128, 0, 128, 255);
    pub const REBECCA_PURPLE: Self = Self::from_rgba8(102, 51, 153, 255);
    pub const RED: Self = Self::from_rgba8(255, 0, 0, 255);
    pub const ROSY_BROWN: Self = Self::from_rgba8(188, 143, 143, 255);
    pub const ROYAL_BLUE: Self = Self::from_rgba8(65, 105, 225, 255);
    pub const SADDLE_BROWN: Self = Self::from_rgba8(139, 69, 19, 255);
    pub const SALMON: Self = Self::from_rgba8(250, 128, 114, 255);
    pub const SANDY_BROWN: Self = Self::from_rgba8(244, 164, 96, 255);
    pub const SEA_GREEN: Self = Self::from_rgba8(46, 139, 87, 255);
    pub const SEA_SHELL: Self = Self::from_rgba8(255, 245, 238, 255);
    pub const SIENNA: Self = Self::from_rgba8(160, 82, 45, 255);
    pub const SILVER: Self = Self::from_rgba8(192, 192, 192, 255);
    pub const SKY_BLUE: Self = Self::from_rgba8(135, 206, 235, 255);
    pub const SLATE_BLUE: Self = Self::from_rgba8(106, 90, 205, 255);
    pub const SLATE_GRAY: Self = Self::from_rgba8(112, 128, 144, 255);
    pub const SNOW: Self = Self::from_rgba8(255, 250, 250, 255);
    pub const SPRING_GREEN: Self = Self::from_rgba8(0, 255, 127, 255);
    pub const STEEL_BLUE: Self = Self::from_rgba8(70, 130, 180, 255);
    pub const TAN: Self = Self::from_rgba8(210, 180, 140, 255);
    pub const TEAL: Self = Self::from_rgba8(0, 128, 128, 255);
    pub const THISTLE: Self = Self::from_rgba8(216, 191, 216, 255);
    pub const TOMATO: Self = Self::from_rgba8(255, 99, 71, 255);
    pub const TURQUOISE: Self = Self::from_rgba8(64, 224, 208, 255);
    pub const VIOLET: Self = Self::from_rgba8(238, 130, 238, 255);
    pub const WHEAT: Self = Self::from_rgba8(245, 222, 179, 255);
    pub const WHITE: Self = Self::from_rgba8(255, 255, 255, 255);
    pub const WHITE_SMOKE: Self = Self::from_rgba8(245, 245, 245, 255);
    pub const YELLOW: Self = Self::from_rgba8(255, 255, 0, 255);
    pub const YELLOW_GREEN: Self = Self::from_rgba8(154, 205, 50, 255);

    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h = (h.fract() + 1.0).fract() * 6.0;
        let c = v * s;
        let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
        let m = v - c;
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Self::rgb(r + m, g + m, b + m)
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = (self.r.clamp(0.0, 1.0) * 255.0) as u8;
        let g = (self.g.clamp(0.0, 1.0) * 255.0) as u8;
        let b = (self.b.clamp(0.0, 1.0) * 255.0) as u8;
        write!(f, "#{:02X}{:02X}{:02X}", r, g, b)
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorVertex2D {
    pub position: Vec2,
    pub color: Color,
}

impl ColorVertex2D {
    pub fn new(position: Vec2, color: Color) -> Self {
        Self { position, color }
    }

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<ColorVertex2D>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TextureVertex2D {
    pub position: Vec2,
    pub texcoord: Vec2,
    pub color: Color,
}

impl TextureVertex2D {
    pub fn new(position: Vec2, texcoord: Vec2, color: Color) -> Self {
        Self {
            position,
            texcoord,
            color,
        }
    }

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<TextureVertex2D>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 8,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 16,
                    shader_location: 2,
                },
            ],
        }
    }
}

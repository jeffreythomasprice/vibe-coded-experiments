use crate::resource::{RESOURCE_COUNT, Resource};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Color = Color::hex(0x000000);

    pub const fn hex(rgb: u32) -> Self {
        Color {
            r: ((rgb >> 16) & 0xff) as f32 / 255.0,
            g: ((rgb >> 8) & 0xff) as f32 / 255.0,
            b: (rgb & 0xff) as f32 / 255.0,
            a: 1.0,
        }
    }

    pub const fn with_alpha(self, a: f32) -> Self {
        Color { a, ..self }
    }

    pub fn scale_alpha(self, k: f32) -> Self {
        Color {
            a: self.a * k,
            ..self
        }
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

pub struct ToastTheme {
    pub colors: [Color; RESOURCE_COUNT],
    pub outlines: [Color; RESOURCE_COUNT],
    pub outline_width: f64,
    pub font_size: f64,
    pub duration: f64,
    pub fade: f64,
    pub rise: f64,
    pub gap: f64,
    pub icon_scale: f64,
}

impl ToastTheme {
    pub fn color(&self, r: Resource) -> Color {
        self.colors[r.index()]
    }

    pub fn outline(&self, r: Resource) -> Color {
        self.outlines[r.index()]
    }
}

pub struct PanelTheme {
    pub padding_x: f64,
    pub padding_y: f64,
    pub border_width: f64,
    pub font_size: f64,
    pub line_height: f64,
    pub tooltip_title_size: f64,
    pub sidebar_title_size: f64,
    pub sidebar_width: f64,
    pub tooltip_max_width: f64,
    pub tooltip_icon: f64,
    pub sidebar_icon: f64,
    pub bar_height: f64,
    pub bar_min_width: f64,
    pub bar_radius: f64,
    pub bar_font_size: f64,
    pub row_gap: f64,
    pub title_gap: f64,
    pub sidebar_title_gap: f64,
    pub section_gap: f64,
    pub label_opacity: f32,
}

pub struct Theme {
    pub background: Color,
    pub empty_fill: Color,
    pub owned_fill: Color,
    pub selected_fill: Color,
    pub empty_border: Color,
    pub owned_border: Color,
    pub selected_border: Color,
    pub panel_background: Color,
    pub panel_border: Color,
    pub panel_text: Color,
    pub bar_background: Color,
    pub bar_generate: Color,
    pub bar_overfill: Color,
    pub bar_text_shadow: Color,
    pub border_width: f64,
    pub selected_border_width: f64,
    pub panel: PanelTheme,
    pub toast: ToastTheme,
}

pub const DARK: Theme = Theme {
    background: Color::hex(0x101018),
    empty_fill: Color::hex(0x16161f),
    owned_fill: Color::hex(0x2f4a6b),
    selected_fill: Color::hex(0x4a6f9c),
    empty_border: Color::hex(0x23232f),
    owned_border: Color::hex(0x8fb4e0),
    selected_border: Color::hex(0xffd866),
    panel_background: Color::hex(0x181824),
    panel_border: Color::hex(0x8fb4e0),
    panel_text: Color::hex(0xd8dde6),
    bar_background: Color::hex(0x23232f),
    bar_generate: Color::hex(0x4a6f9c),
    bar_overfill: Color::hex(0xc9a227),
    bar_text_shadow: Color::BLACK,
    border_width: 1.0,
    selected_border_width: 3.0,
    panel: PanelTheme {
        padding_x: 12.0,
        padding_y: 10.0,
        border_width: 1.0,
        font_size: 24.0,
        line_height: 1.4,
        tooltip_title_size: 16.25,
        sidebar_title_size: 20.0,
        sidebar_width: 280.0,
        tooltip_max_width: 240.0,
        tooltip_icon: 16.0,
        sidebar_icon: 32.0,
        bar_height: 20.0,
        bar_min_width: 140.0,
        bar_radius: 2.0,
        bar_font_size: 20.0,
        row_gap: 6.0,
        title_gap: 4.0,
        sidebar_title_gap: 8.0,
        section_gap: 14.0,
        label_opacity: 0.7,
    },
    toast: ToastTheme {
        colors: [
            Color::hex(0x7ed46a),
            Color::hex(0xff6e40),
            Color::hex(0xd9ad5b),
            Color::hex(0xd5dde3),
            Color::hex(0x4fc3f7),
        ],
        outlines: [
            Color::hex(0x0e2a0a),
            Color::hex(0x3a0d05),
            Color::hex(0x2e1f08),
            Color::hex(0x1c2328),
            Color::hex(0x062638),
        ],
        outline_width: 3.0,
        font_size: 28.0,
        duration: 1.0,
        fade: 0.6,
        rise: 24.0,
        gap: 4.0,
        icon_scale: 1.2,
    },
};

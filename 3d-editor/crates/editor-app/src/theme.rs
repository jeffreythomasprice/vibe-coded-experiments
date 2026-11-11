use egui::{Color32, Context, CornerRadius, FontId, Stroke, TextStyle, Visuals};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    StockDark,
    Dark,
    Light,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Dark, Theme::Light, Theme::StockDark];

    pub fn name(self) -> &'static str {
        match self {
            Theme::StockDark => "egui stock dark",
            Theme::Dark => "Editor dark",
            Theme::Light => "Editor light",
        }
    }
}

struct Palette {
    bg: Color32,
    panel: Color32,
    raised: Color32,
    inset: Color32,
    border: Color32,
    text: Color32,
    weak: Color32,
    accent: Color32,
    hover: Color32,
    active: Color32,
}

const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x1a, 0x1b, 0x1e),
    panel: Color32::from_rgb(0x23, 0x25, 0x29),
    raised: Color32::from_rgb(0x2d, 0x30, 0x35),
    inset: Color32::from_rgb(0x17, 0x18, 0x1b),
    border: Color32::from_rgb(0x38, 0x3b, 0x41),
    text: Color32::from_rgb(0xd5, 0xd8, 0xdd),
    weak: Color32::from_rgb(0x8a, 0x8f, 0x98),
    accent: Color32::from_rgb(0x4f, 0x9d, 0xff),
    hover: Color32::from_rgb(0x38, 0x3c, 0x43),
    active: Color32::from_rgb(0x42, 0x47, 0x50),
};

const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xd9, 0xdb, 0xdf),
    panel: Color32::from_rgb(0xea, 0xeb, 0xee),
    raised: Color32::from_rgb(0xf6, 0xf7, 0xf8),
    inset: Color32::from_rgb(0xff, 0xff, 0xff),
    border: Color32::from_rgb(0xbc, 0xc0, 0xc7),
    text: Color32::from_rgb(0x20, 0x23, 0x28),
    weak: Color32::from_rgb(0x6a, 0x70, 0x79),
    accent: Color32::from_rgb(0x1f, 0x6f, 0xe0),
    hover: Color32::from_rgb(0xdd, 0xe0, 0xe5),
    active: Color32::from_rgb(0xcb, 0xd0, 0xd8),
};

fn visuals(base: Visuals, p: &Palette) -> Visuals {
    let mut v = base;
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.inset;
    v.faint_bg_color = p.raised;
    v.code_bg_color = p.inset;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(5);
    v.override_text_color = Some(p.text);
    v.selection.bg_fill = p.accent.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;
    let radius = CornerRadius::same(3);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.weak);
    w.inactive.bg_fill = p.raised;
    w.inactive.weak_bg_fill = p.raised;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.7));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.active.bg_fill = p.active;
    w.active.weak_bg_fill = p.active;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.open.bg_fill = p.active;
    w.open.weak_bg_fill = p.active;
    for s in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        s.corner_radius = radius;
    }
    v
}

pub fn apply(ctx: &Context, theme: Theme) {
    let v = match theme {
        Theme::StockDark => Visuals::dark(),
        Theme::Dark => visuals(Visuals::dark(), &DARK),
        Theme::Light => visuals(Visuals::light(), &LIGHT),
    };
    ctx.set_theme(match theme {
        Theme::Light => egui::Theme::Light,
        _ => egui::Theme::Dark,
    });
    ctx.global_style_mut(|style| {
        *style = egui::Style {
            visuals: v,
            ..egui::Style::default()
        };
        if theme == Theme::StockDark {
            return;
        }
        style.spacing.item_spacing = egui::vec2(6.0, 5.0);
        style.spacing.button_padding = egui::vec2(8.0, 3.0);
        style.spacing.interact_size.y = 22.0;
        style.spacing.window_margin = egui::Margin::same(8);
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(13.5));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(13.5));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(11.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(16.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(12.5));
    });
}

pub fn viewport_bg(theme: Theme) -> Color32 {
    match theme {
        Theme::Light => LIGHT.bg,
        _ => DARK.bg,
    }
}

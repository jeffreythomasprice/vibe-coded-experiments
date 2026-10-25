use crate::config::game::TOOLTIP_OFFSET;
use crate::config::theme::{Color, PanelTheme, Theme};
use crate::hex::Point;
use crate::info::{Details, Summary};
use crate::prim::{IconPrim, Layer, Rect, RectPrim, TextPrim};
use crate::resource::Resource;
use crate::text::{Fonts, Weight};
use crate::toast::{self, Toasts};

pub struct UiInput<'a> {
    pub hover: Option<(&'a Summary, Point)>,
    pub details: Option<&'a Details>,
    pub toasts: &'a Toasts,
    pub sidebar_scroll: f64,
}

pub struct UiFrame<'a> {
    pub layers: Vec<Layer<'a>>,
    pub sidebar: Option<Rect>,
    pub icon_hits: Vec<(Rect, Resource)>,
    pub max_scroll: f64,
}

struct Ctx<'f> {
    fonts: &'f Fonts,
    theme: &'f Theme,
}

impl Ctx<'_> {
    fn text(
        &self,
        s: &str,
        px: f64,
        weight: Weight,
        origin: Point,
        color: Color,
    ) -> (TextPrim, Point) {
        let laid = self.fonts.layout(s, px, weight, origin);
        (
            TextPrim {
                glyphs: laid.glyphs,
                color,
                outline: None,
            },
            laid.size,
        )
    }

    fn line_box(&self, px: f64) -> f64 {
        px * self.theme.panel.line_height
    }

    fn centered_in_line(&self, px: f64, weight: Weight, top: f64) -> f64 {
        top + (self.line_box(px) - self.fonts.line_height(px, weight)) / 2.0
    }
}

pub fn build<'a>(
    fonts: &Fonts,
    theme: &Theme,
    viewport: Point,
    input: &UiInput<'a>,
) -> UiFrame<'a> {
    let ctx = Ctx { fonts, theme };
    let mut frame = UiFrame {
        layers: Vec::new(),
        sidebar: None,
        icon_hits: Vec::new(),
        max_scroll: 0.0,
    };
    if let Some(details) = input.details {
        sidebar(&ctx, viewport, details, input.sidebar_scroll, &mut frame);
    }
    if let Some((summary, cursor)) = input.hover {
        frame.layers.push(tooltip(&ctx, viewport, summary, cursor));
    }
    frame.layers.push(toasts(&ctx, input.toasts));
    frame
}

fn panel_rect(rect: Rect, theme: &Theme) -> RectPrim {
    RectPrim {
        rect,
        fill: theme.panel_background,
        border: theme.panel_border,
        border_width: theme.panel.border_width,
        radius: 0.0,
    }
}

fn sidebar<'a>(
    ctx: &Ctx,
    viewport: Point,
    details: &'a Details,
    scroll: f64,
    frame: &mut UiFrame<'a>,
) {
    let p = &ctx.theme.panel;
    let bw = p.border_width;
    let width = p.sidebar_width;
    let left = viewport.x - width;
    let inner_left = left + bw + p.padding_x;
    let inner_right = viewport.x - p.padding_x;
    let mut layer = Layer::default();
    let panel = Rect::new(left, -bw, width + bw, viewport.y + 2.0 * bw);
    layer.rects.push(panel_rect(panel, ctx.theme));

    let text_color = ctx.theme.panel_text;
    let mut y = p.padding_y;
    let title_top = ctx.centered_in_line(p.sidebar_title_size, Weight::Bold, y);
    let mut items = vec![Item::Text {
        text: &details.title,
        px: p.sidebar_title_size,
        weight: Weight::Bold,
        x: inner_left,
        top: title_top,
        color: text_color,
        align_right_to: None,
    }];
    y += ctx.line_box(p.sidebar_title_size) + p.sidebar_title_gap;

    let mut hits = Vec::new();
    for resource in &details.resources {
        items.push(Item::Icon {
            icon: &resource.icon.mesh,
            rect: Rect::new(inner_left, y, p.sidebar_icon, p.sidebar_icon),
        });
        hits.push((
            Rect::new(inner_left, y, p.sidebar_icon, p.sidebar_icon),
            resource.resource,
        ));
        y += p.sidebar_icon + 4.0;
        for (label, value) in &resource.lines {
            let top = ctx.centered_in_line(p.font_size, Weight::Regular, y);
            items.push(Item::Text {
                text: label,
                px: p.font_size,
                weight: Weight::Regular,
                x: inner_left,
                top,
                color: text_color.scale_alpha(p.label_opacity),
                align_right_to: None,
            });
            items.push(Item::Text {
                text: value,
                px: p.font_size,
                weight: Weight::Regular,
                x: inner_right,
                top,
                color: text_color,
                align_right_to: Some(inner_right),
            });
            y += ctx.line_box(p.font_size) + 2.0;
        }
        y += p.section_gap;
    }

    let max_scroll = (y + p.padding_y - viewport.y).max(0.0);
    let scroll = scroll.clamp(0.0, max_scroll);
    let shift = Point::new(0.0, -scroll);

    for item in items {
        match item {
            Item::Icon { icon, rect } => {
                let rect = shifted(rect, shift);
                layer.icons.push(IconPrim {
                    mesh: icon,
                    center: Point::new(
                        rect.min.x + rect.size.x / 2.0,
                        rect.min.y + rect.size.y / 2.0,
                    ),
                    radius: rect.size.x / 2.0,
                    opacity: 1.0,
                    tint: None,
                });
            }
            Item::Text {
                text,
                px,
                weight,
                x,
                top,
                color,
                align_right_to,
            } => {
                let x = match align_right_to {
                    Some(right) => right - ctx.fonts.measure(text, px, weight).x,
                    None => x,
                };
                let origin = Point::new(x, top - scroll);
                layer
                    .texts
                    .push(ctx.text(text, px, weight, origin, color).0);
            }
        }
    }

    frame.icon_hits = hits
        .into_iter()
        .map(|(rect, r)| (shifted(rect, shift), r))
        .collect();
    frame.sidebar = Some(panel);
    frame.max_scroll = max_scroll;
    frame.layers.push(layer);
}

enum Item<'a> {
    Icon {
        icon: &'a crate::icon::Mesh,
        rect: Rect,
    },
    Text {
        text: &'a str,
        px: f64,
        weight: Weight,
        x: f64,
        top: f64,
        color: Color,
        align_right_to: Option<f64>,
    },
}

fn shifted(rect: Rect, by: Point) -> Rect {
    Rect {
        min: rect.min + by,
        size: rect.size,
    }
}

fn tooltip<'a>(ctx: &Ctx, viewport: Point, summary: &'a Summary, cursor: Point) -> Layer<'a> {
    let p: &PanelTheme = &ctx.theme.panel;
    let bw = p.border_width;
    let title_w = ctx
        .fonts
        .measure(&summary.title, p.tooltip_title_size, Weight::Bold)
        .x;
    let row_extra = p.tooltip_icon + p.row_gap;
    let bar_text_widths = summary.rows.iter().map(|r| {
        ctx.fonts
            .measure(&r.text, p.bar_font_size, Weight::Regular)
            .x
    });
    let bar_w_natural = bar_text_widths.fold(p.bar_min_width, f64::max);
    let content_w = title_w
        .max(row_extra + bar_w_natural)
        .min(p.tooltip_max_width);
    let bar_w = (content_w - row_extra).max(0.0);
    let title_h = ctx.line_box(p.tooltip_title_size);
    let content_h = title_h + p.title_gap + p.bar_height * summary.rows.len() as f64;
    let size = Point::new(
        content_w + 2.0 * (p.padding_x + bw),
        content_h + 2.0 * (p.padding_y + bw),
    );
    let at = tooltip_position(cursor, size, viewport);

    let mut layer = Layer::default();
    layer
        .rects
        .push(panel_rect(Rect::new(at.x, at.y, size.x, size.y), ctx.theme));
    let left = at.x + bw + p.padding_x;
    let mut y = at.y + bw + p.padding_y;
    let title_top = ctx.centered_in_line(p.tooltip_title_size, Weight::Bold, y);
    let (title, _) = ctx.text(
        &summary.title,
        p.tooltip_title_size,
        Weight::Bold,
        Point::new(left, title_top),
        ctx.theme.panel_text,
    );
    layer.texts.push(title);
    y += title_h + p.title_gap;

    for row in &summary.rows {
        layer.icons.push(IconPrim {
            mesh: &row.icon.mesh,
            center: Point::new(left + p.tooltip_icon / 2.0, y + p.bar_height / 2.0),
            radius: p.tooltip_icon / 2.0,
            opacity: 1.0,
            tint: None,
        });
        let bar_x = left + row_extra;
        let bar = |width: f64, fill: Color| RectPrim {
            rect: Rect::new(bar_x, y, width, p.bar_height),
            fill,
            border: fill.with_alpha(0.0),
            border_width: 0.0,
            radius: p.bar_radius,
        };
        layer.rects.push(bar(bar_w, ctx.theme.bar_background));
        for (fraction, color) in [
            (row.generate_fill, ctx.theme.bar_generate),
            (row.overfill_fill, ctx.theme.bar_overfill),
        ] {
            if fraction > 0.0 {
                layer.rects.push(RectPrim {
                    radius: 0.0,
                    ..bar(bar_w * fraction.clamp(0.0, 1.0), color)
                });
            }
        }
        let text_w = ctx
            .fonts
            .measure(&row.text, p.bar_font_size, Weight::Regular);
        let origin = Point::new(
            bar_x + (bar_w - text_w.x) / 2.0,
            y + (p.bar_height - text_w.y) / 2.0,
        );
        let (mut text, _) = ctx.text(
            &row.text,
            p.bar_font_size,
            Weight::Regular,
            origin,
            ctx.theme.panel_text,
        );
        text.outline = Some((ctx.theme.bar_text_shadow, 1.0));
        layer.texts.push(text);
        y += p.bar_height;
    }
    layer
}

fn toasts<'a>(ctx: &Ctx, toasts: &'a Toasts) -> Layer<'a> {
    let t = &ctx.theme.toast;
    let mut layer = Layer::default();
    for toast in toasts.iter() {
        let alpha = toast::opacity(toast.age, t.duration, t.fade) as f32;
        if alpha <= 0.0 {
            continue;
        }
        let rise = toast::offset(toast.age, t.duration, t.rise);
        let label = toast.label();
        let text_size = ctx.fonts.measure(&label, t.font_size, Weight::Bold);
        let icon_size = t.font_size * t.icon_scale;
        let width = icon_size + t.gap + text_size.x;
        let height = text_size.y.max(icon_size);
        let left = toast.at.x - width / 2.0;
        let top = toast.at.y - rise - height;
        let icon_center = Point::new(left + icon_size / 2.0, top + height / 2.0);
        for (scale, tint, opacity) in [(1.1, Some(Color::BLACK), 0.8 * alpha), (1.0, None, alpha)] {
            layer.icons.push(IconPrim {
                mesh: &toast.icon.mesh,
                center: icon_center,
                radius: icon_size / 2.0 * scale,
                opacity,
                tint,
            });
        }
        let origin = Point::new(left + icon_size + t.gap, top + (height - text_size.y) / 2.0);
        let (mut text, _) = ctx.text(
            &label,
            t.font_size,
            Weight::Bold,
            origin,
            t.color(toast.resource).scale_alpha(alpha),
        );
        text.outline = Some((
            t.outline(toast.resource).scale_alpha(alpha),
            t.outline_width / 2.0,
        ));
        layer.texts.push(text);
    }
    layer
}

pub fn tooltip_position(cursor: Point, size: Point, viewport: Point) -> Point {
    let place = |c: f64, s: f64, v: f64| {
        let after = c + TOOLTIP_OFFSET;
        if after + s <= v {
            after
        } else {
            (c - TOOLTIP_OFFSET - s).max(0.0)
        }
    };
    Point::new(
        place(cursor.x, size.x, viewport.x),
        place(cursor.y, size.y, viewport.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::theme::DARK;
    use crate::game::Game;
    use crate::hex::Axial;
    use crate::icon::Icon;
    use crate::info;

    const VIEWPORT: Point = Point::new(800.0, 600.0);
    const SIZE: Point = Point::new(100.0, 50.0);

    fn fonts() -> Fonts {
        Fonts::load().unwrap()
    }

    #[test]
    fn offsets_from_cursor() {
        let p = tooltip_position(Point::new(100.0, 100.0), SIZE, VIEWPORT);
        assert_eq!(p, Point::new(114.0, 114.0));
    }

    #[test]
    fn flips_at_right_edge() {
        let p = tooltip_position(Point::new(750.0, 100.0), SIZE, VIEWPORT);
        assert_eq!(p, Point::new(636.0, 114.0));
    }

    #[test]
    fn flips_at_bottom_edge() {
        let p = tooltip_position(Point::new(100.0, 580.0), SIZE, VIEWPORT);
        assert_eq!(p, Point::new(114.0, 516.0));
    }

    #[test]
    fn sidebar_has_one_hit_per_resource() {
        let game = Game::new(3);
        let details = info::details(&game, Axial::ORIGIN).unwrap();
        let toasts = Toasts::default();
        let frame = build(
            &fonts(),
            &DARK,
            VIEWPORT,
            &UiInput {
                hover: None,
                details: Some(&details),
                toasts: &toasts,
                sidebar_scroll: 0.0,
            },
        );
        assert_eq!(frame.icon_hits.len(), details.resources.len());
        let sidebar = frame.sidebar.unwrap();
        assert_eq!(sidebar.min.x + sidebar.size.x, VIEWPORT.x + 1.0);
        for (rect, _) in &frame.icon_hits {
            assert!(sidebar.contains(rect.min));
        }
        assert!(sidebar.contains(Point::new(790.0, 10.0)));
        assert!(!sidebar.contains(Point::new(10.0, 10.0)));
    }

    #[test]
    fn no_sidebar_without_selection() {
        let toasts = Toasts::default();
        let frame = build(
            &fonts(),
            &DARK,
            VIEWPORT,
            &UiInput {
                hover: None,
                details: None,
                toasts: &toasts,
                sidebar_scroll: 0.0,
            },
        );
        assert!(frame.sidebar.is_none());
        assert!(frame.icon_hits.is_empty());
    }

    #[test]
    fn scroll_is_clamped_and_moves_hits() {
        let game = Game::new(3);
        let details = info::details(&game, Axial::ORIGIN).unwrap();
        let toasts = Toasts::default();
        let short = Point::new(800.0, 100.0);
        let make = |scroll| {
            build(
                &fonts(),
                &DARK,
                short,
                &UiInput {
                    hover: None,
                    details: Some(&details),
                    toasts: &toasts,
                    sidebar_scroll: scroll,
                },
            )
        };
        let top = make(0.0);
        assert!(top.max_scroll > 0.0);
        let far = make(1.0e6);
        let shift = top.icon_hits[0].0.min.y - far.icon_hits[0].0.min.y;
        assert!((shift - top.max_scroll).abs() < 1e-9);
        let tall = build(
            &fonts(),
            &DARK,
            Point::new(800.0, 5000.0),
            &UiInput {
                hover: None,
                details: Some(&details),
                toasts: &toasts,
                sidebar_scroll: 50.0,
            },
        );
        assert_eq!(tall.max_scroll, 0.0);
        assert_eq!(
            tall.icon_hits[0].0.min.y,
            DARK.panel.padding_y
                + DARK.panel.sidebar_title_size * DARK.panel.line_height
                + DARK.panel.sidebar_title_gap
        );
    }

    #[test]
    fn tooltip_lists_every_resource() {
        let game = Game::new(3);
        let summary = info::summary(&game, Axial::ORIGIN).unwrap();
        let toasts = Toasts::default();
        let cursor = Point::new(100.0, 100.0);
        let frame = build(
            &fonts(),
            &DARK,
            VIEWPORT,
            &UiInput {
                hover: Some((&summary, cursor)),
                details: None,
                toasts: &toasts,
                sidebar_scroll: 0.0,
            },
        );
        let layer = &frame.layers[0];
        assert_eq!(layer.icons.len(), summary.rows.len());
        assert_eq!(layer.texts.len(), summary.rows.len() + 1);
        let panel = layer.rects[0].rect;
        assert_eq!(panel.min, Point::new(114.0, 114.0));
        assert!(panel.size.x >= DARK.panel.bar_min_width);
    }

    #[test]
    fn toasts_sit_above_the_anchor_and_fade() {
        let at = Point::new(300.0, 200.0);
        let mut toasts = Toasts::default();
        toasts.push(Resource::Water, 4, Icon::generate(Resource::Water, 0), at);
        let build_at = |toasts: &Toasts| {
            let frame = build(
                &fonts(),
                &DARK,
                VIEWPORT,
                &UiInput {
                    hover: None,
                    details: None,
                    toasts,
                    sidebar_scroll: 0.0,
                },
            );
            let layer = frame.layers.into_iter().last().unwrap();
            (
                layer.icons.last().map(|i| (i.center, i.opacity)),
                layer.texts.first().map(|t| t.color.a),
            )
        };
        let (icon, alpha) = build_at(&toasts);
        let (center, opacity) = icon.unwrap();
        assert!(center.y < at.y);
        assert!((center.x - at.x).abs() < 40.0);
        assert_eq!((opacity, alpha), (1.0, Some(1.0)));
        toasts.tick(0.8, DARK.toast.duration);
        let (icon, alpha) = build_at(&toasts);
        let (later, opacity) = icon.unwrap();
        assert!(later.y < center.y);
        assert!(opacity < 1.0 && alpha.unwrap() < 1.0);
        toasts.tick(0.5, DARK.toast.duration);
        assert_eq!(build_at(&toasts), (None, None));
    }
}

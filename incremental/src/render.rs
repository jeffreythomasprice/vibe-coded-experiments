use crate::camera::Camera;
use crate::canvas::Canvas;
use crate::error::AppError;
use crate::game::Game;
use crate::hex::{Axial, DIRECTIONS, FORWARD_DIRS, Layout, Point};
use crate::icon::{Icon, Segment, icon_slots};
use crate::theme::Theme;

const ICON_AREA: f64 = 0.6;

pub fn draw(
    canvas: &Canvas,
    camera: &Camera,
    layout: Layout,
    game: &Game,
    theme: &Theme,
    selected: Option<Axial>,
) -> Result<(), AppError> {
    let ctx = canvas.ctx();
    let dpr = canvas.dpr();
    let viewport = Point::new(canvas.width(), canvas.height());

    ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0)?;
    ctx.set_fill_style_str(theme.background);
    ctx.fill_rect(0.0, 0.0, viewport.x, viewport.y);

    let z = camera.zoom;
    ctx.set_transform(
        dpr * z,
        0.0,
        0.0,
        dpr * z,
        dpr * (viewport.x / 2.0 - camera.center.x * z),
        dpr * (viewport.y / 2.0 - camera.center.y * z),
    )?;
    ctx.set_line_join("round");
    ctx.set_line_cap("round");

    let (min, max) = camera.visible_world_rect(viewport);
    let visible: Vec<Axial> = layout.covering(min, max).collect();
    let owned = |a: Axial| game.is_owned(a);

    ctx.set_fill_style_str(theme.empty_fill);
    ctx.begin_path();
    for &a in visible.iter().filter(|&&a| !owned(a)) {
        hex_path(canvas, layout, a);
    }
    ctx.fill();

    ctx.set_fill_style_str(theme.owned_fill);
    ctx.begin_path();
    for &a in visible.iter().filter(|&&a| owned(a) && Some(a) != selected) {
        hex_path(canvas, layout, a);
    }
    ctx.fill();

    let mut light = Vec::new();
    let mut bright = Vec::new();
    for &a in &visible {
        for dir in FORWARD_DIRS {
            let edges = if owned(a) || owned(a + DIRECTIONS[dir]) {
                &mut bright
            } else {
                &mut light
            };
            edges.push(layout.edge(a, dir));
        }
    }
    ctx.set_line_width(theme.border_width / z);
    stroke_edges(canvas, &light, theme.empty_border);
    stroke_edges(canvas, &bright, theme.owned_border);

    if let Some(a) = selected {
        ctx.set_fill_style_str(theme.selected_fill);
        ctx.begin_path();
        hex_path(canvas, layout, a);
        ctx.fill();
        ctx.set_stroke_style_str(theme.selected_border);
        ctx.set_line_width(theme.selected_border_width / z);
        ctx.stroke();
    }

    for &a in visible.iter().filter(|&&a| owned(a)) {
        draw_icons(canvas, layout, game, a);
    }

    Ok(())
}

fn draw_icons(canvas: &Canvas, layout: Layout, game: &Game, a: Axial) {
    let Some(cell) = game.cell(a) else {
        return;
    };
    let icons: Vec<&Icon> = cell.resources().map(|(_, _, icon)| icon).collect();
    let center = layout.hex_to_pixel(a);
    let area = layout.size * ICON_AREA;
    for (icon, (offset, radius)) in icons.into_iter().zip(icon_slots(cell.resources().count())) {
        let origin = center + offset * area;
        let scale = radius * area;
        draw_icon(canvas, icon, origin, scale);
    }
}

fn draw_icon(canvas: &Canvas, icon: &Icon, origin: Point, scale: f64) {
    let ctx = canvas.ctx();
    let map = |q: Point| origin + q * scale;
    for layer in &icon.layers {
        ctx.set_fill_style_str(layer.fill);
        ctx.begin_path();
        for segment in &layer.segments {
            match *segment {
                Segment::MoveTo(a) => ctx.move_to(map(a).x, map(a).y),
                Segment::LineTo(a) => ctx.line_to(map(a).x, map(a).y),
                Segment::QuadTo(c, a) => {
                    ctx.quadratic_curve_to(map(c).x, map(c).y, map(a).x, map(a).y);
                }
                Segment::CubicTo(c1, c2, a) => ctx.bezier_curve_to(
                    map(c1).x,
                    map(c1).y,
                    map(c2).x,
                    map(c2).y,
                    map(a).x,
                    map(a).y,
                ),
                Segment::Close => ctx.close_path(),
            }
        }
        ctx.fill();
    }
}

fn hex_path(canvas: &Canvas, layout: Layout, a: Axial) {
    let ctx = canvas.ctx();
    let corners = layout.corners(a);
    ctx.move_to(corners[0].x, corners[0].y);
    for c in &corners[1..] {
        ctx.line_to(c.x, c.y);
    }
    ctx.close_path();
}

fn stroke_edges(canvas: &Canvas, edges: &[(Point, Point)], color: &str) {
    let ctx = canvas.ctx();
    ctx.set_stroke_style_str(color);
    ctx.begin_path();
    for (p, q) in edges {
        ctx.move_to(p.x, p.y);
        ctx.line_to(q.x, q.y);
    }
    ctx.stroke();
}

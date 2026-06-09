//! Canvas rendering for a map: grid, standalone shapes, and boolean-operator
//! groups (resolved to exact polygons via the `geo` crate's boolean ops).

use geo::{BooleanOps, BoundingRect, Contains};
use geo::{Coord, LineString, MultiPolygon, Point, Polygon};
use shared::{Geometry, Group, GroupNode, Map, Shape};
use wasm_bindgen::JsValue;
use web_sys::{CanvasRenderingContext2d, CanvasWindingRule, HtmlCanvasElement, Path2d};

use crate::camera::{Camera, GRID_PX};

/// Identifies a selectable item on the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelId {
    Shape(String),
    Group(String),
}

/// An axis-aligned rectangle in grid units: `(x, y, w, h)`.
pub type GridRect = (f64, f64, f64, f64);

const SELECTION_COLOR: &str = "#5ad1ff";

// --- public entry points ----------------------------------------------------

/// Redraw the whole map. Sizes the canvas backing store to the element's CSS
/// size times the device pixel ratio, then draws in CSS-pixel space.
pub fn draw(
    canvas: &HtmlCanvasElement,
    map: &Map,
    cam: &Camera,
    selection: &[SelId],
    preview: Option<GridRect>,
) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let dpr = window.device_pixel_ratio().max(1.0);
    let css_w = canvas.client_width() as f64;
    let css_h = canvas.client_height() as f64;
    if css_w <= 0.0 || css_h <= 0.0 {
        return;
    }

    let pw = (css_w * dpr) as u32;
    let ph = (css_h * dpr) as u32;
    if canvas.width() != pw {
        canvas.set_width(pw);
    }
    if canvas.height() != ph {
        canvas.set_height(ph);
    }

    let ctx = match canvas.get_context("2d") {
        Ok(Some(obj)) => obj.unchecked_into::<CanvasRenderingContext2d>(),
        _ => return,
    };
    let _ = ctx.reset_transform();
    let _ = ctx.scale(dpr, dpr);
    ctx.clear_rect(0.0, 0.0, css_w, css_h);

    draw_background(&ctx, map, cam, css_w, css_h);
    draw_grid(&ctx, map, cam);

    for group in &map.groups {
        draw_group(&ctx, group, cam);
    }
    for shape in &map.shapes {
        draw_shape(&ctx, shape, cam);
    }

    draw_selection(&ctx, map, cam, selection);

    if let Some(rect) = preview {
        draw_preview(&ctx, cam, rect);
    }
}

/// Find the topmost selectable item at the given grid-unit point. Standalone
/// shapes are above groups; later items are above earlier ones.
pub fn hit_test(map: &Map, gx: f64, gy: f64) -> Option<SelId> {
    for shape in map.shapes.iter().rev() {
        if point_in_geometry(&shape.geometry, gx, gy) {
            return Some(SelId::Shape(shape.id.clone()));
        }
    }
    for group in map.groups.iter().rev() {
        if resolve_group(&group.root).contains(&Point::new(gx, gy)) {
            return Some(SelId::Group(group.id.clone()));
        }
    }
    None
}

// --- geometry resolution ----------------------------------------------------

/// Resolve a group's boolean tree to a set of polygons, in grid units.
pub fn resolve_group(node: &GroupNode) -> MultiPolygon<f64> {
    match node {
        GroupNode::Leaf { shape } => match shape.geometry {
            Geometry::Rect { x, y, w, h } => MultiPolygon::new(vec![rect_polygon(x, y, w, h)]),
        },
        GroupNode::Op { op, left, right } => {
            let l = resolve_group(left);
            let r = resolve_group(right);
            match op {
                shared::BoolOp::Union => l.union(&r),
                shared::BoolOp::Intersect => l.intersection(&r),
                shared::BoolOp::Subtract => l.difference(&r),
            }
        }
    }
}

fn rect_polygon(x: f64, y: f64, w: f64, h: f64) -> Polygon<f64> {
    Polygon::new(
        LineString::from(vec![
            (x, y),
            (x + w, y),
            (x + w, y + h),
            (x, y + h),
            (x, y),
        ]),
        vec![],
    )
}

fn point_in_geometry(geom: &Geometry, gx: f64, gy: f64) -> bool {
    match *geom {
        Geometry::Rect { x, y, w, h } => gx >= x && gx <= x + w && gy >= y && gy <= y + h,
    }
}

/// Bounding box (grid units) of any selectable item, for the selection outline.
pub fn group_bbox(group: &Group) -> Option<GridRect> {
    let rect = resolve_group(&group.root).bounding_rect()?;
    let min = rect.min();
    let max = rect.max();
    Some((min.x, min.y, max.x - min.x, max.y - min.y))
}

pub fn shape_bbox(shape: &Shape) -> GridRect {
    match shape.geometry {
        Geometry::Rect { x, y, w, h } => (x, y, w, h),
    }
}

// --- drawing helpers --------------------------------------------------------

fn draw_background(
    ctx: &CanvasRenderingContext2d,
    map: &Map,
    cam: &Camera,
    css_w: f64,
    css_h: f64,
) {
    // Neutral void behind everything, then the map's own background rectangle.
    ctx.set_fill_style_str("#0d0d12");
    ctx.fill_rect(0.0, 0.0, css_w, css_h);

    let (sx, sy) = cam.world_to_screen(0.0, 0.0);
    let w = map.width as f64 * GRID_PX * cam.zoom;
    let h = map.height as f64 * GRID_PX * cam.zoom;
    ctx.set_fill_style_str(&map.background_color);
    ctx.fill_rect(sx, sy, w, h);
}

fn draw_grid(ctx: &CanvasRenderingContext2d, map: &Map, cam: &Camera) {
    ctx.set_stroke_style_str(&map.grid_color);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    for col in 0..=map.width {
        let wx = col as f64 * GRID_PX;
        let (sx, sy0) = cam.world_to_screen(wx, 0.0);
        let (_, sy1) = cam.world_to_screen(wx, map.height as f64 * GRID_PX);
        ctx.move_to(sx, sy0);
        ctx.line_to(sx, sy1);
    }
    for row in 0..=map.height {
        let wy = row as f64 * GRID_PX;
        let (sx0, sy) = cam.world_to_screen(0.0, wy);
        let (sx1, _) = cam.world_to_screen(map.width as f64 * GRID_PX, wy);
        ctx.move_to(sx0, sy);
        ctx.line_to(sx1, sy);
    }
    ctx.stroke();
}

fn draw_shape(ctx: &CanvasRenderingContext2d, shape: &Shape, cam: &Camera) {
    match shape.geometry {
        Geometry::Rect { x, y, w, h } => {
            let (sx, sy) = cam.world_to_screen(x * GRID_PX, y * GRID_PX);
            let sw = w * GRID_PX * cam.zoom;
            let sh = h * GRID_PX * cam.zoom;
            ctx.set_fill_style_str(&shape.style.background_color);
            ctx.fill_rect(sx, sy, sw, sh);
            if shape.style.line_width > 0.0 {
                ctx.set_stroke_style_str(&shape.style.line_color);
                ctx.set_line_width(shape.style.line_width * cam.zoom);
                ctx.stroke_rect(sx, sy, sw, sh);
            }
        }
    }
}

fn draw_group(ctx: &CanvasRenderingContext2d, group: &Group, cam: &Camera) {
    let mp = resolve_group(&group.root);
    let Ok(path) = Path2d::new() else {
        return;
    };
    for poly in mp.iter() {
        add_ring(&path, cam, poly.exterior());
        for interior in poly.interiors() {
            add_ring(&path, cam, interior);
        }
    }

    // Exact fill: even-odd makes nested interior rings (subtract holes) holes.
    ctx.set_fill_style_str(&group.style.background_color);
    let _ = ctx.fill_with_path_2d_and_winding(&path, CanvasWindingRule::Evenodd);

    // Exact outline: stroking every ring traces the true boolean boundary,
    // including the edges of any holes.
    if group.style.line_width > 0.0 {
        ctx.set_stroke_style_str(&group.style.line_color);
        ctx.set_line_width(group.style.line_width * cam.zoom);
        ctx.stroke_with_path(&path);
    }
}

fn add_ring(path: &Path2d, cam: &Camera, ring: &LineString<f64>) {
    let mut first = true;
    for Coord { x, y } in ring.coords() {
        let (sx, sy) = cam.world_to_screen(x * GRID_PX, y * GRID_PX);
        if first {
            path.move_to(sx, sy);
            first = false;
        } else {
            path.line_to(sx, sy);
        }
    }
    path.close_path();
}

fn draw_selection(ctx: &CanvasRenderingContext2d, map: &Map, cam: &Camera, selection: &[SelId]) {
    if selection.is_empty() {
        return;
    }
    ctx.set_stroke_style_str(SELECTION_COLOR);
    ctx.set_line_width(2.0);
    set_dash(ctx, &[6.0, 4.0]);
    for sel in selection {
        let bbox = match sel {
            SelId::Shape(id) => map.shapes.iter().find(|s| &s.id == id).map(shape_bbox),
            SelId::Group(id) => map
                .groups
                .iter()
                .find(|g| &g.id == id)
                .and_then(group_bbox),
        };
        if let Some((x, y, w, h)) = bbox {
            let (sx, sy) = cam.world_to_screen(x * GRID_PX, y * GRID_PX);
            ctx.stroke_rect(sx, sy, w * GRID_PX * cam.zoom, h * GRID_PX * cam.zoom);
        }
    }
    set_dash(ctx, &[]);
}

fn draw_preview(ctx: &CanvasRenderingContext2d, cam: &Camera, (x, y, w, h): GridRect) {
    let (sx, sy) = cam.world_to_screen(x * GRID_PX, y * GRID_PX);
    ctx.set_stroke_style_str(SELECTION_COLOR);
    ctx.set_line_width(1.5);
    set_dash(ctx, &[4.0, 3.0]);
    ctx.stroke_rect(sx, sy, w * GRID_PX * cam.zoom, h * GRID_PX * cam.zoom);
    set_dash(ctx, &[]);
}

fn set_dash(ctx: &CanvasRenderingContext2d, segments: &[f64]) {
    let arr = js_sys::Array::new();
    for s in segments {
        arr.push(&JsValue::from_f64(*s));
    }
    let _ = ctx.set_line_dash(&arr);
}

use wasm_bindgen::JsCast;

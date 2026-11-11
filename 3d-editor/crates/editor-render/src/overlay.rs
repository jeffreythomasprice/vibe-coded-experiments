use editor_geom::Aabb;
use editor_geom::camera::{View, ViewKind};
use glam::{DVec2, DVec3};

use crate::scene::{Drawn, Vertex};

const GRID_MINOR: [f32; 3] = [0.25, 0.27, 0.31];
const GRID_MAJOR: [f32; 3] = [0.4, 0.42, 0.48];
const BOUNDS_COLOR: [f32; 3] = [0.85, 0.8, 0.3];
const AXIS_COLORS: [[f32; 3]; 3] = [[0.9, 0.25, 0.25], [0.35, 0.8, 0.35], [0.3, 0.5, 0.95]];
const MAJOR_EVERY: i64 = 8;
const MIN_SPACING_PX: f64 = 8.0;
const MAX_LINES: f64 = 256.0;

fn segment(out: &mut Vec<Vertex>, a: DVec3, b: DVec3, color: [f32; 3]) {
    out.push(Vertex::new(a, color));
    out.push(Vertex::new(b, color));
}

fn grid_color(index: i64) -> [f32; 3] {
    if index % MAJOR_EVERY == 0 {
        GRID_MAJOR
    } else {
        GRID_MINOR
    }
}

/// Lines of a grid over `[lo, hi]`, mapped to world space by `to_world`.
fn grid_over(
    out: &mut Vec<Vertex>,
    cell: f64,
    lo: DVec2,
    hi: DVec2,
    to_world: impl Fn(f64, f64) -> DVec3,
) {
    let first = |v: f64| (v / cell).ceil() as i64;
    let last = |v: f64| (v / cell).floor() as i64;
    for i in first(lo.x)..=last(hi.x) {
        let u = i as f64 * cell;
        segment(out, to_world(u, lo.y), to_world(u, hi.y), grid_color(i));
    }
    for j in first(lo.y)..=last(hi.y) {
        let v = j as f64 * cell;
        segment(out, to_world(lo.x, v), to_world(hi.x, v), grid_color(j));
    }
}

pub fn grid_lines(view: &View, grid: f64, bounds: Option<&Aabb>) -> Vec<Vertex> {
    let mut out = Vec::new();
    let mut cell = if grid.is_finite() && grid > 0.0 {
        grid
    } else {
        16.0
    };
    match (view.kind, view.ortho_extent()) {
        (ViewKind::Orthographic { zoom }, Some((lo, hi))) => {
            while cell * zoom < MIN_SPACING_PX || (hi - lo).max_element() / cell > MAX_LINES {
                cell *= 2.0;
            }
            let (right, up) = (view.right, view.up);
            grid_over(&mut out, cell, lo, hi, |u, v| right * u + up * v);
        }
        _ => {
            let b = bounds.copied().unwrap_or(Aabb {
                min: DVec3::splat(-64.0),
                max: DVec3::splat(64.0),
            });
            let lo = b.min.truncate().min(DVec2::ZERO);
            let hi = b.max.truncate().max(DVec2::ZERO);
            let span = (hi - lo).max_element();
            while span / cell > MAX_LINES / 4.0 {
                cell *= 2.0;
            }
            let pad = DVec2::splat(span * 0.25 + cell * 2.0);
            grid_over(&mut out, cell, lo - pad, hi + pad, |u, v| {
                DVec3::new(u, v, 0.0)
            });
        }
    }
    out
}

pub fn axes_lines(view: &View, reach: f64) -> Vec<Vertex> {
    let mut length = (reach * 1.25).max(64.0);
    if let Some((lo, hi)) = view.ortho_extent() {
        length = length
            .max(lo.abs().max_element())
            .max(hi.abs().max_element());
    }
    let mut out = Vec::new();
    for (axis, color) in [DVec3::X, DVec3::Y, DVec3::Z].into_iter().zip(AXIS_COLORS) {
        if view.is_ortho() && axis.dot(view.forward).abs() > 0.999 {
            continue;
        }
        segment(&mut out, DVec3::ZERO, axis * length, color);
        segment(
            &mut out,
            DVec3::ZERO,
            -axis * length,
            color.map(|c| c * 0.4),
        );
    }
    out
}

pub fn bounds_lines(drawn: &[Drawn]) -> Vec<Vertex> {
    const EDGES: [(usize, usize); 12] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    let mut out = Vec::new();
    for d in drawn {
        let c = d.bounds.corners();
        for (a, b) in EDGES {
            segment(&mut out, c[a], c[b], BOUNDS_COLOR);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use editor_core::Projection;
    use editor_geom::camera::viewport_camera;

    use super::*;

    fn top_view(zoom: f64) -> View {
        let bounds = Aabb {
            min: DVec3::splat(-100.0),
            max: DVec3::splat(100.0),
        };
        let mut camera = viewport_camera("viewport.top", Some(&bounds), 800, 600).unwrap();
        camera.projection = Projection::Orthographic {
            view: editor_core::OrthoView::Top,
            zoom,
        };
        View::new(&camera, 800, 600, bounds.reach())
    }

    #[test]
    fn a_fine_grid_is_coarsened_when_zoomed_out() {
        let lines = grid_lines(&top_view(1.0), 0.125, None);
        assert!(
            lines.len() / 2 <= 2 * (MAX_LINES as usize + 2),
            "{} lines",
            lines.len() / 2
        );
    }

    #[test]
    fn a_fine_grid_is_kept_when_zoomed_in() {
        let near = grid_lines(&top_view(200.0), 0.125, None);
        let far = grid_lines(&top_view(200.0), 16.0, None);
        assert!(near.len() > far.len(), "{} vs {}", near.len(), far.len());
    }
}

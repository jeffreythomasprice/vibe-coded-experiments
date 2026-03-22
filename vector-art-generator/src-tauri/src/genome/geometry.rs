use geo::{BooleanOps, BoundingRect, Coord, LineString, MultiPolygon, Polygon, Translate};

use super::types::*;

const CIRCLE_SEGMENTS: usize = 48;
const CANVAS_SIZE: f64 = 400.0;

/// Evaluate a CSG tree into a MultiPolygon in canvas coordinates (0..400),
/// centered within the canvas.
pub fn evaluate_csg(node: &CsgNode) -> MultiPolygon<f64> {
    let result = evaluate_csg_inner(node);
    center_in_canvas(result)
}

fn evaluate_csg_inner(node: &CsgNode) -> MultiPolygon<f64> {
    match node {
        CsgNode::Primitive(p) => primitive_to_multi_polygon(p),
        CsgNode::BoolOp { op, left, right } => {
            let l = evaluate_csg_inner(left);
            let r = evaluate_csg_inner(right);
            // geo 0.28's sweep line can panic on certain polygon configurations.
            // Catch the panic and fall back to the left operand to avoid crashing.
            let op = *op;
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match op {
                BoolOpType::Union => l.union(&r),
                BoolOpType::Intersection => l.intersection(&r),
                BoolOpType::Subtraction => l.difference(&r),
            }))
            .unwrap_or(l)
        }
    }
}

fn center_in_canvas(mp: MultiPolygon<f64>) -> MultiPolygon<f64> {
    if let Some(bbox) = mp.bounding_rect() {
        let current_cx = (bbox.min().x + bbox.max().x) / 2.0;
        let current_cy = (bbox.min().y + bbox.max().y) / 2.0;
        let target = CANVAS_SIZE / 2.0;
        mp.translate(target - current_cx, target - current_cy)
    } else {
        mp
    }
}

fn primitive_to_multi_polygon(p: &Primitive) -> MultiPolygon<f64> {
    let coords = match p {
        Primitive::Circle { center, radius } => {
            let cx = center.x * CANVAS_SIZE;
            let cy = center.y * CANVAS_SIZE;
            let r = radius * CANVAS_SIZE;
            (0..CIRCLE_SEGMENTS)
                .map(|i| {
                    let angle =
                        (i as f64 / CIRCLE_SEGMENTS as f64) * std::f64::consts::TAU;
                    Coord {
                        x: cx + angle.cos() * r,
                        y: cy + angle.sin() * r,
                    }
                })
                .collect::<Vec<_>>()
        }
        Primitive::Rectangle {
            center,
            width,
            height,
            rotation,
        } => {
            let cx = center.x * CANVAS_SIZE;
            let cy = center.y * CANVAS_SIZE;
            let hw = width * CANVAS_SIZE * 0.5;
            let hh = height * CANVAS_SIZE * 0.5;
            let corners = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)];
            let cos_r = rotation.cos();
            let sin_r = rotation.sin();
            corners
                .iter()
                .map(|&(dx, dy)| Coord {
                    x: cx + dx * cos_r - dy * sin_r,
                    y: cy + dx * sin_r + dy * cos_r,
                })
                .collect::<Vec<_>>()
        }
        Primitive::RegularPolygon {
            center,
            radius,
            sides,
            rotation,
        } => {
            let cx = center.x * CANVAS_SIZE;
            let cy = center.y * CANVAS_SIZE;
            let r = radius * CANVAS_SIZE;
            let n = *sides as usize;
            (0..n)
                .map(|i| {
                    let angle =
                        (i as f64 / n as f64) * std::f64::consts::TAU + rotation;
                    Coord {
                        x: cx + angle.cos() * r,
                        y: cy + angle.sin() * r,
                    }
                })
                .collect::<Vec<_>>()
        }
    };

    let polygon = Polygon::new(LineString::new(coords), vec![]);
    MultiPolygon(vec![polygon])
}

use glam::Vec2;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::scale::{FixedScaleFloatOverlay, FixedScaleOverlayError};
use thiserror::Error;

use super::csg::{Csg, NodeId, NodeRef};

/// Grid resolution (in world units) used for every polygon boolean op. A
/// single fixed value here — rather than i_overlay's default per-call
/// auto-scale — is what will let two independently-computed shapes (e.g.
/// neighbouring chunks, once M6's streaming lands) quantize a shared wall
/// identically instead of producing seams between them.
const FIXED_SCALE: f32 = 4096.0;
const FILL_RULE: FillRule = FillRule::NonZero;

#[derive(Debug, Error)]
pub enum ContourError {
    #[error("polygon overlay failed: {0:?}")]
    Overlay(FixedScaleOverlayError),
}

pub type Shape = Vec<Vec<Vec2>>;
pub type Shapes = Vec<Shape>;

/// Extracts the boolean-composed contour(s) of the subtree rooted at `id`,
/// as a set of shapes (outer CCW + holes CW — `i_overlay`'s convention,
/// which also happens to be what `lyon`'s fill tessellator wants). `tol`
/// controls how finely curved primitives are flattened before the boolean
/// ops run.
pub fn extract(csg: &Csg, id: NodeId, tol: f32) -> Result<Shapes, ContourError> {
    match csg.node(id) {
        NodeRef::Leaf(prim) => Ok(vec![vec![prim.flatten(tol)]]),
        NodeRef::Union(kids) => {
            let mut acc: Option<Shapes> = None;
            for &k in kids {
                let shape = extract(csg, k, tol)?;
                acc = Some(match acc {
                    None => shape,
                    Some(current) => overlay(&current, &shape, OverlayRule::Union)?,
                });
            }
            Ok(acc.unwrap_or_default())
        }
        NodeRef::Intersect(kids) => {
            // `difference()` builds `Intersect(a, Complement(b))`, so a
            // `Complement` kid here means "subtract", handled directly via
            // `OverlayRule::Difference` rather than trying to materialize
            // the complement's own (infinite) shape. This assumes the
            // positive operand always comes first, which is how this
            // module's own `Csg::difference` builds it.
            let mut acc: Option<Shapes> = None;
            for &k in kids {
                let (rule, operand) = match csg.node(k) {
                    NodeRef::Complement(inner) => (OverlayRule::Difference, extract(csg, inner, tol)?),
                    _ => (OverlayRule::Intersect, extract(csg, k, tol)?),
                };
                acc = Some(match acc {
                    None => operand,
                    Some(current) => overlay(&current, &operand, rule)?,
                });
            }
            Ok(acc.unwrap_or_default())
        }
        // A bare `Complement` has no finite contour — the whole plane minus
        // a shape isn't representable as a bounded polygon set. The
        // normal-form invariant (no part root is a bare `Complement`) means
        // this is only reachable through misuse; return empty rather than
        // panic, since contour export is debug/crisp-mode tooling.
        NodeRef::Complement(_) => Ok(Vec::new()),
        NodeRef::Transform(child, sim) => {
            let inner = extract(csg, child, tol)?;
            Ok(transform_shapes(&inner, sim))
        }
    }
}

fn overlay(a: &Shapes, b: &Shapes, rule: OverlayRule) -> Result<Shapes, ContourError> {
    a.overlay_with_fixed_scale(b, rule, FILL_RULE, FIXED_SCALE)
        .map_err(ContourError::Overlay)
}

fn transform_shapes(shapes: &Shapes, sim: &super::primitive::Similarity) -> Shapes {
    shapes
        .iter()
        .map(|shape| {
            shape
                .iter()
                .map(|contour| contour.iter().map(|&p| sim.to_world(p)).collect())
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::primitive::{Circle, Primitive, RoundRect};

    /// Twice the signed area (shoelace formula): positive for CCW, negative
    /// for CW. Matches `i_overlay`'s documented "outer CCW, holes CW".
    fn signed_area_x2(contour: &[Vec2]) -> f32 {
        let n = contour.len();
        (0..n)
            .map(|i| {
                let a = contour[i];
                let b = contour[(i + 1) % n];
                a.x * b.y - b.x * a.y
            })
            .sum()
    }

    #[test]
    fn difference_produces_one_outer_and_one_hole() {
        let mut csg = Csg::new();
        let outer = csg.leaf(Primitive::RoundRect(RoundRect {
            half_extents: Vec2::new(2.0, 2.0),
            radius: 0.0,
        }));
        let hole = csg.leaf(Primitive::Circle(Circle { radius: 0.5 }));
        let root = csg.difference(outer, hole);
        csg.finalize();

        let shapes = extract(&csg, root, 0.01).expect("overlay should succeed");
        assert_eq!(shapes.len(), 1, "expected exactly one shape");
        assert_eq!(shapes[0].len(), 2, "expected an outer contour plus one hole");

        let areas: Vec<f32> = shapes[0].iter().map(|c| signed_area_x2(c)).collect();
        assert!(areas.iter().any(|&a| a > 0.0), "expected a CCW outer contour");
        assert!(areas.iter().any(|&a| a < 0.0), "expected a CW hole contour");
    }

    #[test]
    fn union_of_disjoint_circles_produces_two_shapes() {
        let mut csg = Csg::new();
        let a = csg.leaf(Primitive::Circle(Circle { radius: 1.0 }));
        let b_leaf = csg.leaf(Primitive::Circle(Circle { radius: 1.0 }));
        let b = csg.transform(
            b_leaf,
            crate::geom::primitive::Similarity::translate(Vec2::new(10.0, 0.0)),
        );
        let root = csg.union(vec![a, b]);
        csg.finalize();

        let shapes = extract(&csg, root, 0.01).expect("overlay should succeed");
        assert_eq!(shapes.len(), 2);
        for shape in &shapes {
            assert_eq!(shape.len(), 1, "disjoint circles should have no holes");
            assert!(signed_area_x2(&shape[0]) > 0.0);
        }
    }
}

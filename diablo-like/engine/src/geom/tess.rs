use glam::Vec2;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, StrokeVertexConstructor, TessellationError,
    VertexBuffers,
};
use thiserror::Error;

use super::contour::Shapes;

#[derive(Debug, Error)]
pub enum TessError {
    #[error("fill tessellation failed: {0:?}")]
    Fill(TessellationError),
    #[error("stroke tessellation failed: {0:?}")]
    Stroke(TessellationError),
}

pub struct Mesh {
    pub vertices: Vec<Vec2>,
    pub indices: Vec<u32>,
}

fn build_path(shapes: &Shapes) -> Path {
    let mut builder = Path::builder();
    for shape in shapes {
        for contour in shape {
            if contour.len() < 3 {
                continue;
            }
            let mut points = contour.iter();
            let first = points.next().unwrap();
            builder.begin(lyon::math::point(first.x, first.y));
            for p in points {
                builder.line_to(lyon::math::point(p.x, p.y));
            }
            builder.end(true);
        }
    }
    builder.build()
}

struct FillCtor;

impl FillVertexConstructor<Vec2> for FillCtor {
    fn new_vertex(&mut self, vertex: FillVertex) -> Vec2 {
        let p = vertex.position();
        Vec2::new(p.x, p.y)
    }
}

/// Tessellates a boolean-composed shape set (see `contour::extract`) into a
/// triangle-list fill mesh. Winding follows i_overlay's "outer CCW, holes
/// CW" convention, interpreted with the matching non-zero fill rule.
pub fn fill(shapes: &Shapes, tolerance: f32) -> Result<Mesh, TessError> {
    let path = build_path(shapes);

    let mut geometry: VertexBuffers<Vec2, u32> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();
    tessellator
        .tessellate_path(
            &path,
            &FillOptions::non_zero().with_tolerance(tolerance),
            &mut BuffersBuilder::new(&mut geometry, FillCtor),
        )
        .map_err(TessError::Fill)?;

    Ok(Mesh {
        vertices: geometry.vertices,
        indices: geometry.indices,
    })
}

struct StrokeCtor;

impl StrokeVertexConstructor<Vec2> for StrokeCtor {
    fn new_vertex(&mut self, vertex: StrokeVertex) -> Vec2 {
        let p = vertex.position();
        Vec2::new(p.x, p.y)
    }
}

/// Tessellates the same contours' *outlines* into a triangle-list stroke
/// mesh of constant world-space `width` — the exact-geometry edge drawn in
/// both render modes as the cross-check against the baked field (see the
/// design plan's Frame graph note).
pub fn stroke(shapes: &Shapes, width: f32, tolerance: f32) -> Result<Mesh, TessError> {
    let path = build_path(shapes);

    let mut geometry: VertexBuffers<Vec2, u32> = VertexBuffers::new();
    let mut tessellator = StrokeTessellator::new();
    tessellator
        .tessellate_path(
            &path,
            &StrokeOptions::default()
                .with_line_width(width)
                .with_line_join(LineJoin::Round)
                .with_tolerance(tolerance),
            &mut BuffersBuilder::new(&mut geometry, StrokeCtor),
        )
        .map_err(TessError::Stroke)?;

    Ok(Mesh {
        vertices: geometry.vertices,
        indices: geometry.indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::contour;
    use crate::geom::csg::Csg;
    use crate::geom::primitive::{Circle, Primitive, RoundRect};

    fn mesh_area(mesh: &Mesh) -> f32 {
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|tri| {
                let a = mesh.vertices[tri[0] as usize];
                let b = mesh.vertices[tri[1] as usize];
                let c = mesh.vertices[tri[2] as usize];
                ((b - a).x * (c - a).y - (c - a).x * (b - a).y).abs() * 0.5
            })
            .sum()
    }

    #[test]
    fn fill_area_matches_shape_with_hole() {
        let mut csg = Csg::new();
        let outer = csg.leaf(Primitive::RoundRect(RoundRect {
            half_extents: Vec2::new(2.0, 2.0),
            radius: 0.0,
        }));
        let hole = csg.leaf(Primitive::Circle(Circle { radius: 0.5 }));
        let root = csg.difference(outer, hole);
        csg.finalize();

        let shapes = contour::extract(&csg, root, 0.001).expect("overlay should succeed");
        let mesh = fill(&shapes, 0.01).expect("tessellation should succeed");

        assert!(!mesh.indices.is_empty());

        let expected = 4.0 * 4.0 - std::f32::consts::PI * 0.5 * 0.5;
        let actual = mesh_area(&mesh);
        assert!(
            (actual - expected).abs() < 0.05,
            "mesh area {actual} did not match expected {expected}"
        );
    }

    #[test]
    fn stroke_area_matches_perimeter_times_width() {
        let mut csg = Csg::new();
        let circle = csg.leaf(Primitive::Circle(Circle { radius: 2.0 }));
        csg.finalize();

        let shapes = contour::extract(&csg, circle, 0.001).expect("overlay should succeed");
        let width = 0.05;
        let mesh = stroke(&shapes, width, 0.001).expect("tessellation should succeed");

        assert!(!mesh.indices.is_empty());

        let expected = 2.0 * std::f32::consts::PI * 2.0 * width;
        let actual = mesh_area(&mesh);
        assert!(
            (actual - expected).abs() < 0.02,
            "stroke area {actual} did not match expected {expected}"
        );
    }
}

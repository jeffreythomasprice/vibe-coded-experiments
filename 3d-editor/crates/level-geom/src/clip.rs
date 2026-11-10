use glam::DVec3;
use level_core::{Brush, BrushFace, FaceAttrs};

use crate::plane::plane_from_points;
use crate::polytope::{BrushError, normalize};

#[derive(Debug, Clone, PartialEq)]
pub struct ClipResult {
    pub front: Option<Brush>,
    pub back: Option<Brush>,
}

pub fn clip_brush(
    brush: &Brush,
    points: [DVec3; 3],
    attrs: FaceAttrs,
) -> Result<ClipResult, BrushError> {
    plane_from_points(points).map_err(|_| BrushError::DegenerateClipPlane)?;
    let [a, b, c] = points;
    Ok(ClipResult {
        front: piece(brush, [a, c, b], &attrs)?,
        back: piece(brush, [a, b, c], &attrs)?,
    })
}

fn piece(
    brush: &Brush,
    points: [DVec3; 3],
    attrs: &FaceAttrs,
) -> Result<Option<Brush>, BrushError> {
    let mut b = brush.clone();
    b.faces.push(BrushFace {
        id: brush.next_face_id(),
        points,
        attrs: attrs.clone(),
    });
    match normalize(&b) {
        Ok(b) => Ok(Some(b)),
        Err(BrushError::Empty) => Ok(None),
        Err(e) => Err(e),
    }
}

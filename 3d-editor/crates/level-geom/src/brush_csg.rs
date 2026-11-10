use level_core::{Brush, FaceId};

use crate::clip::clip_brush;
use crate::polytope::{BrushError, normalize};

pub fn brush_intersect(a: &Brush, b: &Brush) -> Result<Option<Brush>, BrushError> {
    let mut out = a.clone();
    let first = a.next_face_id().0;
    for (i, f) in b.faces.iter().enumerate() {
        let mut f = f.clone();
        f.id = FaceId(first + i as u32);
        out.faces.push(f);
    }
    match normalize(&out) {
        Ok(b) => Ok(Some(b)),
        Err(BrushError::Empty) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Convex fragments of `a` outside `b`.
pub fn brush_subtract(a: &Brush, b: &Brush) -> Result<Vec<Brush>, BrushError> {
    if brush_intersect(a, b)?.is_none() {
        return Ok(vec![a.clone()]);
    }
    let b = normalize(b)?;
    let mut fragments = Vec::new();
    let mut remaining = a.clone();
    for face in &b.faces {
        let r = clip_brush(&remaining, face.points, face.attrs.clone())?;
        if let Some(front) = r.front {
            fragments.push(front);
        }
        match r.back {
            Some(back) => remaining = back,
            None => break,
        }
    }
    Ok(fragments)
}

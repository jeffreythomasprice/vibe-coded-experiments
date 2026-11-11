use editor_core::{Brush, BrushFace, FaceId};
use glam::DVec3;

use crate::clip::clip_brush;
use crate::hull::{HullError, convex_hull};
use crate::plane::{plane_from_points, same_plane};
use crate::polytope::{BrushError, brush_geometry, normalize};
use crate::tolerance::snap_vec;
use crate::vertex_edit::plane_points;

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

/// The convex hull of all the brushes' corners as one brush.
/// A hull face on an input face's plane keeps that face's points and attributes;
/// any other face takes the attributes of the input face with the closest normal.
pub fn brush_merge(brushes: &[Brush]) -> Result<Brush, BrushError> {
    let mut corners = Vec::new();
    let mut sources: Vec<(editor_core::Plane, &BrushFace)> = Vec::new();
    for brush in brushes {
        let geo = brush_geometry(brush)?;
        corners.extend(geo.vertices.iter().copied());
        for g in &geo.faces {
            if let Some(face) = brush.faces.iter().find(|f| f.id == g.face) {
                sources.push((g.plane, face));
            }
        }
    }
    let hull = convex_hull(&corners).map_err(|HullError::Flat| BrushError::Empty)?;
    let faces = hull
        .faces
        .iter()
        .enumerate()
        .map(|(i, hf)| {
            let exact = sources.iter().find(|(p, _)| same_plane(p, &hf.plane));
            let (points, attrs) = match exact {
                Some((_, f)) => (f.points, f.attrs.clone()),
                None => {
                    let donor = sources
                        .iter()
                        .max_by(|a, b| {
                            a.0.normal
                                .dot(hf.plane.normal)
                                .total_cmp(&b.0.normal.dot(hf.plane.normal))
                        })
                        .map(|(_, f)| f.attrs.clone())
                        .unwrap_or_default();
                    (plane_points(&hull, &hf.corners), donor)
                }
            };
            BrushFace {
                id: FaceId(i as u32 + 1),
                points,
                attrs,
            }
        })
        .collect();
    normalize(&Brush { faces })
}

/// The brush with every face moved `thickness` inward. Too large a thickness is `Empty`.
pub fn brush_inset(brush: &Brush, thickness: f64) -> Result<Brush, BrushError> {
    let mut out = brush.clone();
    for face in &mut out.faces {
        let plane =
            plane_from_points(face.points).map_err(|_| BrushError::DegeneratePlane(face.id))?;
        let shift: DVec3 = -plane.normal * thickness;
        face.points = face.points.map(|p| snap_vec(p + shift));
    }
    normalize(&out)
}

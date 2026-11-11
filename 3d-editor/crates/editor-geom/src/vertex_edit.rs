use std::collections::{BTreeMap, BTreeSet};

use editor_core::{Brush, BrushFace, FaceAttrs, FaceId};
use glam::DVec3;

use crate::hull::{Hull, HullError, convex_hull};
use crate::plane::same_plane;
use crate::polytope::{BrushError, brush_geometry, normalize};
use crate::tolerance::{WELD_EPS, snap_vec};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum VertexEditError {
    #[error("the brush has no vertex at {0}")]
    VertexNotFound(DVec3),
    #[error(
        "the vertex at {0} would end up inside the brush or on an edge, which makes it concave"
    )]
    WouldBeConcave(DVec3),
    #[error(transparent)]
    Brush(#[from] BrushError),
}

/// Moves the brush's vertices nearest to `picks` by `delta` and rebuilds the brush as the convex hull of the result.
/// Vertices that land on each other fuse, and a face the move makes non-planar splits into several planes.
/// A face keeps its id when its plane or its corners are unchanged.
pub fn move_brush_vertices(
    brush: &Brush,
    picks: &[DVec3],
    delta: DVec3,
) -> Result<Brush, VertexEditError> {
    let geo = brush_geometry(brush)?;
    let mut moving = vec![false; geo.vertices.len()];
    for &pick in picks {
        let nearest = geo
            .vertices
            .iter()
            .enumerate()
            .map(|(i, v)| (i, (*v - pick).length()))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match nearest {
            Some((i, d)) if d <= WELD_EPS => moving[i] = true,
            _ => return Err(VertexEditError::VertexNotFound(pick)),
        }
    }
    let after: Vec<DVec3> = geo
        .vertices
        .iter()
        .zip(&moving)
        .map(|(&v, &m)| if m { snap_vec(v + delta) } else { v })
        .collect();
    if after == geo.vertices {
        return Ok(brush.clone());
    }

    let hull = convex_hull(&after).map_err(|HullError::Flat| BrushError::Empty)?;
    if let Some(i) = (0..after.len()).find(|&i| !hull.is_corner(hull.input[i])) {
        return Err(VertexEditError::WouldBeConcave(after[i]));
    }

    let hull_sets: Vec<BTreeSet<usize>> = hull
        .faces
        .iter()
        .map(|f| f.corners.iter().copied().collect())
        .collect();
    let originals: Vec<(FaceId, &editor_core::Plane, BTreeSet<usize>)> = geo
        .faces
        .iter()
        .map(|f| {
            (
                f.face,
                &f.plane,
                f.loop_.iter().map(|&i| hull.input[i]).collect(),
            )
        })
        .collect();

    let mut assigned: Vec<Option<FaceId>> = vec![None; hull.faces.len()];
    let mut claimed: BTreeSet<FaceId> = BTreeSet::new();
    let mut repoint: BTreeMap<FaceId, [DVec3; 3]> = BTreeMap::new();
    for (h, face) in hull.faces.iter().enumerate() {
        if let Some((id, ..)) = originals
            .iter()
            .find(|(id, plane, _)| !claimed.contains(id) && same_plane(plane, &face.plane))
        {
            assigned[h] = Some(*id);
            claimed.insert(*id);
        }
    }
    for (h, face) in hull.faces.iter().enumerate() {
        if assigned[h].is_some() {
            continue;
        }
        if let Some((id, ..)) = originals
            .iter()
            .find(|(id, _, set)| !claimed.contains(id) && *set == hull_sets[h])
        {
            assigned[h] = Some(*id);
            claimed.insert(*id);
            repoint.insert(*id, plane_points(&hull, &face.corners));
        }
    }

    let mut faces: Vec<BrushFace> = brush
        .faces
        .iter()
        .filter(|f| claimed.contains(&f.id))
        .cloned()
        .map(|mut f| {
            if let Some(points) = repoint.get(&f.id) {
                f.points = *points;
            }
            f
        })
        .collect();
    let mut next_id = brush.next_face_id().0;
    for (h, face) in hull.faces.iter().enumerate() {
        if assigned[h].is_some() {
            continue;
        }
        let donor = originals
            .iter()
            .map(|(id, _, set)| (*id, set.intersection(&hull_sets[h]).count()))
            .filter(|(_, shared)| *shared > 0)
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
            .and_then(|(id, _)| brush.faces.iter().find(|f| f.id == id))
            .or(brush.faces.first())
            .map_or_else(FaceAttrs::default, |f| f.attrs.clone());
        faces.push(BrushFace {
            id: FaceId(next_id),
            points: plane_points(&hull, &face.corners),
            attrs: donor,
        });
        next_id += 1;
    }

    let out = normalize(&Brush { faces })?;
    let derived = brush_geometry(&out)?;
    for (i, p) in hull.points.iter().enumerate() {
        if hull.is_corner(i)
            && !derived
                .vertices
                .iter()
                .any(|v| (*v - *p).length() <= WELD_EPS)
        {
            return Err(VertexEditError::WouldBeConcave(*p));
        }
    }
    Ok(out)
}

/// Three corners spanning the largest triangle, in the clockwise-from-outside order faces store.
fn plane_points(hull: &Hull, corners: &[usize]) -> [DVec3; 3] {
    let at = |i: usize| hull.points[corners[i]];
    let mut best = (0, 1, 2);
    let mut best_area = -1.0;
    for i in 0..corners.len() {
        for j in i + 1..corners.len() {
            for k in j + 1..corners.len() {
                let area = (at(j) - at(i)).cross(at(k) - at(i)).length();
                if area > best_area {
                    best_area = area;
                    best = (i, j, k);
                }
            }
        }
    }
    let (i, j, k) = best;
    [at(i), at(k), at(j)]
}

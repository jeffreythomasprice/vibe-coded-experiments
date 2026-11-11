use editor_core::{Brush, FaceId, Plane};
use glam::DVec3;

use crate::plane::{GeomError, opposite_plane, plane_from_points, same_plane};
use crate::tolerance::{SIDE_EPS, WORLD_BOUND, Welder, snap_vec};
use crate::winding::{base_winding, clip_back, is_sliver};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BrushError {
    #[error("a brush needs at least 4 faces")]
    TooFewFaces,
    #[error("face {0:?} has collinear points")]
    DegeneratePlane(FaceId),
    #[error("the clip plane has collinear points")]
    DegenerateClipPlane,
    #[error("the brush is not bounded")]
    Unbounded,
    #[error("the brush encloses no volume")]
    Empty,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryFace {
    pub face: FaceId,
    pub plane: Plane,
    pub loop_: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrushGeometry {
    pub vertices: Vec<DVec3>,
    pub faces: Vec<GeometryFace>,
    pub dropped: Vec<FaceId>,
}

impl BrushGeometry {
    pub fn face_positions(&self, face: &GeometryFace) -> Vec<DVec3> {
        face.loop_.iter().map(|&i| self.vertices[i]).collect()
    }

    pub fn volume(&self) -> f64 {
        self.faces
            .iter()
            .map(|f| crate::volume::face_volume(&self.face_positions(f)))
            .sum()
    }
}

pub fn brush_geometry(brush: &Brush) -> Result<BrushGeometry, BrushError> {
    if brush.faces.len() < 4 {
        return Err(BrushError::TooFewFaces);
    }
    let mut planes = Vec::with_capacity(brush.faces.len());
    for f in &brush.faces {
        let p = plane_from_points(f.points)
            .map_err(|GeomError::DegeneratePlane| BrushError::DegeneratePlane(f.id))?;
        planes.push(p);
    }

    let mut dropped = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    for i in 0..planes.len() {
        if active.iter().any(|&j| same_plane(&planes[j], &planes[i])) {
            dropped.push(brush.faces[i].id);
        } else {
            active.push(i);
        }
    }
    for (k, &i) in active.iter().enumerate() {
        for &j in &active[k + 1..] {
            if opposite_plane(&planes[i], &planes[j]) {
                return Err(BrushError::Empty);
            }
        }
    }

    let mut windings: Vec<(usize, Vec<DVec3>)> = Vec::new();
    for &i in &active {
        let mut w = base_winding(&planes[i]);
        for &j in &active {
            if j == i {
                continue;
            }
            w = clip_back(&w, &planes[j]);
            if w.is_empty() {
                break;
            }
        }
        if w.len() < 3 || is_sliver(&w, SIDE_EPS) {
            dropped.push(brush.faces[i].id);
        } else {
            windings.push((i, w));
        }
    }
    if windings.len() < 4 {
        return Err(BrushError::Empty);
    }
    if windings
        .iter()
        .any(|(_, w)| w.iter().any(|p| p.length() > WORLD_BOUND / 2.0))
    {
        return Err(BrushError::Unbounded);
    }

    let mut vertices: Vec<DVec3> = Vec::new();
    let mut welder = Welder::default();
    let mut faces = Vec::new();
    for (i, w) in windings {
        let mut loop_: Vec<usize> = Vec::with_capacity(w.len());
        for p in w {
            let (idx, added) = welder.insert(p);
            if added {
                vertices.push(snap_vec(p));
            }
            if loop_.last() != Some(&idx) {
                loop_.push(idx);
            }
        }
        while loop_.len() > 1 && loop_.first() == loop_.last() {
            loop_.pop();
        }
        if loop_.len() < 3 {
            dropped.push(brush.faces[i].id);
            continue;
        }
        faces.push(GeometryFace {
            face: brush.faces[i].id,
            plane: planes[i],
            loop_,
        });
    }
    if faces.len() < 4 {
        return Err(BrushError::Empty);
    }
    Ok(BrushGeometry {
        vertices,
        faces,
        dropped,
    })
}

pub fn normalize(brush: &Brush) -> Result<Brush, BrushError> {
    let geo = brush_geometry(brush)?;
    let mut out = brush.clone();
    out.faces.retain(|f| !geo.dropped.contains(&f.id));
    Ok(out)
}

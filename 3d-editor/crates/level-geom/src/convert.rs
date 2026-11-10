use glam::DVec3;
use level_core::{Brush, BrushFace, FaceId, Mesh, MeshFace, MeshVertex, VertexId};

use crate::mesh::validate::{MeshIssue, validate};
use crate::polytope::{BrushError, brush_geometry, normalize};
use crate::tolerance::PLANAR_EPS;
use crate::winding::newell;

pub fn brush_to_mesh(brush: &Brush) -> Result<Mesh, BrushError> {
    let geo = brush_geometry(brush)?;
    let vertices = geo
        .vertices
        .iter()
        .enumerate()
        .map(|(i, &pos)| MeshVertex {
            id: VertexId(i as u32 + 1),
            pos,
        })
        .collect();
    let faces = geo
        .faces
        .iter()
        .map(|f| {
            let src = brush.faces.iter().find(|b| b.id == f.face).unwrap();
            MeshFace {
                id: f.face,
                verts: f.loop_.iter().map(|&i| VertexId(i as u32 + 1)).collect(),
                attrs: src.attrs.clone(),
            }
        })
        .collect();
    Ok(Mesh { vertices, faces })
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MeshToBrushError {
    #[error("mesh is not closed and consistently wound")]
    NotClosed,
    #[error("face {0:?} is not planar or has no area")]
    BadFace(FaceId),
    #[error("mesh is not convex")]
    NotConvex,
    #[error(transparent)]
    Brush(#[from] BrushError),
}

pub fn mesh_to_brush(mesh: &Mesh) -> Result<Brush, MeshToBrushError> {
    let issues = validate(mesh);
    if issues.iter().any(|i| {
        matches!(
            i,
            MeshIssue::OpenEdge { .. }
                | MeshIssue::NonManifoldEdge { .. }
                | MeshIssue::InconsistentWinding { .. }
        )
    }) {
        return Err(MeshToBrushError::NotClosed);
    }
    let mut faces = Vec::new();
    let mut planes = Vec::new();
    for f in &mesh.faces {
        let poly: Vec<DVec3> = f.verts.iter().filter_map(|&v| mesh.position(v)).collect();
        if poly.len() != f.verts.len() || poly.len() < 3 {
            return Err(MeshToBrushError::BadFace(f.id));
        }
        let nv = newell(&poly) * 2.0;
        if nv.length() < 1e-9 {
            return Err(MeshToBrushError::BadFace(f.id));
        }
        let n = nv.normalize();
        let d = n.dot(poly[0]);
        if poly.iter().any(|p| (n.dot(*p) - d).abs() > PLANAR_EPS) {
            return Err(MeshToBrushError::BadFace(f.id));
        }
        let far = (1..poly.len())
            .max_by(|&i, &j| {
                (poly[i] - poly[0])
                    .length_squared()
                    .total_cmp(&(poly[j] - poly[0]).length_squared())
            })
            .unwrap();
        let third = (1..poly.len())
            .filter(|&i| i != far)
            .max_by(|&i, &j| {
                let ci = (poly[far] - poly[0]).cross(poly[i] - poly[0]).length();
                let cj = (poly[far] - poly[0]).cross(poly[j] - poly[0]).length();
                ci.total_cmp(&cj)
            })
            .unwrap();
        let (p, q, r) = (poly[0], poly[far], poly[third]);
        let points = if (q - p).cross(r - p).dot(n) > 0.0 {
            [p, r, q]
        } else {
            [p, q, r]
        };
        planes.push((n, d));
        faces.push(BrushFace {
            id: f.id,
            points,
            attrs: f.attrs.clone(),
        });
    }
    for (n, d) in &planes {
        if mesh.vertices.iter().any(|v| n.dot(v.pos) - d > PLANAR_EPS) {
            return Err(MeshToBrushError::NotConvex);
        }
    }
    Ok(normalize(&Brush { faces })?)
}

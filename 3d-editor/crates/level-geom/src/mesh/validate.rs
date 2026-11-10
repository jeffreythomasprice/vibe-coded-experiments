use std::collections::{HashMap, HashSet};

use glam::DVec3;
use level_core::{FaceId, Mesh, VertexId};

use crate::tolerance::{AREA_EPS, PLANAR_EPS};
use crate::volume::mesh_volume;
use crate::winding::newell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MeshIssue {
    DanglingVertex {
        face: FaceId,
        vertex: VertexId,
    },
    FaceTooSmall {
        face: FaceId,
    },
    DuplicateVertexInFace {
        face: FaceId,
        vertex: VertexId,
    },
    ZeroAreaFace {
        face: FaceId,
    },
    NonPlanarFace {
        face: FaceId,
        deviation: f64,
    },
    OpenEdge {
        a: VertexId,
        b: VertexId,
    },
    NonManifoldEdge {
        a: VertexId,
        b: VertexId,
        forward: u32,
        backward: u32,
    },
    InconsistentWinding {
        a: VertexId,
        b: VertexId,
    },
    UnusedVertex {
        vertex: VertexId,
    },
    InsideOut,
}

impl MeshIssue {
    pub fn severity(&self) -> Severity {
        match self {
            MeshIssue::DanglingVertex { .. }
            | MeshIssue::FaceTooSmall { .. }
            | MeshIssue::DuplicateVertexInFace { .. } => Severity::Error,
            _ => Severity::Warning,
        }
    }
}

pub fn validate(mesh: &Mesh) -> Vec<MeshIssue> {
    let mut issues = Vec::new();
    let positions: HashMap<VertexId, DVec3> = mesh.vertices.iter().map(|v| (v.id, v.pos)).collect();
    let mut used: HashSet<VertexId> = HashSet::new();
    let mut edges: HashMap<(VertexId, VertexId), (u32, u32)> = HashMap::new();
    let mut structurally_ok = true;

    for f in &mesh.faces {
        if f.verts.len() < 3 {
            issues.push(MeshIssue::FaceTooSmall { face: f.id });
            structurally_ok = false;
            continue;
        }
        let mut seen = HashSet::new();
        let mut poly = Vec::with_capacity(f.verts.len());
        let mut face_ok = true;
        for &v in &f.verts {
            used.insert(v);
            match positions.get(&v) {
                Some(&p) => poly.push(p),
                None => {
                    issues.push(MeshIssue::DanglingVertex {
                        face: f.id,
                        vertex: v,
                    });
                    face_ok = false;
                }
            }
            if !seen.insert(v) {
                issues.push(MeshIssue::DuplicateVertexInFace {
                    face: f.id,
                    vertex: v,
                });
                face_ok = false;
            }
        }
        if !face_ok {
            structurally_ok = false;
            continue;
        }
        for i in 0..f.verts.len() {
            let a = f.verts[i];
            let b = f.verts[(i + 1) % f.verts.len()];
            let entry = edges.entry((a.min(b), a.max(b))).or_insert((0, 0));
            if a < b {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
        }
        let nv = newell(&poly);
        if nv.length() <= AREA_EPS {
            issues.push(MeshIssue::ZeroAreaFace { face: f.id });
            continue;
        }
        let n = nv.normalize();
        let centroid = poly.iter().copied().sum::<DVec3>() / poly.len() as f64;
        let deviation = poly
            .iter()
            .map(|p| n.dot(*p - centroid).abs())
            .fold(0.0, f64::max);
        if deviation > PLANAR_EPS {
            issues.push(MeshIssue::NonPlanarFace {
                face: f.id,
                deviation,
            });
        }
    }

    let mut edge_list: Vec<_> = edges.into_iter().collect();
    edge_list.sort_by_key(|(k, _)| *k);
    let mut edges_ok = true;
    for ((a, b), (fwd, back)) in edge_list {
        match (fwd, back) {
            (1, 1) => {}
            (1, 0) | (0, 1) => {
                edges_ok = false;
                issues.push(MeshIssue::OpenEdge { a, b });
            }
            (2, 0) | (0, 2) => {
                edges_ok = false;
                issues.push(MeshIssue::InconsistentWinding { a, b });
            }
            _ => {
                edges_ok = false;
                issues.push(MeshIssue::NonManifoldEdge {
                    a,
                    b,
                    forward: fwd,
                    backward: back,
                });
            }
        }
    }
    for v in &mesh.vertices {
        if !used.contains(&v.id) {
            issues.push(MeshIssue::UnusedVertex { vertex: v.id });
        }
    }
    if structurally_ok && edges_ok && !mesh.faces.is_empty() && mesh_volume(mesh) < -1e-6 {
        issues.push(MeshIssue::InsideOut);
    }
    issues
}

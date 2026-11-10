use std::collections::HashMap;

use level_core::{Mesh, MeshFace, MeshVertex, VertexId};

use crate::tolerance::{WELD_EPS, Welder, snap_vec};
use crate::winding::is_sliver;

/// Merges vertices that share a snapped position, drops collapsed faces and unused vertices.
pub fn weld(mesh: &mut Mesh) {
    let mut welder = Welder::default();
    let mut ids: Vec<VertexId> = Vec::new();
    let mut remap: HashMap<VertexId, VertexId> = HashMap::new();
    let mut kept: Vec<MeshVertex> = Vec::new();
    for v in &mesh.vertices {
        let (i, added) = welder.insert(v.pos);
        if added {
            kept.push(MeshVertex {
                id: v.id,
                pos: snap_vec(v.pos),
            });
            ids.push(v.id);
        }
        remap.insert(v.id, ids[i]);
    }
    let mut next_face = mesh.next_face_id().0;
    let positions: HashMap<VertexId, glam::DVec3> = kept.iter().map(|v| (v.id, v.pos)).collect();
    let mut faces = Vec::with_capacity(mesh.faces.len());
    for f in std::mem::take(&mut mesh.faces) {
        let mut loop_: Vec<VertexId> = Vec::with_capacity(f.verts.len());
        for v in &f.verts {
            let v = remap.get(v).copied().unwrap_or(*v);
            if loop_.last() != Some(&v) {
                loop_.push(v);
            }
        }
        while loop_.len() > 1 && loop_.first() == loop_.last() {
            loop_.pop();
        }
        let lobes = split_pinched(loop_);
        let pinched = lobes.len() > 1;
        for (i, lobe) in lobes.into_iter().enumerate() {
            if lobe.len() < 3 {
                continue;
            }
            if pinched {
                let poly: Vec<glam::DVec3> = lobe.iter().map(|v| positions[v]).collect();
                if is_sliver(&poly, WELD_EPS) {
                    continue;
                }
            }
            let id = if i == 0 {
                f.id
            } else {
                next_face += 1;
                level_core::FaceId(next_face - 1)
            };
            faces.push(MeshFace {
                id,
                verts: lobe,
                attrs: f.attrs.clone(),
            });
        }
    }
    mesh.faces = faces;
    let used: std::collections::HashSet<VertexId> = mesh
        .faces
        .iter()
        .flat_map(|f| f.verts.iter().copied())
        .collect();
    kept.retain(|v| used.contains(&v.id));
    mesh.vertices = kept;
}

/// Splits a loop that visits a vertex more than once into simple loops.
fn split_pinched(loop_: Vec<VertexId>) -> Vec<Vec<VertexId>> {
    let mut path: Vec<VertexId> = Vec::with_capacity(loop_.len());
    let mut lobes = Vec::new();
    for v in loop_ {
        if let Some(k) = path.iter().position(|&p| p == v) {
            let lobe = path.split_off(k + 1);
            let mut closed = vec![v];
            closed.extend(lobe);
            lobes.push(closed);
        } else {
            path.push(v);
        }
    }
    lobes.insert(0, path);
    lobes
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;
    use level_core::{FaceAttrs, FaceId, MeshFace};

    #[test]
    fn merges_duplicates() {
        let mut m = Mesh {
            vertices: vec![
                MeshVertex {
                    id: VertexId(1),
                    pos: DVec3::ZERO,
                },
                MeshVertex {
                    id: VertexId(2),
                    pos: DVec3::X,
                },
                MeshVertex {
                    id: VertexId(3),
                    pos: DVec3::Y,
                },
                MeshVertex {
                    id: VertexId(4),
                    pos: DVec3::new(1e-6, 0.0, 0.0),
                },
            ],
            faces: vec![MeshFace {
                id: FaceId(1),
                verts: vec![VertexId(1), VertexId(4), VertexId(2), VertexId(3)],
                attrs: FaceAttrs::default(),
            }],
        };
        weld(&mut m);
        assert_eq!(m.vertices.len(), 3);
        assert_eq!(m.faces[0].verts.len(), 3);
    }
}

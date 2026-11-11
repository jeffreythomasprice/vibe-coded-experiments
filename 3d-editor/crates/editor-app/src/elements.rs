use std::collections::BTreeSet;

use editor_cmd::args::BrushVertexAt;
use editor_core::{BrushId, Document, FaceId, ObjectId};
use editor_geom::brush_geometry;
use editor_geom::tolerance::WELD_EPS;
use glam::DVec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Vertex,
    Edge,
    Face,
}

/// A grab point on a brush: dragging it moves every corner in `corners`.
#[derive(Debug, Clone)]
pub struct ElementHandle {
    pub pos: DVec3,
    pub corners: Vec<BrushVertexAt>,
    /// What lights up with the handle: the edge, or the face's boundary.
    pub outline: Vec<(DVec3, DVec3)>,
}

/// The vertex, edge or face handles of the selected brushes (including group and entity members).
pub fn handles(doc: &Document, selection: &BTreeSet<ObjectId>, kind: Kind) -> Vec<ElementHandle> {
    let mut out = Vec::new();
    for id in doc.with_members(selection) {
        let ObjectId::Brush(b) = id else { continue };
        let Some(obj) = doc.brushes.get(&b) else {
            continue;
        };
        let Ok(geo) = brush_geometry(&obj.brush) else {
            continue;
        };
        let at = |i: usize| BrushVertexAt {
            brush: b,
            at: geo.vertices[i],
        };
        match kind {
            Kind::Vertex => {
                out.extend((0..geo.vertices.len()).map(|i| ElementHandle {
                    pos: geo.vertices[i],
                    corners: vec![at(i)],
                    outline: Vec::new(),
                }));
            }
            Kind::Edge => {
                for face in &geo.faces {
                    for (k, &i) in face.loop_.iter().enumerate() {
                        let j = face.loop_[(k + 1) % face.loop_.len()];
                        if i < j {
                            let (p, q) = (geo.vertices[i], geo.vertices[j]);
                            out.push(ElementHandle {
                                pos: (p + q) / 2.0,
                                corners: vec![at(i), at(j)],
                                outline: vec![(p, q)],
                            });
                        }
                    }
                }
            }
            Kind::Face => {
                for face in &geo.faces {
                    out.push(face_handle_of(&geo.vertices, &face.loop_, b));
                }
            }
        }
    }
    out
}

fn face_handle_of(vertices: &[DVec3], loop_: &[usize], brush: BrushId) -> ElementHandle {
    let corners: Vec<BrushVertexAt> = loop_
        .iter()
        .map(|&i| BrushVertexAt {
            brush,
            at: vertices[i],
        })
        .collect();
    let pos = loop_.iter().map(|&i| vertices[i]).sum::<DVec3>() / loop_.len() as f64;
    let outline = (0..loop_.len())
        .map(|k| (vertices[loop_[k]], vertices[loop_[(k + 1) % loop_.len()]]))
        .collect();
    ElementHandle {
        pos,
        corners,
        outline,
    }
}

/// The handle for one face of one brush, for grabbing a face by clicking on it.
pub fn face_handle(doc: &Document, brush: BrushId, face: FaceId) -> Option<ElementHandle> {
    let geo = brush_geometry(&doc.brushes.get(&brush)?.brush).ok()?;
    let g = geo.faces.iter().find(|g| g.face == face)?;
    Some(face_handle_of(&geo.vertices, &g.loop_, brush))
}

/// Every corner of `handles`, once.
pub fn merged_corners<'a>(
    handles: impl IntoIterator<Item = &'a ElementHandle>,
) -> Vec<BrushVertexAt> {
    let mut out: Vec<BrushVertexAt> = Vec::new();
    for c in handles.into_iter().flat_map(|h| &h.corners) {
        if !out
            .iter()
            .any(|o| o.brush == c.brush && (o.at - c.at).length() <= WELD_EPS)
        {
            out.push(c.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use editor_core::{BrushObject, Extra, Flags, LayerId};
    use editor_geom::shapes::cuboid;

    fn doc_with_box() -> (Document, BTreeSet<ObjectId>) {
        let mut doc = Document::empty("t", 10);
        let id = BrushId(1);
        doc.brushes.insert(
            id,
            BrushObject {
                id,
                layer: LayerId(1),
                group: None,
                entity: None,
                brush: cuboid(DVec3::ZERO, DVec3::splat(8.0)),
                extra: Extra::new(),
                flags: Flags::default(),
            },
        );
        (doc, BTreeSet::from([ObjectId::Brush(id)]))
    }

    #[test]
    fn a_box_has_eight_corners_twelve_edges_and_six_faces() {
        let (doc, sel) = doc_with_box();
        assert_eq!(handles(&doc, &sel, Kind::Vertex).len(), 8);
        let edges = handles(&doc, &sel, Kind::Edge);
        assert_eq!(edges.len(), 12);
        assert!(
            edges
                .iter()
                .all(|e| e.corners.len() == 2 && e.outline.len() == 1)
        );
        let faces = handles(&doc, &sel, Kind::Face);
        assert_eq!(faces.len(), 6);
        assert!(
            faces
                .iter()
                .all(|f| f.corners.len() == 4 && f.outline.len() == 4)
        );
    }

    #[test]
    fn handles_sit_at_the_middle_of_what_they_move() {
        let (doc, sel) = doc_with_box();
        let top = handles(&doc, &sel, Kind::Face)
            .into_iter()
            .find(|f| (f.pos.z - 8.0).abs() < 1e-9)
            .unwrap();
        assert_eq!(top.pos, DVec3::new(4.0, 4.0, 8.0));
    }

    #[test]
    fn merging_shared_corners_counts_each_once() {
        let (doc, sel) = doc_with_box();
        let faces = handles(&doc, &sel, Kind::Face);
        assert_eq!(merged_corners(&faces).len(), 8);
    }

    #[test]
    fn a_face_can_be_looked_up_by_id() {
        let (doc, _) = doc_with_box();
        let face = doc.brushes[&BrushId(1)].brush.faces[0].id;
        let h = face_handle(&doc, BrushId(1), face).unwrap();
        assert_eq!(h.corners.len(), 4);
        assert!(face_handle(&doc, BrushId(2), face).is_none());
    }
}

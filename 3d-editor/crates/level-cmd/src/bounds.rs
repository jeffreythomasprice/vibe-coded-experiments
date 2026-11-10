use level_core::{Document, Object, ObjectId};
use level_geom::{Aabb, brush_geometry};
use serde_json::{Value, json};

pub fn object_bounds(object: &Object) -> Option<Aabb> {
    match object {
        Object::Brush(b) => brush_geometry(&b.brush)
            .ok()
            .and_then(|g| Aabb::from_points(g.vertices)),
        Object::Mesh(m) => Aabb::from_points(m.mesh.vertices.iter().map(|v| v.pos)),
        Object::Entity(e) => e.origin.and_then(|o| Aabb::from_points([o])),
        Object::Camera(c) => Aabb::from_points([c.position]),
        Object::Group(_) | Object::Layer(_) => None,
    }
}

/// Like `object_bounds`, but a group, brush entity or layer covers its members.
pub fn extent(doc: &Document, id: ObjectId) -> Option<Aabb> {
    let mut members = std::collections::BTreeSet::new();
    match id {
        ObjectId::Layer(_) => {
            for child in doc.children(id) {
                crate::commands::util::descendants(doc, child, &mut members);
            }
        }
        _ => crate::commands::util::descendants(doc, id, &mut members),
    }
    members
        .into_iter()
        .filter(|m| *m == id || !matches!(m, ObjectId::Camera(_)))
        .filter_map(|m| doc.get(m))
        .filter_map(|o| object_bounds(&o))
        .reduce(Aabb::merge)
}

/// Bounds of everything except cameras.
pub fn scene_bounds(doc: &Document) -> Option<Aabb> {
    doc.all_ids()
        .into_iter()
        .filter(|id| !matches!(id, ObjectId::Camera(_)))
        .filter_map(|id| doc.get(id))
        .filter_map(|o| object_bounds(&o))
        .reduce(Aabb::merge)
}

pub fn bounds_json(b: Aabb) -> Value {
    json!({ "min": b.min, "max": b.max })
}

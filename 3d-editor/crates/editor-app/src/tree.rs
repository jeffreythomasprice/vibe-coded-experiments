use std::collections::BTreeMap;

use editor_core::{Document, ObjectId};

/// The outliner's view of a document: layers at the top, then groups, brush entities and their contents.
/// An object sits under its entity if it has one, else its group, else its layer.
#[derive(Debug, Default)]
pub struct Tree {
    children: BTreeMap<ObjectId, Vec<ObjectId>>,
    parents: BTreeMap<ObjectId, ObjectId>,
    layers: Vec<ObjectId>,
}

impl Tree {
    pub fn new(doc: &Document) -> Tree {
        let mut tree = Tree {
            layers: doc.layers.keys().map(|&l| ObjectId::Layer(l)).collect(),
            ..Tree::default()
        };
        for g in doc.groups.values() {
            let parent = g.parent.map_or(ObjectId::Layer(g.layer), ObjectId::Group);
            tree.add(parent, ObjectId::Group(g.id));
        }
        for e in doc.entities.values() {
            let parent = e.group.map_or(ObjectId::Layer(e.layer), ObjectId::Group);
            tree.add(parent, ObjectId::Entity(e.id));
        }
        for b in doc.brushes.values() {
            tree.add(owner(b.entity, b.group, b.layer), ObjectId::Brush(b.id));
        }
        for m in doc.meshes.values() {
            tree.add(owner(m.entity, m.group, m.layer), ObjectId::Mesh(m.id));
        }
        tree
    }

    fn add(&mut self, parent: ObjectId, child: ObjectId) {
        self.children.entry(parent).or_default().push(child);
        self.parents.insert(child, parent);
    }

    pub fn layers(&self) -> &[ObjectId] {
        &self.layers
    }

    pub fn children(&self, id: ObjectId) -> &[ObjectId] {
        self.children.get(&id).map_or(&[], Vec::as_slice)
    }

    /// Parents from the nearest up to the layer.
    pub fn ancestors(&self, id: ObjectId) -> Vec<ObjectId> {
        let mut out = Vec::new();
        let mut at = id;
        while let Some(&parent) = self.parents.get(&at) {
            out.push(parent);
            at = parent;
        }
        out
    }
}

fn owner(
    entity: Option<editor_core::EntityId>,
    group: Option<editor_core::GroupId>,
    layer: editor_core::LayerId,
) -> ObjectId {
    entity
        .map(ObjectId::Entity)
        .or(group.map(ObjectId::Group))
        .unwrap_or(ObjectId::Layer(layer))
}

#[cfg(test)]
mod tests {
    use editor_cmd::Session;
    use serde_json::{Value, json};

    use super::*;

    fn run(s: &mut Session, op: &str, args: Value) -> Value {
        let v = s
            .execute_value(json!({ "op": op, "args": args }), "test")
            .to_value();
        assert_eq!(v["ok"], true, "{op}: {v}");
        v
    }

    fn id(v: &Value) -> ObjectId {
        v["result"]["id"].as_str().unwrap().parse().unwrap()
    }

    fn make_box(s: &mut Session, at: i32) -> ObjectId {
        id(&run(
            s,
            "brush.create_box",
            json!({ "min": [at, 0, 0], "max": [at + 8, 8, 8] }),
        ))
    }

    #[test]
    fn objects_hang_under_group_entity_or_layer() {
        let mut s = Session::new(Document::new("t"));
        let loose = make_box(&mut s, 0);
        let grouped = make_box(&mut s, 16);
        let owned = make_box(&mut s, 32);
        let group = id(&run(
            &mut s,
            "group.create",
            json!({ "objects": [grouped.to_string()] }),
        ));
        let entity = id(&run(
            &mut s,
            "entity.create_brush_entity",
            json!({ "classname": "func_door", "objects": [owned.to_string()] }),
        ));

        let tree = Tree::new(s.document());
        let layer = tree.layers()[0];
        assert_eq!(tree.layers().len(), 1);
        assert!(tree.children(layer).contains(&loose));
        assert!(tree.children(layer).contains(&group));
        assert!(tree.children(layer).contains(&entity));
        assert_eq!(tree.children(group), [grouped]);
        assert_eq!(tree.children(entity), [owned]);
        assert!(!tree.children(layer).contains(&grouped));
        assert!(!tree.children(layer).contains(&owned));
    }

    #[test]
    fn ancestors_run_from_the_nearest_to_the_layer() {
        let mut s = Session::new(Document::new("t"));
        let brush = make_box(&mut s, 0);
        let inner = id(&run(
            &mut s,
            "group.create",
            json!({ "objects": [brush.to_string()] }),
        ));
        let outer = id(&run(
            &mut s,
            "group.create",
            json!({ "objects": [inner.to_string()] }),
        ));

        let tree = Tree::new(s.document());
        assert_eq!(tree.ancestors(brush), [inner, outer, tree.layers()[0]]);
        assert!(tree.ancestors(tree.layers()[0]).is_empty());
    }
}

use std::collections::BTreeMap;

use crate::{
    BrushId, BrushObject, Camera, CameraId, Entity, EntityId, Extra, Flags, Group, GroupId, Layer,
    LayerId, MeshId, MeshObject, Object, ObjectId, Properties,
};

#[derive(Debug, Clone, PartialEq)]
pub struct EditorSettings {
    pub grid: f64,
    pub extra: Extra,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            grid: 16.0,
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub name: String,
    next_id: u64,
    pub meta_extra: Extra,
    pub world: Properties,
    pub editor: EditorSettings,
    pub layers: BTreeMap<LayerId, Layer>,
    pub groups: BTreeMap<GroupId, Group>,
    pub cameras: BTreeMap<CameraId, Camera>,
    pub entities: BTreeMap<EntityId, Entity>,
    pub brushes: BTreeMap<BrushId, BrushObject>,
    pub meshes: BTreeMap<MeshId, MeshObject>,
}

impl Document {
    pub fn empty(name: impl Into<String>, next_id: u64) -> Self {
        Self {
            name: name.into(),
            next_id,
            meta_extra: Extra::new(),
            world: Properties::new(),
            editor: EditorSettings::default(),
            layers: BTreeMap::new(),
            groups: BTreeMap::new(),
            cameras: BTreeMap::new(),
            entities: BTreeMap::new(),
            brushes: BTreeMap::new(),
            meshes: BTreeMap::new(),
        }
    }

    pub fn new(name: impl Into<String>) -> Self {
        let mut doc = Self::empty(name, 1);
        let id = LayerId(doc.alloc_id());
        doc.layers.insert(
            id,
            Layer {
                flags: Default::default(),
                omit_from_export: false,
                id,
                name: "main".into(),
                extra: Extra::new(),
            },
        );
        doc
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    pub fn set_next_id(&mut self, next_id: u64) {
        self.next_id = next_id;
    }

    pub fn get(&self, id: ObjectId) -> Option<Object> {
        match id {
            ObjectId::Brush(i) => self.brushes.get(&i).cloned().map(Object::Brush),
            ObjectId::Mesh(i) => self.meshes.get(&i).cloned().map(Object::Mesh),
            ObjectId::Entity(i) => self.entities.get(&i).cloned().map(Object::Entity),
            ObjectId::Group(i) => self.groups.get(&i).cloned().map(Object::Group),
            ObjectId::Layer(i) => self.layers.get(&i).cloned().map(Object::Layer),
            ObjectId::Camera(i) => self.cameras.get(&i).cloned().map(Object::Camera),
        }
    }

    pub fn contains(&self, id: ObjectId) -> bool {
        match id {
            ObjectId::Brush(i) => self.brushes.contains_key(&i),
            ObjectId::Mesh(i) => self.meshes.contains_key(&i),
            ObjectId::Entity(i) => self.entities.contains_key(&i),
            ObjectId::Group(i) => self.groups.contains_key(&i),
            ObjectId::Layer(i) => self.layers.contains_key(&i),
            ObjectId::Camera(i) => self.cameras.contains_key(&i),
        }
    }

    pub fn insert(&mut self, object: Object) -> Option<Object> {
        match object {
            Object::Brush(o) => self.brushes.insert(o.id, o).map(Object::Brush),
            Object::Mesh(o) => self.meshes.insert(o.id, o).map(Object::Mesh),
            Object::Entity(o) => self.entities.insert(o.id, o).map(Object::Entity),
            Object::Group(o) => self.groups.insert(o.id, o).map(Object::Group),
            Object::Layer(o) => self.layers.insert(o.id, o).map(Object::Layer),
            Object::Camera(o) => self.cameras.insert(o.id, o).map(Object::Camera),
        }
    }

    pub fn remove(&mut self, id: ObjectId) -> Option<Object> {
        match id {
            ObjectId::Brush(i) => self.brushes.remove(&i).map(Object::Brush),
            ObjectId::Mesh(i) => self.meshes.remove(&i).map(Object::Mesh),
            ObjectId::Entity(i) => self.entities.remove(&i).map(Object::Entity),
            ObjectId::Group(i) => self.groups.remove(&i).map(Object::Group),
            ObjectId::Layer(i) => self.layers.remove(&i).map(Object::Layer),
            ObjectId::Camera(i) => self.cameras.remove(&i).map(Object::Camera),
        }
    }

    pub fn all_ids(&self) -> Vec<ObjectId> {
        let mut ids: Vec<ObjectId> = Vec::new();
        ids.extend(self.layers.keys().map(|&i| ObjectId::from(i)));
        ids.extend(self.groups.keys().map(|&i| ObjectId::from(i)));
        ids.extend(self.cameras.keys().map(|&i| ObjectId::from(i)));
        ids.extend(self.entities.keys().map(|&i| ObjectId::from(i)));
        ids.extend(self.brushes.keys().map(|&i| ObjectId::from(i)));
        ids.extend(self.meshes.keys().map(|&i| ObjectId::from(i)));
        ids.sort_by_key(|i| i.number());
        ids
    }

    pub fn len(&self) -> usize {
        self.layers.len()
            + self.groups.len()
            + self.cameras.len()
            + self.entities.len()
            + self.brushes.len()
            + self.meshes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn links(&self, id: ObjectId) -> Option<(Option<EntityId>, Option<GroupId>, Option<LayerId>)> {
        Some(match id {
            ObjectId::Brush(i) => {
                let o = self.brushes.get(&i)?;
                (o.entity, o.group, Some(o.layer))
            }
            ObjectId::Mesh(i) => {
                let o = self.meshes.get(&i)?;
                (o.entity, o.group, Some(o.layer))
            }
            ObjectId::Entity(i) => {
                let o = self.entities.get(&i)?;
                (None, o.group, Some(o.layer))
            }
            ObjectId::Group(i) => {
                let o = self.groups.get(&i)?;
                (None, o.parent, Some(o.layer))
            }
            ObjectId::Layer(_) | ObjectId::Camera(_) => (None, None, None),
        })
    }

    /// The hidden and locked flags of `id` alone; cameras have none.
    pub fn own_flags(&self, id: ObjectId) -> Option<Flags> {
        match id {
            ObjectId::Brush(i) => self.brushes.get(&i).map(|o| o.flags),
            ObjectId::Mesh(i) => self.meshes.get(&i).map(|o| o.flags),
            ObjectId::Entity(i) => self.entities.get(&i).map(|o| o.flags),
            ObjectId::Group(i) => self.groups.get(&i).map(|o| o.flags),
            ObjectId::Layer(i) => self.layers.get(&i).map(|o| o.flags),
            ObjectId::Camera(_) => None,
        }
    }

    /// What contains `id`: its brush entity, its group chain and its layer.
    pub fn ancestors(&self, id: ObjectId) -> Vec<ObjectId> {
        let mut out = Vec::new();
        let Some((entity, mut group, mut layer)) = self.links(id) else {
            return out;
        };
        if let Some(e) = entity {
            out.push(e.into());
            if let Some((_, g, l)) = self.links(e.into()) {
                group = group.or(g);
                layer = layer.or(l);
            }
        }
        let mut guard = self.groups.len() + 1;
        while let Some(g) = group {
            if guard == 0 {
                break;
            }
            guard -= 1;
            out.push(g.into());
            let (_, parent, l) = self.links(g.into()).unwrap_or((None, None, None));
            layer = layer.or(l);
            group = parent;
        }
        out.extend(layer.map(ObjectId::from));
        out
    }

    /// Hidden or locked because the object or something containing it is.
    pub fn effective_flags(&self, id: ObjectId) -> Flags {
        std::iter::once(id)
            .chain(self.ancestors(id))
            .filter_map(|i| self.own_flags(i))
            .fold(Flags::default(), |acc, f| Flags {
                hidden: acc.hidden || f.hidden,
                locked: acc.locked || f.locked,
            })
    }

    /// Whether a click can land on `id`: it is not locked, and lies inside the open group if one is open.
    pub fn pickable(&self, id: ObjectId, open: Option<GroupId>) -> bool {
        if self.effective_locked(id) {
            return false;
        }
        open.is_none_or(|g| self.ancestors(id).contains(&g.into()))
    }

    /// What a click on `id` selects: the outermost group around it that is not open, else `id` itself.
    pub fn pick_target(&self, id: ObjectId, open: Option<GroupId>) -> ObjectId {
        let opened: Vec<ObjectId> = open
            .into_iter()
            .flat_map(|g| std::iter::once(ObjectId::from(g)).chain(self.ancestors(g.into())))
            .collect();
        self.ancestors(id)
            .into_iter()
            .rev()
            .find(|a| matches!(a, ObjectId::Group(_)) && !opened.contains(a))
            .unwrap_or(id)
    }

    /// `ids` and everything below them: group members and brush entity members.
    pub fn with_members(
        &self,
        ids: &std::collections::BTreeSet<ObjectId>,
    ) -> std::collections::BTreeSet<ObjectId> {
        fn walk(doc: &Document, id: ObjectId, out: &mut std::collections::BTreeSet<ObjectId>) {
            if out.insert(id) && matches!(id, ObjectId::Group(_) | ObjectId::Entity(_)) {
                for child in doc.children(id) {
                    walk(doc, child, out);
                }
            }
        }
        let mut out = std::collections::BTreeSet::new();
        for &id in ids {
            walk(self, id, &mut out);
        }
        out
    }

    /// Every object that is hidden, directly or through what contains it.
    pub fn hidden_ids(&self) -> std::collections::BTreeSet<ObjectId> {
        self.all_ids()
            .into_iter()
            .filter(|&id| self.effective_hidden(id))
            .collect()
    }

    /// Every object that is locked, directly or through what contains it.
    pub fn locked_ids(&self) -> std::collections::BTreeSet<ObjectId> {
        self.all_ids()
            .into_iter()
            .filter(|&id| self.effective_locked(id))
            .collect()
    }

    pub fn effective_hidden(&self, id: ObjectId) -> bool {
        self.effective_flags(id).hidden
    }

    pub fn effective_locked(&self, id: ObjectId) -> bool {
        self.effective_flags(id).locked
    }

    /// Direct children: objects whose parent pointer names `id`.
    pub fn children(&self, id: ObjectId) -> Vec<ObjectId> {
        let mut out = Vec::new();
        match id {
            ObjectId::Layer(l) => {
                out.extend(
                    self.groups
                        .values()
                        .filter(|g| g.layer == l)
                        .map(|g| ObjectId::from(g.id)),
                );
                out.extend(
                    self.entities
                        .values()
                        .filter(|e| e.layer == l)
                        .map(|e| ObjectId::from(e.id)),
                );
                out.extend(
                    self.brushes
                        .values()
                        .filter(|b| b.layer == l)
                        .map(|b| ObjectId::from(b.id)),
                );
                out.extend(
                    self.meshes
                        .values()
                        .filter(|m| m.layer == l)
                        .map(|m| ObjectId::from(m.id)),
                );
            }
            ObjectId::Group(g) => {
                out.extend(
                    self.groups
                        .values()
                        .filter(|x| x.parent == Some(g))
                        .map(|x| ObjectId::from(x.id)),
                );
                out.extend(
                    self.entities
                        .values()
                        .filter(|x| x.group == Some(g))
                        .map(|x| ObjectId::from(x.id)),
                );
                out.extend(
                    self.brushes
                        .values()
                        .filter(|x| x.group == Some(g))
                        .map(|x| ObjectId::from(x.id)),
                );
                out.extend(
                    self.meshes
                        .values()
                        .filter(|x| x.group == Some(g))
                        .map(|x| ObjectId::from(x.id)),
                );
            }
            ObjectId::Entity(e) => {
                out.extend(
                    self.brushes
                        .values()
                        .filter(|x| x.entity == Some(e))
                        .map(|x| ObjectId::from(x.id)),
                );
                out.extend(
                    self.meshes
                        .values()
                        .filter(|x| x.entity == Some(e))
                        .map(|x| ObjectId::from(x.id)),
                );
            }
            _ => {}
        }
        out.sort_by_key(|i| i.number());
        out
    }
}

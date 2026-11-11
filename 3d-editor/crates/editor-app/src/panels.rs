use std::collections::BTreeSet;

use editor_core::{Document, LayerId, Object, ObjectId, ObjectKind, Properties, PropertyValue};
use editor_render::material_rgb8;
use egui::collapsing_header::CollapsingState;
use egui::{Align, Color32, ComboBox, DragValue, Sense, Ui, vec2};
use glam::DVec3;

use crate::tree::Tree;

/// What a panel asks the editor to do. Panels only read the document; the editor turns these into ops.
#[derive(Debug, Clone)]
pub enum Event {
    Select {
        id: ObjectId,
        additive: bool,
    },
    Frame(ObjectId),
    SetHidden {
        id: ObjectId,
        hidden: bool,
    },
    SetLocked {
        id: ObjectId,
        locked: bool,
    },
    Rename {
        id: ObjectId,
        name: String,
    },
    SetLayer {
        ids: Vec<ObjectId>,
        layer: LayerId,
    },
    SetClassname {
        id: ObjectId,
        classname: String,
    },
    SetTransform {
        id: ObjectId,
        origin: Option<DVec3>,
        angles: Option<DVec3>,
        merge: Option<String>,
    },
    SetProperty {
        id: ObjectId,
        key: String,
        value: PropertyValue,
        merge: Option<String>,
    },
    RemoveProperty {
        id: ObjectId,
        key: String,
    },
    SetMaterial {
        ids: Vec<ObjectId>,
        material: String,
    },
}

#[derive(Default)]
pub struct Panels {
    tree: Option<(u64, Tree)>,
    materials: Option<(u64, Vec<String>)>,
    seen_selection: BTreeSet<ObjectId>,
    reveal: Option<ObjectId>,
    clicked_here: bool,
    drag_serial: u64,
    drag_key: String,
    new_material: String,
    new_key: String,
    new_value: String,
}

fn label(doc: &Document, id: ObjectId) -> String {
    match id {
        ObjectId::Layer(l) => doc
            .layers
            .get(&l)
            .map_or_else(|| id.to_string(), |x| x.name.clone()),
        ObjectId::Group(g) => doc
            .groups
            .get(&g)
            .map_or_else(|| id.to_string(), |x| x.name.clone()),
        ObjectId::Entity(e) => doc
            .entities
            .get(&e)
            .map_or_else(|| id.to_string(), |x| format!("{}  {id}", x.classname)),
        ObjectId::Brush(_) => format!("brush  {id}"),
        ObjectId::Mesh(_) => format!("mesh  {id}"),
        ObjectId::Camera(_) => id.to_string(),
    }
}

fn parse_value(text: &str) -> PropertyValue {
    let text = text.trim();
    match text {
        "true" => return PropertyValue::Bool(true),
        "false" => return PropertyValue::Bool(false),
        _ => {}
    }
    if let Ok(n) = text.parse::<f64>() {
        return PropertyValue::Number(n);
    }
    let parts: Vec<f64> = text
        .split_whitespace()
        .filter_map(|p| p.parse().ok())
        .collect();
    if let [x, y, z] = parts[..]
        && text.split_whitespace().count() == 3
    {
        return PropertyValue::Vec3(DVec3::new(x, y, z));
    }
    PropertyValue::String(text.to_string())
}

/// A single-line text field that reports its new text when edited and left, and drops the edit on Escape.
fn text_field(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
) -> Option<String> {
    let id = ui.make_persistent_id(salt);
    let mut draft: String = ui
        .data_mut(|d| d.get_temp(id))
        .unwrap_or_else(|| current.to_string());
    let response = ui
        .add(egui::TextEdit::singleline(&mut draft).desired_width(ui.available_width().min(240.0)));
    if response.has_focus() {
        ui.data_mut(|d| d.insert_temp(id, draft.clone()));
        return None;
    }
    ui.data_mut(|d| d.remove::<String>(id));
    let cancelled = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape));
    (response.lost_focus() && !cancelled && draft != current).then_some(draft)
}

fn kind_name(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Brush => "brush",
        ObjectKind::Mesh => "mesh",
        ObjectKind::Entity => "entity",
        ObjectKind::Group => "group",
        ObjectKind::Layer => "layer",
        ObjectKind::Camera => "camera",
    }
}

fn face_materials(object: &Object) -> Vec<&str> {
    match object {
        Object::Brush(b) => b
            .brush
            .faces
            .iter()
            .map(|f| f.attrs.material.as_str())
            .collect(),
        Object::Mesh(m) => m
            .mesh
            .faces
            .iter()
            .map(|f| f.attrs.material.as_str())
            .collect(),
        _ => Vec::new(),
    }
}

fn document_materials(doc: &Document) -> Vec<String> {
    let mut names = BTreeSet::new();
    for b in doc.brushes.values() {
        names.extend(b.brush.faces.iter().map(|f| f.attrs.material.clone()));
    }
    for m in doc.meshes.values() {
        names.extend(m.mesh.faces.iter().map(|f| f.attrs.material.clone()));
    }
    names.into_iter().collect()
}

impl Panels {
    fn tree<'a>(&'a mut self, doc: &Document, revision: u64) -> &'a Tree {
        if self.tree.as_ref().is_none_or(|(rev, _)| *rev != revision) {
            self.tree = Some((revision, Tree::new(doc)));
        }
        &self.tree.as_ref().expect("just built").1
    }

    pub fn outliner(
        &mut self,
        ui: &mut Ui,
        doc: &Document,
        revision: u64,
        selection: &BTreeSet<ObjectId>,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        if *selection != self.seen_selection {
            let first = selection.iter().next().copied();
            self.reveal = first.filter(|_| !self.clicked_here);
            if let Some(id) = first {
                let ancestors = self.tree(doc, revision).ancestors(id);
                for ancestor in ancestors {
                    let state_id = ui.make_persistent_id(("outline", ancestor));
                    let mut state =
                        CollapsingState::load_with_default_open(ui.ctx(), state_id, false);
                    state.set_open(true);
                    state.store(ui.ctx());
                }
            }
            self.seen_selection = selection.clone();
            self.clicked_here = false;
        }
        let layers = self.tree(doc, revision).layers().to_vec();
        let reveal = self.reveal;
        let mut clicked = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for layer in layers {
                self.node(
                    ui,
                    doc,
                    revision,
                    selection,
                    layer,
                    reveal,
                    &mut events,
                    &mut clicked,
                );
            }
        });
        if reveal.is_some() {
            self.reveal = None;
        }
        self.clicked_here |= clicked;
        events
    }

    #[allow(clippy::too_many_arguments)]
    fn node(
        &mut self,
        ui: &mut Ui,
        doc: &Document,
        revision: u64,
        selection: &BTreeSet<ObjectId>,
        id: ObjectId,
        reveal: Option<ObjectId>,
        events: &mut Vec<Event>,
        clicked: &mut bool,
    ) {
        let children = self.tree(doc, revision).children(id).to_vec();
        let selected = selection.contains(&id);
        let text = label(doc, id);
        let own = doc.own_flags(id).unwrap_or_default();
        let effective = doc.effective_flags(id);
        let mut row = |ui: &mut Ui| {
            let mut name = egui::RichText::new(text.clone());
            if effective.hidden {
                name = name.weak();
            }
            if effective.locked {
                name = name.italics();
            }
            let response = ui.selectable_label(selected, name);
            if reveal == Some(id) {
                response.scroll_to_me(Some(Align::Center));
            }
            if response.clicked() {
                *clicked = true;
                events.push(Event::Select {
                    id,
                    additive: ui.input(|i| i.modifiers.shift),
                });
            }
            if response.double_clicked() {
                events.push(Event::Frame(id));
            }
            let mut hidden = own.hidden;
            if ui
                .toggle_value(&mut hidden, egui::RichText::new("H").small().monospace())
                .on_hover_text("hidden")
                .changed()
            {
                events.push(Event::SetHidden { id, hidden });
            }
            let mut locked = own.locked;
            if ui
                .toggle_value(&mut locked, egui::RichText::new("L").small().monospace())
                .on_hover_text("locked")
                .changed()
            {
                events.push(Event::SetLocked { id, locked });
            }
        };
        if children.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(ui.spacing().indent);
                row(ui);
            });
            return;
        }
        let state_id = ui.make_persistent_id(("outline", id));
        CollapsingState::load_with_default_open(
            ui.ctx(),
            state_id,
            matches!(id, ObjectId::Layer(_)),
        )
        .show_header(ui, |ui| row(ui))
        .body(|ui| {
            for child in children {
                self.node(ui, doc, revision, selection, child, reveal, events, clicked);
            }
        });
    }

    pub fn inspector(
        &mut self,
        ui: &mut Ui,
        doc: &Document,
        revision: u64,
        selection: &BTreeSet<ObjectId>,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let ids: Vec<ObjectId> = selection.iter().copied().collect();
        egui::ScrollArea::vertical().show(ui, |ui| {
            match ids.as_slice() {
                [] => {
                    ui.weak("Nothing selected");
                }
                [id] => self.single(ui, doc, *id, &mut events),
                many => Self::several(ui, doc, many, &mut events),
            }
            ui.add_space(8.0);
            self.materials(ui, doc, revision, &ids, &mut events);
        });
        events
    }

    fn number(&mut self, ui: &mut Ui, value: &mut f64, speed: f64) -> Option<String> {
        let response = ui.add(DragValue::new(value).speed(speed));
        if response.drag_started() || response.gained_focus() {
            self.drag_serial += 1;
            self.drag_key = format!("inspect-{}", self.drag_serial);
        }
        response.changed().then(|| self.drag_key.clone())
    }

    fn vec3(&mut self, ui: &mut Ui, value: DVec3, speed: f64) -> Option<(DVec3, String)> {
        let mut next = value;
        let mut key = None;
        ui.horizontal(|ui| {
            for i in 0..3 {
                let mut v = next[i];
                if let Some(k) = self.number(ui, &mut v, speed) {
                    next[i] = v;
                    key = Some(k);
                }
            }
        });
        key.map(|k| (next, k))
    }

    fn layer_combo(ui: &mut Ui, doc: &Document, ids: &[ObjectId], events: &mut Vec<Event>) {
        let layers: BTreeSet<Option<LayerId>> = ids
            .iter()
            .filter_map(|&id| doc.get(id))
            .filter(|o| !matches!(o, Object::Layer(_) | Object::Camera(_)))
            .map(|o| o.layer())
            .collect();
        let current = match layers.iter().collect::<Vec<_>>().as_slice() {
            [Some(l)] => doc.layers.get(l).map(|x| x.name.clone()),
            _ => None,
        };
        if layers.is_empty() {
            return;
        }
        ui.horizontal(|ui| {
            ui.weak("layer");
            ComboBox::from_id_salt("inspector-layer")
                .selected_text(current.clone().unwrap_or_else(|| "—".into()))
                .show_ui(ui, |ui| {
                    for (id, layer) in &doc.layers {
                        let is_current = current.as_deref() == Some(layer.name.as_str());
                        if ui.selectable_label(is_current, &layer.name).clicked() && !is_current {
                            events.push(Event::SetLayer {
                                ids: ids.to_vec(),
                                layer: *id,
                            });
                        }
                    }
                });
        });
    }

    fn single(&mut self, ui: &mut Ui, doc: &Document, id: ObjectId, events: &mut Vec<Event>) {
        let Some(object) = doc.get(id) else { return };
        ui.strong(format!("{}  {id}", kind_name(id.kind())));
        let name = match &object {
            Object::Group(g) => Some(g.name.clone()),
            Object::Layer(l) => Some(l.name.clone()),
            Object::Camera(c) => Some(c.name.clone()),
            _ => None,
        };
        if let Some(name) = name {
            ui.horizontal(|ui| {
                ui.weak("name");
                if let Some(name) = text_field(ui, ("name", id), &name) {
                    events.push(Event::Rename { id, name });
                }
            });
        }
        Self::layer_combo(ui, doc, &[id], events);
        if let Some(b) = editor_cmd::object_bounds(&object) {
            egui::Grid::new("bounds").num_columns(2).show(ui, |ui| {
                let s = b.size();
                for (name, v) in [("min", b.min), ("max", b.max), ("size", s)] {
                    ui.weak(name);
                    ui.monospace(format!("{:.0}, {:.0}, {:.0}", v.x, v.y, v.z));
                    ui.end_row();
                }
            });
        }
        let faces = face_materials(&object).len();
        if faces > 0 {
            ui.weak(format!("{faces} faces"));
        }
        if let Object::Entity(e) = &object {
            self.entity(ui, id, e, events);
        }
    }

    fn entity(
        &mut self,
        ui: &mut Ui,
        id: ObjectId,
        e: &editor_core::Entity,
        events: &mut Vec<Event>,
    ) {
        ui.horizontal(|ui| {
            ui.weak("class");
            if let Some(classname) = text_field(ui, ("classname", id), &e.classname) {
                events.push(Event::SetClassname { id, classname });
            }
        });
        if let Some(origin) = e.origin {
            ui.weak("origin");
            if let Some((origin, key)) = self.vec3(ui, origin, 1.0) {
                events.push(Event::SetTransform {
                    id,
                    origin: Some(origin),
                    angles: None,
                    merge: Some(key),
                });
            }
            ui.weak("angles (pitch yaw roll)");
            if let Some((angles, key)) = self.vec3(ui, e.angles, 1.0) {
                events.push(Event::SetTransform {
                    id,
                    origin: None,
                    angles: Some(angles),
                    merge: Some(key),
                });
            }
        }
        ui.add_space(4.0);
        ui.weak("properties");
        self.properties(ui, id, &e.properties, events);
    }

    fn properties(
        &mut self,
        ui: &mut Ui,
        id: ObjectId,
        properties: &Properties,
        events: &mut Vec<Event>,
    ) {
        for (key, value) in properties {
            ui.horizontal(|ui| {
                ui.monospace(key);
                match value {
                    PropertyValue::Bool(b) => {
                        let mut next = *b;
                        if ui.checkbox(&mut next, "").changed() {
                            events.push(Event::SetProperty {
                                id,
                                key: key.clone(),
                                value: PropertyValue::Bool(next),
                                merge: None,
                            });
                        }
                    }
                    PropertyValue::Number(n) => {
                        let mut next = *n;
                        if let Some(merge) = self.number(ui, &mut next, 0.1) {
                            events.push(Event::SetProperty {
                                id,
                                key: key.clone(),
                                value: PropertyValue::Number(next),
                                merge: Some(merge),
                            });
                        }
                    }
                    PropertyValue::Vec3(v) => {
                        if let Some((next, merge)) = self.vec3(ui, *v, 0.1) {
                            events.push(Event::SetProperty {
                                id,
                                key: key.clone(),
                                value: PropertyValue::Vec3(next),
                                merge: Some(merge),
                            });
                        }
                    }
                    PropertyValue::String(s) => {
                        if let Some(next) = text_field(ui, ("property", id, key), s) {
                            events.push(Event::SetProperty {
                                id,
                                key: key.clone(),
                                value: PropertyValue::String(next),
                                merge: None,
                            });
                        }
                    }
                }
                if ui.small_button("×").on_hover_text("Remove").clicked() {
                    events.push(Event::RemoveProperty {
                        id,
                        key: key.clone(),
                    });
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.new_key)
                    .desired_width(70.0)
                    .hint_text("key"),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.new_value)
                    .desired_width(70.0)
                    .hint_text("value"),
            );
            let ready = !self.new_key.trim().is_empty();
            if ui.add_enabled(ready, egui::Button::new("Add")).clicked() {
                events.push(Event::SetProperty {
                    id,
                    key: self.new_key.trim().to_string(),
                    value: parse_value(&self.new_value),
                    merge: None,
                });
                self.new_key.clear();
                self.new_value.clear();
            }
        });
    }

    fn several(ui: &mut Ui, doc: &Document, ids: &[ObjectId], events: &mut Vec<Event>) {
        ui.strong(format!("{} objects", ids.len()));
        let mut counts = std::collections::BTreeMap::new();
        for id in ids {
            *counts.entry(id.kind()).or_insert(0usize) += 1;
        }
        for (kind, n) in counts {
            ui.weak(format!("{n} {}", kind_name(kind)));
        }
        Self::layer_combo(ui, doc, ids, events);
    }

    fn materials(
        &mut self,
        ui: &mut Ui,
        doc: &Document,
        revision: u64,
        ids: &[ObjectId],
        events: &mut Vec<Event>,
    ) {
        if self
            .materials
            .as_ref()
            .is_none_or(|(rev, _)| *rev != revision)
        {
            self.materials = Some((revision, document_materials(doc)));
        }
        let names = self
            .materials
            .as_ref()
            .map(|(_, n)| n.clone())
            .unwrap_or_default();
        let targets: Vec<ObjectId> = ids
            .iter()
            .copied()
            .filter(|id| matches!(id, ObjectId::Brush(_) | ObjectId::Mesh(_)))
            .collect();
        let used: BTreeSet<String> = targets
            .iter()
            .filter_map(|&id| doc.get(id))
            .flat_map(|o| {
                face_materials(&o)
                    .into_iter()
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .collect();
        egui::CollapsingHeader::new("Materials")
            .default_open(true)
            .show(ui, |ui| {
                if targets.is_empty() {
                    ui.weak("Select brushes or meshes to assign a material");
                }
                for name in &names {
                    ui.horizontal(|ui| {
                        let [r, g, b] = material_rgb8(name);
                        let (rect, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                        ui.painter()
                            .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
                        let shown = if name.is_empty() { "(none)" } else { name };
                        let response = ui.selectable_label(used.contains(name), shown);
                        if response.clicked() && !targets.is_empty() {
                            events.push(Event::SetMaterial {
                                ids: targets.clone(),
                                material: name.clone(),
                            });
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_material)
                            .desired_width(110.0)
                            .hint_text("new material"),
                    );
                    let ready = !targets.is_empty() && !self.new_material.trim().is_empty();
                    if ui.add_enabled(ready, egui::Button::new("Apply")).clicked() {
                        events.push(Event::SetMaterial {
                            ids: targets.clone(),
                            material: self.new_material.trim().to_string(),
                        });
                        self.new_material.clear();
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_values_are_recognised() {
        assert_eq!(parse_value("true"), PropertyValue::Bool(true));
        assert_eq!(parse_value(" 2.5 "), PropertyValue::Number(2.5));
        assert_eq!(
            parse_value("1 2 3"),
            PropertyValue::Vec3(DVec3::new(1.0, 2.0, 3.0))
        );
        assert_eq!(parse_value("1 2"), PropertyValue::String("1 2".to_string()));
        assert_eq!(parse_value("door"), PropertyValue::String("door".into()));
    }
}

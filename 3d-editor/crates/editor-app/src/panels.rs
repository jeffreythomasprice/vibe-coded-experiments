use std::collections::BTreeSet;

use editor_cmd::args::Members;
use editor_cmd::component_defs::Catalog;
use editor_cmd::links::{EntityLink, LinkStatus};
use editor_cmd::results::ComponentDefsList;
use editor_core::{Document, LayerId, Object, ObjectId, ObjectKind, Properties, Value, find_null};
use egui::collapsing_header::CollapsingState;
use egui::{Align, ComboBox, DragValue, Ui};
use glam::DVec3;

use crate::tree::Tree;

const ERROR: egui::Color32 = egui::Color32::from_rgb(0xe0, 0x5a, 0x5a);
const WARN: egui::Color32 = egui::Color32::from_rgb(0xe0, 0xb0, 0x50);

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
    SetTransform {
        id: ObjectId,
        origin: Option<DVec3>,
        angles: Option<DVec3>,
        members: Members,
        merge: Option<String>,
    },
    SetComponent {
        id: ObjectId,
        name: String,
        data: Properties,
    },
    AddComponent {
        ids: Vec<ObjectId>,
        name: String,
    },
    RemoveComponent {
        id: ObjectId,
        name: String,
    },
    /// Add a component to the selected entities, or make an entity of the selected geometry, or place a point entity at `point` (or the middle of the selection or scene).
    PlaceComponent {
        name: String,
        point: Option<DVec3>,
    },
    EntityFromSelection,
    ReloadDefs,
    AddDefsFile,
    EmbedDefs(ObjectId),
    RemoveDefs(ObjectId),
}

/// Drag-and-drop payload of the component browser.
#[derive(Debug, Clone)]
pub struct ComponentDrag {
    pub name: String,
}

/// What a component's JSON text area holds.
#[derive(Debug, PartialEq)]
pub enum JsonCheck {
    Syntax(String),
    NotObject,
    Null(String),
    /// Valid JSON object; `schema` lists mismatches with the component schema (advisory).
    Object {
        data: Properties,
        schema: Vec<String>,
    },
}

pub fn check_json(text: &str, catalog: &Catalog, component: &str) -> JsonCheck {
    let value: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            return JsonCheck::Syntax(format!("line {}, column {}: {e}", e.line(), e.column()));
        }
    };
    let Value::Object(data) = value else {
        return JsonCheck::NotObject;
    };
    if let Some(pointer) = find_null(&Value::Object(data.clone())) {
        return JsonCheck::Null(pointer);
    }
    let schema = catalog
        .validate(component, &data)
        .into_iter()
        .map(|e| {
            format!(
                "{}: {}",
                if e.pointer.is_empty() {
                    "/"
                } else {
                    &e.pointer
                },
                e.message
            )
        })
        .collect();
    JsonCheck::Object { data, schema }
}

#[derive(Default)]
struct JsonEdit {
    text: String,
    base: String,
}

#[derive(Default)]
pub struct Panels {
    tree: Option<(u64, Tree)>,
    seen_selection: BTreeSet<ObjectId>,
    reveal: Option<ObjectId>,
    clicked_here: bool,
    drag_serial: u64,
    drag_key: String,
    json: std::collections::HashMap<(ObjectId, String), JsonEdit>,
    pivot_only: bool,
    browser_search: String,
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
        ObjectId::Entity(e) => doc.entities.get(&e).map_or_else(
            || id.to_string(),
            |x| {
                let what = x.components.keys().next().map_or("entity", String::as_str);
                format!("{what}  {id}")
            },
        ),
        ObjectId::Brush(_) => format!("brush  {id}"),
        ObjectId::Mesh(_) => format!("mesh  {id}"),
        ObjectId::Camera(_) | ObjectId::Material(_) | ObjectId::ComponentDefs(_) => id.to_string(),
    }
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
        ObjectKind::Material => "material",
        ObjectKind::ComponentDefs => "entity definitions",
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
        selection: &BTreeSet<ObjectId>,
        catalog: &Catalog,
        links: &[EntityLink],
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let ids: Vec<ObjectId> = selection.iter().copied().collect();
        match ids.as_slice() {
            [] => {
                ui.weak("Nothing selected");
            }
            [id] => self.single(ui, doc, *id, catalog, links, &mut events),
            many => Self::several(ui, doc, many, &mut events),
        }
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
            .filter(|o| {
                !matches!(
                    o,
                    Object::Layer(_)
                        | Object::Camera(_)
                        | Object::Material(_)
                        | Object::ComponentDefs(_)
                )
            })
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

    fn single(
        &mut self,
        ui: &mut Ui,
        doc: &Document,
        id: ObjectId,
        catalog: &Catalog,
        links: &[EntityLink],
        events: &mut Vec<Event>,
    ) {
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
        if let Some(b) = editor_cmd::extent(doc, id) {
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
            let has_members = !doc.children(id).is_empty();
            self.entity(ui, e, catalog, links, has_members, events);
        }
    }

    fn entity(
        &mut self,
        ui: &mut Ui,
        e: &editor_core::Entity,
        catalog: &Catalog,
        links: &[EntityLink],
        has_members: bool,
        events: &mut Vec<Event>,
    ) {
        let id = ObjectId::from(e.id);
        let members = if has_members && self.pivot_only {
            Members::Keep
        } else {
            Members::Carry
        };
        ui.weak("origin");
        if let Some((origin, key)) = self.vec3(ui, e.origin, 1.0) {
            events.push(Event::SetTransform {
                id,
                origin: Some(origin),
                angles: None,
                members,
                merge: Some(key),
            });
        }
        ui.weak("angles (pitch yaw roll)");
        if let Some((angles, key)) = self.vec3(ui, e.angles, 1.0) {
            events.push(Event::SetTransform {
                id,
                origin: None,
                angles: Some(angles),
                members,
                merge: Some(key),
            });
        }
        if has_members {
            ui.checkbox(&mut self.pivot_only, "Move pivot only")
                .on_hover_text("Edit the origin and angles without moving the brushes");
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.strong("Components");
            let missing: Vec<&str> = catalog
                .components()
                .map(|r| r.def.name.as_str())
                .filter(|n| !e.components.contains_key(*n))
                .collect();
            ComboBox::from_id_salt(("add-component", id))
                .selected_text("+ add")
                .show_ui(ui, |ui| {
                    for name in missing {
                        if ui.selectable_label(false, name).clicked() {
                            events.push(Event::AddComponent {
                                ids: vec![id],
                                name: name.to_string(),
                            });
                        }
                    }
                });
        });
        for (name, data) in &e.components {
            self.component(ui, id, name, data, catalog, events);
        }
        Self::links(ui, id, links, events);
    }

    fn component(
        &mut self,
        ui: &mut Ui,
        id: ObjectId,
        name: &str,
        data: &Properties,
        catalog: &Catalog,
        events: &mut Vec<Event>,
    ) {
        let def = catalog.component(name);
        let header = egui::CollapsingHeader::new(name)
            .id_salt(("component", id, name))
            .default_open(true);
        header.show(ui, |ui| {
            match def {
                Some(r) => {
                    if let Some(d) = &r.def.description {
                        ui.weak(d);
                    }
                }
                None if catalog.components().any(|r| r.from.is_some()) => {
                    ui.colored_label(WARN, "no definition for this component");
                }
                None => {}
            }
            let canonical =
                serde_json::to_string_pretty(&Value::Object(data.clone())).unwrap_or_default();
            let edit = self.json.entry((id, name.to_string())).or_default();
            if edit.text == edit.base && canonical != edit.base || edit.base.is_empty() {
                edit.text = canonical.clone();
                edit.base = canonical;
            }
            ui.add(
                egui::TextEdit::multiline(&mut edit.text)
                    .id_salt(("component-json", id, name))
                    .code_editor()
                    .desired_rows(6)
                    .desired_width(f32::INFINITY)
                    .lock_focus(true),
            );
            let dirty = edit.text != edit.base;
            let check = check_json(&edit.text, catalog, name);
            let mut applicable = None;
            match &check {
                JsonCheck::Syntax(m) => {
                    ui.colored_label(ERROR, m);
                }
                JsonCheck::NotObject => {
                    ui.colored_label(ERROR, "must be a JSON object");
                }
                JsonCheck::Null(p) => {
                    ui.colored_label(ERROR, format!("{p} is null, which scenes cannot store"));
                }
                JsonCheck::Object { data, schema } => {
                    applicable = Some(data);
                    if def.is_some() {
                        if schema.is_empty() {
                            ui.weak("matches the schema");
                        }
                        for message in schema {
                            ui.colored_label(WARN, message);
                        }
                    }
                }
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(dirty && applicable.is_some(), egui::Button::new("Apply"))
                    .clicked()
                    && let Some(data) = applicable
                {
                    edit.base = serde_json::to_string_pretty(&Value::Object(data.clone()))
                        .unwrap_or_default();
                    edit.text = edit.base.clone();
                    events.push(Event::SetComponent {
                        id,
                        name: name.to_string(),
                        data: data.clone(),
                    });
                }
                if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
                    edit.text = edit.base.clone();
                }
                if ui.button("Remove").clicked() {
                    events.push(Event::RemoveComponent {
                        id,
                        name: name.to_string(),
                    });
                }
            });
        });
    }

    fn links(ui: &mut Ui, id: ObjectId, links: &[EntityLink], events: &mut Vec<Event>) {
        let ours: Vec<&EntityLink> = links
            .iter()
            .filter(|l| ObjectId::from(l.from) == id)
            .collect();
        let incoming: Vec<&EntityLink> = links
            .iter()
            .filter(|l| l.to.is_some_and(|t| ObjectId::from(t) == id))
            .collect();
        if ours.is_empty() && incoming.is_empty() {
            return;
        }
        ui.add_space(4.0);
        ui.weak("links");
        for l in ours {
            ui.horizontal(|ui| {
                ui.monospace(format!("{}{} ->", l.component, l.pointer));
                match (l.status, l.to) {
                    (LinkStatus::Ok, Some(to)) => {
                        if ui.link(to.to_string()).clicked() {
                            events.push(Event::Select {
                                id: to.into(),
                                additive: false,
                            });
                        }
                    }
                    (LinkStatus::Missing, _) => {
                        ui.colored_label(WARN, format!("{} (missing)", l.value));
                    }
                    (LinkStatus::NotEntity, _) => {
                        ui.colored_label(WARN, format!("{} (not an entity)", l.value));
                    }
                    _ => {
                        ui.colored_label(WARN, format!("{:?} (not an id)", l.value));
                    }
                }
            });
        }
        for l in incoming {
            ui.horizontal(|ui| {
                ui.weak("from");
                if ui.link(l.from.to_string()).clicked() {
                    events.push(Event::Select {
                        id: l.from.into(),
                        additive: false,
                    });
                }
                ui.monospace(format!("{}{}", l.component, l.pointer));
            });
        }
    }

    pub fn browser(
        &mut self,
        ui: &mut Ui,
        catalog: &Catalog,
        defs: &ComponentDefsList,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.browser_search)
                    .desired_width(ui.available_width() - 60.0)
                    .hint_text("search"),
            );
            if ui
                .small_button("Reload")
                .on_hover_text("Read the definition files again")
                .clicked()
            {
                events.push(Event::ReloadDefs);
            }
        });
        if ui
            .button("Entity from selection")
            .on_hover_text("Make one entity that owns the selected world brushes and meshes")
            .clicked()
        {
            events.push(Event::EntityFromSelection);
        }
        let needle = self.browser_search.trim().to_lowercase();
        let mut groups: std::collections::BTreeMap<&str, Vec<&editor_core::ComponentDef>> =
            Default::default();
        for r in catalog.components() {
            let c = &r.def;
            if needle.is_empty() || c.name.to_lowercase().contains(&needle) {
                groups
                    .entry(c.category.as_deref().unwrap_or("other"))
                    .or_default()
                    .push(c);
            }
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (category, components) in groups {
                egui::CollapsingHeader::new(category)
                    .default_open(true)
                    .show(ui, |ui| {
                        for c in components {
                            Self::component_row(ui, c, &mut events);
                        }
                    });
            }
            ui.add_space(8.0);
            ui.separator();
            ui.strong("Definitions");
            for d in &defs.defs {
                ui.horizontal_wrapped(|ui| {
                    let what = d.path.as_deref().unwrap_or("embedded");
                    ui.monospace(format!("{}  {what}", d.id));
                    ui.weak(format!("{} components", d.components));
                    if d.path.is_some() && ui.small_button("Embed").clicked() {
                        events.push(Event::EmbedDefs(d.id.into()));
                    }
                    if ui.small_button("Remove").clicked() {
                        events.push(Event::RemoveDefs(d.id.into()));
                    }
                });
            }
            for p in &defs.problems {
                ui.colored_label(ERROR, &p.message);
            }
            if ui.button("Add file...").clicked() {
                events.push(Event::AddDefsFile);
            }
        });
        events
    }

    fn component_row(ui: &mut Ui, c: &editor_core::ComponentDef, events: &mut Vec<Event>) {
        let [r, g, b] = c.color.map_or_else(
            || editor_render::material_rgb8(&c.name),
            |rgb| rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
        );
        ui.horizontal(|ui| {
            let (swatch, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(swatch, 2.0, egui::Color32::from_rgb(r, g, b));
            let drag_id = egui::Id::new(("component-drag", c.name.as_str()));
            let response = ui
                .dnd_drag_source(
                    drag_id,
                    ComponentDrag {
                        name: c.name.to_string(),
                    },
                    |ui| {
                        ui.label(c.name.as_str());
                    },
                )
                .response;
            if let Some(d) = &c.description {
                response.on_hover_text(d);
            }
            if ui
                .small_button("+")
                .on_hover_text(
                    "Add to the selected entities, make an entity of the selected brushes, or place a point entity",
                )
                .clicked()
            {
                events.push(Event::PlaceComponent {
                    name: c.name.to_string(),
                    point: None,
                });
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::from_components([(
            Some(editor_core::ComponentDefsId(1)),
            editor_core::ComponentDef {
                name: "light".parse().unwrap(),
                description: None,
                category: None,
                color: None,
                size: None,
                schema: serde_json::json!({
                    "type": "object",
                    "properties": { "intensity": { "type": "number" } }
                }),
            },
        )])
    }

    #[test]
    fn text_is_checked_for_syntax_shape_nulls_and_schema() {
        let c = catalog();
        assert!(matches!(check_json("{", &c, "light"), JsonCheck::Syntax(_)));
        assert_eq!(check_json("[1]", &c, "light"), JsonCheck::NotObject);
        assert_eq!(
            check_json(r#"{"a": [null]}"#, &c, "light"),
            JsonCheck::Null("/a/0".into())
        );
        match check_json(r#"{"intensity": "bright"}"#, &c, "light") {
            JsonCheck::Object { schema, .. } => {
                assert_eq!(schema.len(), 1);
                assert!(schema[0].starts_with("/intensity"));
            }
            other => panic!("{other:?}"),
        }
        match check_json(r#"{"intensity": 3}"#, &c, "other") {
            JsonCheck::Object { schema, .. } => assert!(schema.is_empty()),
            other => panic!("{other:?}"),
        }
    }
}

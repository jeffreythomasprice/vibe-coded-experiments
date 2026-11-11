use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use editor_cmd::args::{
    AlignAction, AlignUv, Anchor, Create, DeleteMaterial, FitAxes, RenameMaterial, ResetTo, Set,
    SetUv, Store, TextureInput, UvAxis, UvMode,
};
use editor_core::{Document, ElementRef, FaceAttrs, FaceId, Filter, MaterialId, ObjectId, Part};
use editor_render::TextureCache;
use egui::{Color32, DragValue, Sense, TextureHandle, Ui, vec2};
use glam::DVec2;

const THUMB: f32 = 52.0;
const THUMB_PIXELS: usize = 64;

/// What the surface panels ask the editor to do.
pub enum Event {
    SetUv {
        args: SetUv,
        merge: Option<String>,
    },
    Align(AlignUv),
    /// Continue the first selected face's image onto the other selected faces.
    WrapFromFirst,
    /// Give the selected faces (or, with none, the selected objects) this material.
    Assign(String),
    CreateMaterial(Create),
    SetMaterial(Set),
    RenameMaterial(RenameMaterial),
    DeleteMaterial(DeleteMaterial),
    EmbedMaterial(String),
}

pub struct SurfaceUi {
    selected: Option<String>,
    thumbs: HashMap<MaterialId, (u64, TextureHandle)>,
    new_name: String,
    rename: String,
    filter: String,
    store: Option<Store>,
    step: f64,
    repeat: [f64; 2],
    as_group: bool,
    drag_serial: u64,
    drag_key: String,
}

impl Default for SurfaceUi {
    fn default() -> Self {
        Self {
            selected: None,
            thumbs: HashMap::new(),
            new_name: String::new(),
            rename: String::new(),
            filter: String::new(),
            store: None,
            step: 16.0,
            repeat: [1.0, 1.0],
            as_group: false,
            drag_serial: 0,
            drag_key: String::new(),
        }
    }
}

fn face_attrs(doc: &Document, object: ObjectId, face: FaceId) -> Option<FaceAttrs> {
    match object {
        ObjectId::Brush(b) => doc
            .brushes
            .get(&b)?
            .brush
            .faces
            .iter()
            .find(|f| f.id == face)
            .map(|f| f.attrs.clone()),
        ObjectId::Mesh(m) => doc
            .meshes
            .get(&m)?
            .mesh
            .faces
            .iter()
            .find(|f| f.id == face)
            .map(|f| f.attrs.clone()),
        _ => None,
    }
}

fn refs(faces: &BTreeSet<(ObjectId, FaceId)>) -> Vec<ElementRef> {
    faces
        .iter()
        .map(|&(object, face)| ElementRef {
            object,
            part: Part::Face(face),
        })
        .collect()
}

fn relative(faces: Vec<ElementRef>) -> SetUv {
    SetUv {
        faces,
        offset: None,
        scale: None,
        rotation: None,
        mode: Some(UvMode::Relative),
    }
}

fn texture_input(path: &Path, store: Store) -> TextureInput {
    TextureInput::File {
        path: path.display().to_string(),
        store: Some(store),
    }
}

fn pick_image() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("image", &["png", "jpg", "jpeg", "tga", "bmp"])
        .pick_file()
}

impl SurfaceUi {
    pub fn selected_material(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    fn key(&mut self, started: bool) -> String {
        if started {
            self.drag_serial += 1;
            self.drag_key = format!("uv-{}", self.drag_serial);
        }
        self.drag_key.clone()
    }

    /// A number whose edits become relative changes: `from` is the shown value and `to` the edited one.
    fn field(&mut self, ui: &mut Ui, shown: f64, speed: f64) -> Option<(f64, String)> {
        let mut v = shown;
        let response = ui.add(DragValue::new(&mut v).speed(speed).max_decimals(3));
        if response.drag_started() || response.gained_focus() {
            self.key(true);
        }
        response.changed().then(|| (v, self.key(false)))
    }

    pub fn faces(
        &mut self,
        ui: &mut Ui,
        doc: &Document,
        faces: &BTreeSet<(ObjectId, FaceId)>,
        out: &mut Vec<Event>,
    ) {
        let targets = refs(faces);
        let attrs: Vec<FaceAttrs> = faces
            .iter()
            .filter_map(|&(o, f)| face_attrs(doc, o, f))
            .collect();
        let Some(first) = attrs.first() else { return };
        if faces.len() == 1 {
            let (o, f) = faces.iter().next().expect("one face");
            ui.strong(format!("face {o}/f{}", f.0));
        } else {
            ui.strong(format!("{} faces", faces.len()));
        }
        let same = |pick: &dyn Fn(&FaceAttrs) -> f64| attrs.iter().all(|a| pick(a) == pick(first));
        ui.horizontal(|ui| {
            ui.weak("material");
            let all_same = attrs.iter().all(|a| a.material == first.material);
            ui.monospace(if !all_same {
                "—"
            } else if first.material.is_empty() {
                "(none)"
            } else {
                &first.material
            });
        });

        egui::Grid::new("uv-fields").num_columns(3).show(ui, |ui| {
            let mixed = |ui: &mut Ui, same: bool| {
                if !same {
                    ui.weak("≠");
                } else {
                    ui.label("");
                }
            };
            ui.weak("offset");
            ui.horizontal(|ui| {
                if let Some((v, key)) = self.field(ui, first.offset.x, 0.5) {
                    let mut a = relative(targets.clone());
                    a.offset = Some(DVec2::new(v - first.offset.x, 0.0));
                    out.push(Event::SetUv {
                        args: a,
                        merge: Some(key),
                    });
                }
                if let Some((v, key)) = self.field(ui, first.offset.y, 0.5) {
                    let mut a = relative(targets.clone());
                    a.offset = Some(DVec2::new(0.0, v - first.offset.y));
                    out.push(Event::SetUv {
                        args: a,
                        merge: Some(key),
                    });
                }
            });
            mixed(ui, same(&|a| a.offset.x) && same(&|a| a.offset.y));
            ui.end_row();

            ui.weak("scale");
            ui.horizontal(|ui| {
                for axis in 0..2 {
                    let current = first.scale[axis];
                    if let Some((v, key)) = self.field(ui, current, 0.01)
                        && v.abs() > 1e-6
                    {
                        let mut a = relative(targets.clone());
                        let mut factor = DVec2::ONE;
                        factor[axis] = v / current;
                        a.scale = Some(factor);
                        out.push(Event::SetUv {
                            args: a,
                            merge: Some(key),
                        });
                    }
                }
            });
            mixed(ui, same(&|a| a.scale.x) && same(&|a| a.scale.y));
            ui.end_row();

            ui.weak("rotation");
            if let Some((v, key)) = self.field(ui, first.rotation, 0.5) {
                let mut a = relative(targets.clone());
                a.rotation = Some(v - first.rotation);
                out.push(Event::SetUv {
                    args: a,
                    merge: Some(key),
                });
            }
            mixed(ui, same(&|a| a.rotation));
            ui.end_row();
        });

        let nudge = |offset: DVec2, scale: DVec2, rotation: f64| {
            let mut a = relative(targets.clone());
            a.offset = (offset != DVec2::ZERO).then_some(offset);
            a.scale = (scale != DVec2::ONE).then_some(scale);
            a.rotation = (rotation != 0.0).then_some(rotation);
            Event::SetUv {
                args: a,
                merge: None,
            }
        };
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.weak("move");
            let s = self.step;
            for (label, tip, d) in [
                ("Left", "move the image left", DVec2::new(s, 0.0)),
                ("Right", "move the image right", DVec2::new(-s, 0.0)),
                ("Up", "move the image up", DVec2::new(0.0, s)),
                ("Down", "move the image down", DVec2::new(0.0, -s)),
            ] {
                if ui.button(label).on_hover_text(tip).clicked() {
                    out.push(nudge(d, DVec2::ONE, 0.0));
                }
            }
            ui.add(
                DragValue::new(&mut self.step)
                    .speed(0.5)
                    .prefix("by ")
                    .range(0.0..=4096.0),
            );
        });
        ui.horizontal(|ui| {
            ui.weak("scale");
            if ui.button("×2").on_hover_text("twice as large").clicked() {
                out.push(nudge(DVec2::ZERO, DVec2::splat(2.0), 0.0));
            }
            if ui.button("÷2").on_hover_text("half as large").clicked() {
                out.push(nudge(DVec2::ZERO, DVec2::splat(0.5), 0.0));
            }
            ui.weak("rotate");
            for (label, d) in [("−90", -90.0), ("−15", -15.0), ("+15", 15.0), ("+90", 90.0)] {
                if ui.button(label).clicked() {
                    out.push(nudge(DVec2::ZERO, DVec2::ONE, d));
                }
            }
        });
        let align = |action: AlignAction, as_group: bool| {
            Event::Align(AlignUv {
                faces: targets.clone(),
                action,
                as_group,
            })
        };
        ui.horizontal(|ui| {
            ui.weak("flip");
            if ui.button("U").clicked() {
                out.push(align(AlignAction::Flip { axis: UvAxis::U }, false));
            }
            if ui.button("V").clicked() {
                out.push(align(AlignAction::Flip { axis: UvAxis::V }, false));
            }
            ui.weak("reset");
            if ui
                .button("World")
                .on_hover_text("axes from the face direction")
                .clicked()
            {
                out.push(align(
                    AlignAction::Reset {
                        to: Some(ResetTo::Paraxial),
                    },
                    false,
                ));
            }
            if ui
                .button("Face")
                .on_hover_text("axes lie in the face")
                .clicked()
            {
                out.push(align(
                    AlignAction::Reset {
                        to: Some(ResetTo::FaceAligned),
                    },
                    false,
                ));
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("Fit")
                .on_hover_text("scale the image to cover the face this many times")
                .clicked()
            {
                out.push(align(
                    AlignAction::Fit {
                        repeat: Some(self.repeat),
                        axes: Some(FitAxes::Both),
                    },
                    self.as_group,
                ));
            }
            ui.add(
                DragValue::new(&mut self.repeat[0])
                    .speed(0.1)
                    .range(0.01..=1000.0),
            );
            ui.label("×");
            ui.add(
                DragValue::new(&mut self.repeat[1])
                    .speed(0.1)
                    .range(0.01..=1000.0),
            );
            ui.checkbox(&mut self.as_group, "one surface")
                .on_hover_text("treat the selected faces as one surface");
        });
        ui.horizontal(|ui| {
            ui.weak("justify");
            egui::Grid::new("justify")
                .spacing(vec2(2.0, 2.0))
                .show(ui, |ui| {
                    let anchors = [Anchor::Start, Anchor::Center, Anchor::End];
                    for v in anchors {
                        for u in anchors {
                            if ui.small_button(" ").clicked() {
                                out.push(align(
                                    AlignAction::Justify {
                                        u: Some(u),
                                        v: Some(v),
                                    },
                                    self.as_group,
                                ));
                            }
                        }
                        ui.end_row();
                    }
                });
            ui.vertical(|ui| {
                if ui.small_button("left").clicked() {
                    out.push(align(
                        AlignAction::Justify {
                            u: Some(Anchor::Start),
                            v: None,
                        },
                        self.as_group,
                    ));
                }
                if ui.small_button("right").clicked() {
                    out.push(align(
                        AlignAction::Justify {
                            u: Some(Anchor::End),
                            v: None,
                        },
                        self.as_group,
                    ));
                }
                if ui.small_button("top").clicked() {
                    out.push(align(
                        AlignAction::Justify {
                            u: None,
                            v: Some(Anchor::Start),
                        },
                        self.as_group,
                    ));
                }
                if ui.small_button("bottom").clicked() {
                    out.push(align(
                        AlignAction::Justify {
                            u: None,
                            v: Some(Anchor::End),
                        },
                        self.as_group,
                    ));
                }
            });
        });
        if faces.len() > 1
            && ui
                .button("Wrap from first")
                .on_hover_text("continue the first face's image over the other faces")
                .clicked()
        {
            out.push(Event::WrapFromFirst);
        }
    }

    fn thumbnail(
        &mut self,
        ctx: &egui::Context,
        doc: &Document,
        id: MaterialId,
        textures: &TextureCache,
        scene_dir: Option<&Path>,
    ) -> Result<Option<egui::TextureId>, String> {
        let Some(material) = doc.materials.get(&id) else {
            return Ok(None);
        };
        let Some(source) = &material.texture else {
            self.thumbs.remove(&id);
            return Ok(None);
        };
        let data = textures.get(source, scene_dir)?;
        if let Some((key, handle)) = self.thumbs.get(&id)
            && *key == data.key
        {
            return Ok(Some(handle.id()));
        }
        let (w, h) = (data.width as usize, data.height as usize);
        let scale = (THUMB_PIXELS as f32 / w.max(h) as f32).min(1.0);
        let (tw, th) = (
            ((w as f32 * scale) as usize).max(1),
            ((h as f32 * scale) as usize).max(1),
        );
        let mut pixels = Vec::with_capacity(tw * th * 4);
        for y in 0..th {
            for x in 0..tw {
                let (sx, sy) = (x * w / tw, y * h / th);
                let at = (sy * w + sx) * 4;
                pixels.extend_from_slice(&data.rgba[at..at + 4]);
            }
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([tw, th], &pixels);
        let handle = ctx.load_texture(
            format!("material-{id}"),
            image,
            egui::TextureOptions::NEAREST,
        );
        let texture = handle.id();
        self.thumbs.insert(id, (data.key, handle));
        Ok(Some(texture))
    }

    pub fn materials(
        &mut self,
        ui: &mut Ui,
        doc: &Document,
        textures: &TextureCache,
        scene_dir: Option<&Path>,
        out: &mut Vec<Event>,
    ) {
        self.thumbs.retain(|id, _| doc.materials.contains_key(id));
        if self
            .selected
            .as_ref()
            .is_some_and(|s| doc.material_by_name(s).is_none())
        {
            self.selected = None;
        }
        let default_store = if scene_dir.is_some() {
            Store::Relative
        } else {
            Store::Absolute
        };
        egui::CollapsingHeader::new("Materials")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .desired_width(80.0)
                            .hint_text("filter"),
                    );
                    if ui.button("New from image…").clicked()
                        && let Some(path) = pick_image()
                    {
                        let name = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "material".into());
                        let name = unique_name(doc, &name);
                        out.push(Event::CreateMaterial(Create {
                            name: name.clone(),
                            texture: Some(texture_input(
                                &path,
                                self.store.unwrap_or(default_store),
                            )),
                            ..Default::default()
                        }));
                        self.selected = Some(name);
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_name)
                            .desired_width(110.0)
                            .hint_text("new material"),
                    );
                    let ready = !self.new_name.trim().is_empty()
                        && doc.material_by_name(self.new_name.trim()).is_none();
                    if ui.add_enabled(ready, egui::Button::new("Create")).clicked() {
                        let name = self.new_name.trim().to_string();
                        out.push(Event::CreateMaterial(Create {
                            name: name.clone(),
                            ..Default::default()
                        }));
                        self.selected = Some(name);
                        self.new_name.clear();
                    }
                });

                let filter = self.filter.to_lowercase();
                let ctx = ui.ctx().clone();
                let mut chosen: Option<String> = None;
                ui.horizontal_wrapped(|ui| {
                    for (id, m) in &doc.materials {
                        if !filter.is_empty() && !m.name.to_lowercase().contains(&filter) {
                            continue;
                        }
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(THUMB, THUMB), Sense::click());
                        let tint = Color32::from_rgba_unmultiplied(
                            (m.color[0] * 255.0) as u8,
                            (m.color[1] * 255.0) as u8,
                            (m.color[2] * 255.0) as u8,
                            255,
                        );
                        let mut problem = None;
                        match self.thumbnail(&ctx, doc, *id, textures, scene_dir) {
                            Ok(Some(texture)) => {
                                ui.painter().image(
                                    texture,
                                    rect,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    ),
                                    Color32::WHITE,
                                );
                            }
                            Ok(None) => {
                                ui.painter().rect_filled(rect, 2.0, tint);
                            }
                            Err(e) => {
                                ui.painter().rect_filled(
                                    rect,
                                    2.0,
                                    Color32::from_rgb(110, 30, 110),
                                );
                                problem = Some(e);
                            }
                        }
                        let selected = self.selected.as_deref() == Some(m.name.as_str());
                        ui.painter().rect_stroke(
                            rect,
                            2.0,
                            egui::Stroke::new(
                                if selected { 2.0 } else { 1.0 },
                                if selected {
                                    Color32::from_rgb(90, 190, 255)
                                } else {
                                    Color32::from_gray(70)
                                },
                            ),
                            egui::StrokeKind::Inside,
                        );
                        let label = problem
                            .map(|e| format!("{}\n{e}", m.name))
                            .unwrap_or_else(|| m.name.clone());
                        if response.on_hover_text(label).clicked() {
                            chosen = Some(m.name.clone());
                        }
                    }
                });
                if let Some(name) = chosen {
                    self.selected = Some(name.clone());
                    self.rename = name.clone();
                    out.push(Event::Assign(name));
                }

                let undefined = undefined_names(doc);
                if !undefined.is_empty() {
                    ui.add_space(4.0);
                    ui.weak("used by faces but not defined:");
                    for name in undefined {
                        ui.horizontal(|ui| {
                            ui.monospace(&name);
                            if ui.small_button("Define").clicked() {
                                out.push(Event::CreateMaterial(Create {
                                    name,
                                    ..Default::default()
                                }));
                            }
                        });
                    }
                }

                if let Some(name) = self.selected.clone()
                    && let Some(m) = doc.material_by_name(&name)
                {
                    ui.separator();
                    self.editor(ui, m, scene_dir, default_store, out);
                }
            });
    }

    fn editor(
        &mut self,
        ui: &mut Ui,
        m: &editor_core::Material,
        scene_dir: Option<&Path>,
        default_store: Store,
        out: &mut Vec<Event>,
    ) {
        let set = |m: &editor_core::Material| Set {
            material: m.name.clone(),
            ..Default::default()
        };
        ui.horizontal(|ui| {
            ui.weak("name");
            let response =
                ui.add(egui::TextEdit::singleline(&mut self.rename).desired_width(130.0));
            if self.rename.is_empty() {
                self.rename = m.name.clone();
            }
            if response.lost_focus()
                && self.rename.trim() != m.name
                && !self.rename.trim().is_empty()
            {
                out.push(Event::RenameMaterial(RenameMaterial {
                    material: m.name.clone(),
                    name: self.rename.trim().to_string(),
                }));
                self.selected = Some(self.rename.trim().to_string());
            }
        });
        ui.horizontal(|ui| {
            ui.weak("image");
            match &m.texture {
                None => {
                    ui.weak("none");
                }
                Some(editor_core::TextureSource::Path { path }) => {
                    ui.monospace(path).on_hover_text(path);
                }
                Some(editor_core::TextureSource::Embedded { format, data }) => {
                    ui.monospace(format!("embedded {format}, {} KiB", data.0.len() / 1024));
                }
            }
        });
        let store = self.store.unwrap_or(default_store);
        ui.horizontal(|ui| {
            ui.weak("store");
            for (value, label) in [
                (Store::Relative, "relative"),
                (Store::Absolute, "absolute"),
                (Store::Embed, "embed"),
            ] {
                if ui.selectable_label(store == value, label).clicked() {
                    self.store = Some(value);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.button("Pick image…").clicked()
                && let Some(path) = pick_image()
            {
                out.push(Event::SetMaterial(Set {
                    texture: Some(texture_input(&path, store)),
                    ..set(m)
                }));
            }
            if matches!(m.texture, Some(editor_core::TextureSource::Path { .. }))
                && ui
                    .button("Embed")
                    .on_hover_text("store the image inside the scene")
                    .clicked()
            {
                out.push(Event::EmbedMaterial(m.name.clone()));
            }
            if m.texture.is_some() && ui.button("Clear").clicked() {
                out.push(Event::SetMaterial(Set {
                    clear_texture: true,
                    ..set(m)
                }));
            }
        });
        if scene_dir.is_none() && store == Store::Relative {
            ui.colored_label(
                Color32::from_rgb(220, 160, 60),
                "save the scene first for relative paths",
            );
        }

        let mut color = m.color;
        ui.horizontal(|ui| {
            ui.weak("tint");
            if ui.color_edit_button_rgba_unmultiplied(&mut color).changed() {
                out.push(Event::SetMaterial(Set {
                    color: Some(color),
                    ..set(m)
                }));
            }
            let mut emissive = m.emissive;
            ui.weak("emissive");
            if ui.color_edit_button_rgb(&mut emissive).changed() {
                out.push(Event::SetMaterial(Set {
                    emissive: Some(emissive),
                    ..set(m)
                }));
            }
            let mut strength = m.emissive_strength;
            if ui
                .add(DragValue::new(&mut strength).speed(0.05).range(0.0..=100.0))
                .changed()
            {
                out.push(Event::SetMaterial(Set {
                    emissive_strength: Some(strength),
                    ..set(m)
                }));
            }
        });
        for (label, value, is_roughness) in [
            ("roughness", m.roughness, true),
            ("metallic", m.metallic, false),
        ] {
            let mut v = value;
            ui.horizontal(|ui| {
                ui.weak(label);
                if ui.add(egui::Slider::new(&mut v, 0.0..=1.0)).changed() {
                    let mut s = set(m);
                    if is_roughness {
                        s.roughness = Some(v);
                    } else {
                        s.metallic = Some(v);
                    }
                    out.push(Event::SetMaterial(s));
                }
            });
        }
        ui.horizontal(|ui| {
            ui.weak("filter");
            for (value, label) in [(Filter::Linear, "smooth"), (Filter::Nearest, "pixelated")] {
                if ui.selectable_label(m.filter == value, label).clicked() && m.filter != value {
                    out.push(Event::SetMaterial(Set {
                        filter: Some(value),
                        ..set(m)
                    }));
                }
            }
        });
        ui.horizontal(|ui| {
            for (label, value, tip) in [
                (
                    "double sided",
                    m.double_sided,
                    "stored for export; the viewport never culls faces",
                ),
                ("unlit", m.unlit, "no editor shading"),
                (
                    "nodraw",
                    m.nodraw,
                    "not drawn by the game; shown translucent here",
                ),
            ] {
                let mut on = value;
                if ui.checkbox(&mut on, label).on_hover_text(tip).changed() {
                    let mut s = set(m);
                    match label {
                        "double sided" => s.double_sided = Some(on),
                        "unlit" => s.unlit = Some(on),
                        _ => s.nodraw = Some(on),
                    }
                    out.push(Event::SetMaterial(s));
                }
            }
        });
        if ui.button("Delete material").clicked() {
            out.push(Event::DeleteMaterial(DeleteMaterial {
                materials: vec![m.name.clone()],
                replace_with: None,
            }));
            self.selected = None;
        }
    }
}

fn unique_name(doc: &Document, base: &str) -> String {
    if doc.material_by_name(base).is_none() {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|name| doc.material_by_name(name).is_none())
        .expect("an unused name exists")
}

fn undefined_names(doc: &Document) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    for b in doc.brushes.values() {
        names.extend(b.brush.faces.iter().map(|f| f.attrs.material.clone()));
    }
    for m in doc.meshes.values() {
        names.extend(m.mesh.faces.iter().map(|f| f.attrs.material.clone()));
    }
    names.retain(|n| !n.is_empty() && doc.material_by_name(n).is_none());
    names
}

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use editor_cmd::ClipboardCopy;
use editor_cmd::args::{
    AddComponent, AddDefs, Axis, AxisArg, BrushVertexAt, Clip, CloseGroup, CreateBox,
    CreateBrushEntity, CreateEntity, Delete, EmbedDefs, Factors, Flip, Isolate, Keep, NoArgs,
    OpenGroup, Pivot, RemoveComponent, RemoveDefs, Rename, Rotate, Save, Scale, SelectFaces,
    SelectIds, SelectKind, SetComponent, SetHidden, SetLayer, SetLocked, SetMaterial,
    SetOmitFromExport, SetTransform, SetUv, TransferMode, TransferUv, Translate, TranslateVertices,
    UvMode,
};
use editor_cmd::links::{EntityLink, doc_links};
use editor_cmd::results::{
    ClipboardState, Created, CreatedMapped, Deleted, Duplicated, FaceSelection, FacesChanged,
    IdCount, IdOnly, Moved, OpenGroups, Pasted, RecoveryEntry, RecoveryList, Saved, Stepped,
    Transformed, Updated, VerticesMoved,
};
use editor_cmd::{Request, Success};
use editor_core::{ElementRef, FaceId, GroupId, ObjectId, ObjectKind, Part};
use editor_geom::Aabb;
use editor_geom::camera::{frame_camera, ortho_axes};
use editor_geom::tolerance::snap_vec;
use editor_proto::WakeFn;
use editor_render::{
    ComponentLook, PreparedScene, Renderer, SceneEnv, ViewportOptions, component_looks,
};
use egui::{Context, Key, Ui};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use glam::DVec3;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::clip::{self, ClipState, Preview};
use crate::drag::DragOp;
use crate::drill;
use crate::error::AppError;
use crate::gfx::Gfx;
use crate::nav;
use crate::pane::{Action, Pane, PaneEnv, PaneId, SurfaceClick, Tool};
use crate::panels::{Event, Panels};
use crate::remote::Remote;
use crate::surface::{self, SurfaceUi};
use crate::theme::{self, Theme};

const GRIDS: [f64; 12] = [
    0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0,
];
const DEFAULT_GRID: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeftTab {
    Outliner,
    Entities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    One,
    Two,
    Three,
    Four,
}

fn build_dock(layout: Layout) -> DockState<PaneId> {
    let mut dock = DockState::new(vec![PaneId::Persp]);
    let tree = dock.main_surface_mut();
    match layout {
        Layout::One => {}
        Layout::Two => {
            tree.split_right(NodeIndex::root(), 0.5, vec![PaneId::Top]);
        }
        Layout::Three => {
            let [_, right] = tree.split_right(NodeIndex::root(), 0.55, vec![PaneId::Top]);
            tree.split_below(right, 0.5, vec![PaneId::Front]);
        }
        Layout::Four => {
            let [left, right] = tree.split_right(NodeIndex::root(), 0.5, vec![PaneId::Top]);
            tree.split_below(left, 0.5, vec![PaneId::Front]);
            tree.split_below(right, 0.5, vec![PaneId::Side]);
        }
    }
    dock
}

struct Viewer<'a> {
    panes: &'a mut HashMap<PaneId, Pane>,
    env: PaneEnv<'a>,
}

impl TabViewer for Viewer<'_> {
    type Tab = PaneId;

    fn id(&mut self, tab: &mut PaneId) -> egui::Id {
        egui::Id::new(*tab)
    }

    fn title(&mut self, tab: &mut PaneId) -> egui::WidgetText {
        tab.title().into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut PaneId) {
        if let Some(pane) = self.panes.get_mut(tab) {
            pane.show(ui, &mut self.env);
        }
    }

    fn is_closeable(&self, _tab: &PaneId) -> bool {
        false
    }

    fn clear_background(&self, _tab: &PaneId) -> bool {
        false
    }

    fn scroll_bars(&self, _tab: &PaneId) -> [bool; 2] {
        [false, false]
    }
}

#[derive(Default)]
struct Keys {
    copy: bool,
    cut: bool,
    paste: bool,
    paste_text: Option<String>,
    paste_in_place: bool,
    duplicate: bool,
    undo: bool,
    redo: bool,
    delete: bool,
    space: bool,
    frame: bool,
    snap: bool,
    select_all: bool,
    select_invert: bool,
    select_none: bool,
    grid_finer: bool,
    grid_coarser: bool,
    rotate_ccw: bool,
    rotate_cw: bool,
    flip: Option<Axis>,
    tool: Option<Tool>,
    hide: bool,
    isolate: bool,
    show_all: bool,
    lock: bool,
    unlock_all: bool,
    escape: bool,
    backspace: bool,
    enter: bool,
    reverse: bool,
    tab: bool,
    arrow: Option<(f64, f64)>,
    page: f64,
    shift: bool,
}

impl Keys {
    fn read(i: &egui::InputState) -> Keys {
        let cmd = i.modifiers.command;
        let shift = i.modifiers.shift;
        let plain = |key| !cmd && i.key_pressed(key);
        let paste_text = i.events.iter().find_map(|e| match e {
            egui::Event::Paste(t) => Some(t.clone()),
            _ => None,
        });
        Keys {
            copy: i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
            cut: i.events.iter().any(|e| matches!(e, egui::Event::Cut)),
            paste: paste_text.is_some() || (cmd && i.key_pressed(Key::V)),
            paste_text,
            paste_in_place: shift,
            duplicate: cmd && !shift && i.key_pressed(Key::D),
            undo: cmd && !shift && i.key_pressed(Key::Z),
            redo: (cmd && shift && i.key_pressed(Key::Z)) || (cmd && i.key_pressed(Key::Y)),
            delete: i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
            space: i.key_pressed(Key::Space),
            frame: plain(Key::F),
            snap: plain(Key::Backslash),
            select_all: cmd && !shift && i.key_pressed(Key::A),
            select_invert: cmd && !shift && i.key_pressed(Key::I),
            select_none: cmd && shift && i.key_pressed(Key::A),
            hide: cmd && !shift && i.key_pressed(Key::H),
            isolate: cmd && shift && i.key_pressed(Key::H),
            show_all: i.modifiers.alt && !cmd && i.key_pressed(Key::H),
            lock: cmd && !shift && i.key_pressed(Key::L),
            unlock_all: i.modifiers.alt && !cmd && i.key_pressed(Key::L),
            escape: i.key_pressed(Key::Escape),
            backspace: i.key_pressed(Key::Backspace),
            enter: !cmd && !shift && i.key_pressed(Key::Enter),
            reverse: !cmd && shift && i.key_pressed(Key::Enter),
            tab: !cmd && !shift && i.key_pressed(Key::Tab),
            arrow: [
                (Key::ArrowLeft, (1.0, 0.0)),
                (Key::ArrowRight, (-1.0, 0.0)),
                (Key::ArrowUp, (0.0, 1.0)),
                (Key::ArrowDown, (0.0, -1.0)),
            ]
            .into_iter()
            .find_map(|(key, step)| (!cmd && i.key_pressed(key)).then_some(step)),
            page: if i.key_pressed(Key::PageUp) {
                15.0
            } else if i.key_pressed(Key::PageDown) {
                -15.0
            } else {
                0.0
            },
            shift,
            grid_finer: i.key_pressed(Key::OpenBracket),
            grid_coarser: i.key_pressed(Key::CloseBracket),
            rotate_ccw: plain(Key::Period),
            rotate_cw: plain(Key::Comma),
            flip: [(Key::X, Axis::X), (Key::Y, Axis::Y), (Key::Z, Axis::Z)]
                .into_iter()
                .find_map(|(key, axis)| {
                    (i.modifiers.alt && !cmd && i.key_pressed(key)).then_some(axis)
                }),
            tool: [
                (Key::Num1, Tool::Select),
                (Key::Num2, Tool::Move),
                (Key::Num3, Tool::Box),
                (Key::Num4, Tool::Rotate),
                (Key::Num5, Tool::Scale),
                (Key::Num6, Tool::Vertex),
                (Key::Num7, Tool::Clip),
                (Key::Num8, Tool::Edge),
                (Key::Num9, Tool::Face),
                (Key::Num0, Tool::Surface),
            ]
            .into_iter()
            .find_map(|(key, tool)| plain(key).then_some(tool)),
        }
    }
}

struct PasteRequest {
    os_text: Option<String>,
    in_place: bool,
}

pub struct Editor {
    remote: Remote,
    dock: DockState<PaneId>,
    saved_dock: Option<DockState<PaneId>>,
    panes: HashMap<PaneId, Pane>,
    tool: Tool,
    grid: f64,
    snap: bool,
    texture_lock: bool,
    texture_generation: u64,
    surface: SurfaceUi,
    pub theme: Theme,
    show_grid: bool,
    show_axes: bool,
    show_links: bool,
    left_tab: LeftTab,
    links: Vec<EntityLink>,
    component_looks: HashMap<String, ComponentLook>,
    show_help: bool,
    hover: Option<ObjectId>,
    vertex_picks: Vec<BrushVertexAt>,
    clip: ClipState,
    clip_preview: Preview,
    clip_key: Option<ClipKey>,
    active: Option<PaneId>,
    prepared: Option<(PrepKey, PreparedScene)>,
    actions: Vec<Action>,
    drag_applied: DragOp,
    drag_pivot: DVec3,
    link_zoom: bool,
    relink: bool,
    last_zoom: HashMap<PaneId, f64>,
    last_box: Option<Aabb>,
    pending_box: Option<Aabb>,
    drag_key: String,
    drag_serial: u64,
    dragging: bool,
    status: String,
    clip_text: Option<String>,
    paste_pending: Option<PasteRequest>,
    recovery: Vec<RecoveryEntry>,
    recovery_for: Option<Option<String>>,
    panels: Panels,
    pub timings: Timings,
    adapter: String,
    pub quit: bool,
    pub repaint: bool,
}

#[derive(Default, Clone, Copy)]
pub struct Timings {
    pub ui: f32,
    pub panes: f32,
    pub pick: f32,
    pub present: f32,
    pub frame: f32,
}

impl Timings {
    pub fn blend(slot: &mut f32, sample: f32) {
        *slot += (sample - *slot) * 0.1;
    }
}

type ClipKey = (u64, [u64; 9], Vec<ObjectId>);

type PrepKey = (
    u64,
    Vec<ObjectId>,
    Option<ObjectId>,
    Option<GroupId>,
    Vec<(ObjectId, FaceId)>,
    u64,
    u64,
    bool,
);

impl Editor {
    pub fn new(
        path: Option<&Path>,
        default_scene: &Path,
        adapter: String,
        wake: WakeFn,
    ) -> Result<Self, AppError> {
        let remote = Remote::connect(path, default_scene, wake)?;
        let grid = remote.doc.editor.grid;
        Ok(Self {
            remote,
            dock: build_dock(Layout::Four),
            saved_dock: None,
            panes: PaneId::ALL.iter().map(|&id| (id, Pane::new(id))).collect(),
            tool: Tool::Select,
            texture_lock: true,
            texture_generation: 0,
            surface: SurfaceUi::default(),
            grid,
            snap: true,
            theme: Theme::Dark,
            show_grid: true,
            show_axes: true,
            show_links: true,
            left_tab: LeftTab::Outliner,
            links: Vec::new(),
            component_looks: HashMap::new(),
            show_help: false,
            hover: None,
            vertex_picks: Vec::new(),
            clip: ClipState::default(),
            clip_preview: Preview::default(),
            clip_key: None,
            active: None,
            prepared: None,
            actions: Vec::new(),
            drag_applied: DragOp::Translate(DVec3::ZERO),
            drag_pivot: DVec3::ZERO,
            link_zoom: true,
            relink: true,
            last_zoom: HashMap::new(),
            last_box: None,
            pending_box: None,
            drag_key: String::new(),
            drag_serial: 0,
            dragging: false,
            status: String::new(),
            clip_text: None,
            paste_pending: None,
            recovery: Vec::new(),
            recovery_for: None,
            panels: Panels::default(),
            timings: Timings::default(),
            adapter,
            quit: false,
            repaint: false,
        })
    }

    fn send(&mut self, op: &str, args: Value, merge_key: Option<&str>) -> Option<Success> {
        let mut request = Request::new(op, args);
        request.merge_key = merge_key.map(String::from);
        match self.remote.call(request) {
            Ok(success) => {
                self.status = success.warnings.join("; ");
                Some(success)
            }
            Err(e) => {
                self.status = format!("{op}: {} ({})", e.message, e.hint);
                None
            }
        }
    }

    fn call<A: Serialize, R: DeserializeOwned>(
        &mut self,
        op: &str,
        args: &A,
        merge_key: Option<&str>,
    ) -> Option<R> {
        let args = match serde_json::to_value(args) {
            Ok(v) => v,
            Err(e) => {
                self.status = format!("{op}: bad arguments: {e}");
                return None;
            }
        };
        let success = self.send(op, args, merge_key)?;
        match serde_json::from_value(success.result) {
            Ok(r) => Some(r),
            Err(e) => {
                self.status = format!("{op}: unexpected result: {e}");
                None
            }
        }
    }

    fn selection_ids(&self) -> Vec<ObjectId> {
        self.remote.selection.iter().copied().collect()
    }

    fn selection_bounds(&self) -> Option<Aabb> {
        let doc = &self.remote.doc;
        self.remote
            .selection
            .iter()
            .filter_map(|&id| editor_cmd::extent(doc, id))
            .reduce(|a, b| a.merge(b))
    }

    fn selection_center(&self) -> DVec3 {
        self.selection_bounds().map_or(DVec3::ZERO, |b| b.center())
    }

    fn active_view_axis(&self) -> DVec3 {
        self.active
            .and_then(|a| self.panes.get(&a))
            .and_then(|p| p.camera.as_ref())
            .and_then(nav::ortho_view)
            .map_or(DVec3::Z, |view| {
                let (_, right, up) = ortho_axes(view);
                right.cross(up)
            })
    }

    fn rotate_selection(&mut self, degrees: f64) {
        let ids = self.selection_ids();
        if ids.is_empty() {
            return;
        }
        let args = Rotate {
            ids,
            axis: AxisArg::Vector(self.active_view_axis()),
            degrees,
            pivot: Pivot::default(),
            texture_lock: Some(self.texture_lock),
        };
        self.call::<_, Transformed>("transform.rotate", &args, None);
    }

    fn flip_selection(&mut self, axis: Axis) {
        let ids = self.selection_ids();
        if ids.is_empty() {
            return;
        }
        let args = Flip {
            ids,
            axis: AxisArg::Named(axis),
            pivot: Pivot::default(),
            texture_lock: Some(self.texture_lock),
        };
        self.call::<_, Transformed>("transform.flip", &args, None);
    }

    fn select(&mut self, op: &str, ids: Vec<ObjectId>) {
        self.clear_faces();
        self.call::<_, IdCount>(op, &SelectIds { ids }, None);
    }

    fn clear_faces(&mut self) {
        if !self.remote.face_selection.is_empty() {
            self.call::<_, FaceSelection>("select.faces.clear", &NoArgs::default(), None);
        }
    }

    fn face_refs(&self) -> Vec<ElementRef> {
        self.remote
            .face_selection
            .iter()
            .map(|&(object, face)| ElementRef {
                object,
                part: Part::Face(face),
            })
            .collect()
    }

    fn select_kind(&mut self, op: &str) {
        self.call::<_, IdCount>(op, &SelectKind { kind: None }, None);
    }

    fn drag_step(&mut self, step: DragOp) -> bool {
        let ids = self.selection_ids();
        let key = self.drag_key.clone();
        let pivot = Pivot::Point(self.drag_pivot);
        let key = Some(key.as_str());
        match step {
            DragOp::Translate(delta) if self.tool.element_kind().is_some() => {
                let args = TranslateVertices {
                    vertices: Vec::new(),
                    brush_vertices: self.vertex_picks.clone(),
                    delta,
                    texture_lock: Some(self.texture_lock),
                };
                let moved = self
                    .call::<_, VerticesMoved>("vertex.translate", &args, key)
                    .is_some();
                if moved {
                    for pick in &mut self.vertex_picks {
                        pick.at = snap_vec(pick.at + delta);
                    }
                    let mut seen: Vec<BrushVertexAt> = Vec::new();
                    for pick in std::mem::take(&mut self.vertex_picks) {
                        if !seen
                            .iter()
                            .any(|s| s.brush == pick.brush && s.at == pick.at)
                        {
                            seen.push(pick);
                        }
                    }
                    self.vertex_picks = seen;
                }
                moved
            }
            DragOp::Translate(delta) => self
                .call::<_, Transformed>(
                    "transform.translate",
                    &Translate {
                        ids,
                        delta,
                        texture_lock: Some(self.texture_lock),
                    },
                    key,
                )
                .is_some(),
            DragOp::Rotate { axis, degrees } => self
                .call::<_, Transformed>(
                    "transform.rotate",
                    &Rotate {
                        ids,
                        axis: AxisArg::Vector(axis),
                        degrees,
                        pivot,
                        texture_lock: Some(self.texture_lock),
                    },
                    key,
                )
                .is_some(),
            DragOp::Scale(factors) => self
                .call::<_, Transformed>(
                    "transform.scale",
                    &Scale {
                        ids,
                        factors: Factors::PerAxis(factors),
                        pivot,
                        texture_lock: Some(self.texture_lock),
                    },
                    key,
                )
                .is_some(),
        }
    }

    fn surface_click(
        &mut self,
        face: Option<(ObjectId, FaceId)>,
        additive: bool,
        mode: SurfaceClick,
    ) {
        let Some((object, id)) = face else {
            if !additive {
                self.clear_faces();
            }
            return;
        };
        let target = ElementRef {
            object,
            part: Part::Face(id),
        };
        match mode {
            SurfaceClick::Select => {
                if !self.remote.selection.is_empty() {
                    self.call::<_, IdCount>("select.clear", &NoArgs::default(), None);
                }
                let op = match (additive, self.remote.face_selection.contains(&(object, id))) {
                    (false, _) => "select.faces.set",
                    (true, true) => "select.faces.remove",
                    (true, false) => "select.faces.add",
                };
                self.call::<_, FaceSelection>(
                    op,
                    &SelectFaces {
                        faces: vec![target],
                    },
                    None,
                );
            }
            SurfaceClick::Apply => match self.surface.selected_material().map(String::from) {
                Some(material) => {
                    let args = SetMaterial {
                        faces: vec![target],
                        material,
                    };
                    self.call::<_, FacesChanged>("face.set_material", &args, None);
                }
                None => self.status = "pick a material in the inspector first".into(),
            },
            SurfaceClick::Wrap => {
                let from = self
                    .remote
                    .face_selection
                    .iter()
                    .find(|f| **f != (object, id))
                    .map(|&(object, face)| ElementRef {
                        object,
                        part: Part::Face(face),
                    });
                match from {
                    Some(from) => {
                        let args = TransferUv {
                            from,
                            to: vec![target],
                            mode: Some(TransferMode::Wrap),
                        };
                        self.call::<_, FacesChanged>("face.transfer_uv", &args, None);
                    }
                    None => self.status = "select a face to wrap from first".into(),
                }
            }
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Select { id, additive } => {
                let id = id.map(|id| self.pick_target(id));
                let selected = id.is_some_and(|id| self.remote.selection.contains(&id));
                match (id, additive, selected) {
                    (Some(id), false, _) => self.select("select.set", vec![id]),
                    (Some(id), true, true) => self.select("select.remove", vec![id]),
                    (Some(id), true, false) => self.select("select.add", vec![id]),
                    (None, false, _) => {
                        self.clear_faces();
                        self.call::<_, IdCount>("select.clear", &NoArgs::default(), None);
                    }
                    (None, true, _) => {}
                }
            }
            Action::SurfaceClick {
                face,
                additive,
                mode,
            } => self.surface_click(face, additive, mode),
            Action::Paint(id) => {
                let id = self.pick_target(id);
                self.select("select.add", vec![id]);
            }
            Action::VertexBegin(picks) => self.vertex_picks = picks,
            Action::ClipAdd { point, dir } => self.clip.add(point, dir),
            Action::ClipSet { index, point } => {
                if let Some(p) = self.clip.points.get_mut(index) {
                    *p = point;
                }
            }
            Action::OpenGroupAt(id) => {
                if let ObjectId::Group(g) = self.pick_target(id) {
                    self.call::<_, OpenGroups>("group.open", &OpenGroup { id: g }, None);
                    self.select("select.set", vec![id]);
                }
            }
            Action::Drill {
                origin,
                dir,
                additive,
            } => {
                let kinds =
                    BTreeSet::from([ObjectKind::Brush, ObjectKind::Mesh, ObjectKind::Entity]);
                let doc = &self.remote.doc;
                let mut ignore = doc.hidden_ids();
                ignore.extend(doc.locked_ids());
                let mut hits: Vec<ObjectId> = Vec::new();
                for hit in editor_cmd::ray_hits(doc, origin, dir, &kinds, &ignore, None) {
                    let target = doc.pick_target(hit.id);
                    if doc.pickable(hit.id) && !hits.contains(&target) {
                        hits.push(target);
                    }
                }
                match (drill::next_hit(&hits, &self.remote.selection), additive) {
                    (Some(id), false) => self.select("select.set", vec![id]),
                    (Some(id), true) => self.select("select.add", vec![id]),
                    (None, false) => {
                        self.call::<_, IdCount>("select.clear", &NoArgs::default(), None);
                    }
                    (None, true) => {}
                }
            }
            Action::DragBegin { pivot } => {
                if self.remote.selection.is_empty() {
                    return;
                }
                self.drag_serial += 1;
                self.drag_key = format!("drag-{}", self.drag_serial);
                self.dragging = true;
                self.drag_pivot = pivot.unwrap_or_else(|| self.selection_center());
                self.drag_applied = DragOp::Translate(DVec3::ZERO);
            }
            Action::DragTo(total) => {
                if !self.dragging {
                    return;
                }
                if self.drag_applied.is_identity() {
                    self.drag_applied = total.identity();
                }
                let step = total.step_from(self.drag_applied);
                if self.drag_step(step) {
                    self.drag_applied = total;
                }
            }
            Action::DragEnd { commit } => {
                if !self.dragging {
                    return;
                }
                self.dragging = false;
                if !commit && !self.drag_applied.is_identity() {
                    let undo = self.drag_applied.identity().step_from(self.drag_applied);
                    self.drag_step(undo);
                }
            }
            Action::DropComponent { name, point } => self.place_component(name, Some(point)),
            Action::PasteAt { .. } => {}
            Action::CreateBox { min, max } => {
                self.last_box = Some(Aabb { min, max });
                let args = CreateBox {
                    min,
                    max,
                    layer: None,
                    material: String::new(),
                };
                let created = self.call::<_, Created>("brush.create_box", &args, None);
                if let Some(id) = created.and_then(|c| c.id) {
                    self.select("select.set", vec![id]);
                }
            }
        }
    }

    fn clip_apply(&mut self) {
        let Some(plane) = self.clip.plane() else {
            self.status = "clip: place two points first".into();
            return;
        };
        let ids = clip::brush_ids(&self.remote.doc, &self.remote.selection);
        if ids.is_empty() {
            self.status = "clip: select a brush first".into();
            return;
        }
        let args = Clip {
            ids,
            plane,
            keep: self.clip.keep,
            material: String::new(),
        };
        if let Some(done) = self.call::<_, CreatedMapped>("brush.clip", &args, None) {
            self.clip.clear();
            if self.clip.keep == Keep::Both && !done.ids.is_empty() {
                self.select("select.add", done.ids);
            }
        }
    }

    fn refresh_clip_preview(&mut self) {
        let plane = (self.tool == Tool::Clip)
            .then(|| self.clip.plane())
            .flatten();
        let key = plane.map(|p| {
            let bits = p.map(|v| v.to_array().map(f64::to_bits));
            let flat: Vec<u64> = bits.into_iter().flatten().collect();
            (
                self.remote.revision,
                flat.try_into().expect("nine coordinates"),
                self.remote.selection.iter().copied().collect(),
            )
        });
        if key == self.clip_key {
            return;
        }
        self.clip_preview = match plane {
            Some(p) => clip::preview(&self.remote.doc, &self.remote.selection, p),
            None => Preview::default(),
        };
        self.clip_key = key;
        self.repaint = true;
    }

    fn set_hidden_selection(&mut self, hidden: bool) {
        let ids = self.selection_ids();
        if !ids.is_empty() {
            self.call::<_, Updated>("object.set_hidden", &SetHidden { ids, hidden }, None);
        }
    }

    fn lock_selection(&mut self) {
        let ids = self.selection_ids();
        if ids.is_empty() {
            return;
        }
        let doc = &self.remote.doc;
        let locked = !ids
            .iter()
            .all(|&id| doc.own_flags(id).is_some_and(|f| f.locked));
        self.call::<_, Updated>("object.set_locked", &SetLocked { ids, locked }, None);
    }

    fn pick_target(&self, id: ObjectId) -> ObjectId {
        self.remote.doc.pick_target(id)
    }

    fn copy_selection(&mut self, ctx: &Context) -> bool {
        let ids = self.selection_ids();
        if ids.is_empty() {
            return false;
        }
        let Some(state) =
            self.call::<_, ClipboardState>("clipboard.copy", &ClipboardCopy { ids }, None)
        else {
            return false;
        };
        if let Some(text) = state.text {
            ctx.copy_text(text.clone());
            self.clip_text = Some(text);
        }
        self.status = format!("copied {} objects", state.count);
        true
    }

    fn cut_selection(&mut self, ctx: &Context) {
        if self.copy_selection(ctx) {
            self.delete_selection();
        }
    }

    /// Half the size of what the clipboard text holds, to rest it on a surface instead of sinking it in.
    fn clip_half_size(text: &str) -> Option<DVec3> {
        let doc = editor_fmt::read(text).ok()?;
        let ids = doc
            .brushes
            .keys()
            .map(|&i| ObjectId::from(i))
            .chain(doc.meshes.keys().map(|&i| ObjectId::from(i)))
            .chain(doc.entities.keys().map(|&i| ObjectId::from(i)));
        ids.filter_map(|id| editor_cmd::extent(&doc, id))
            .reduce(|a, b| a.merge(b))
            .map(|b| (b.max - b.min) * 0.5)
    }

    fn paste(&mut self, request: PasteRequest, at: Option<(DVec3, Option<DVec3>)>) {
        let mut args = serde_json::Map::new();
        let external = request.os_text.filter(|t| {
            self.clip_text.as_ref() != Some(t) && t.trim_start().starts_with("format = ")
        });
        if let Some(text) = external {
            args.insert("text".into(), Value::String(text));
        }
        if !request.in_place {
            let at = at.map(|(point, normal)| match normal {
                Some(n) => {
                    let text = args
                        .get("text")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .or_else(|| self.server_clipboard_text());
                    let reach = text
                        .and_then(|t| Self::clip_half_size(&t))
                        .map_or(0.0, |h| n.abs().dot(h));
                    point + n * reach
                }
                None => point,
            });
            let (key, v) = match at {
                Some(p) => ("at", serde_json::json!([p.x, p.y, p.z])),
                None => ("offset", serde_json::json!([self.grid, self.grid, 0.0])),
            };
            args.insert(key.into(), v);
        }
        let Some(success) = self.send("clipboard.paste", Value::Object(args), None) else {
            return;
        };
        match serde_json::from_value::<Pasted>(success.result) {
            Ok(p) => {
                let top: Vec<ObjectId> = p.ids.into_iter().take(p.top_level).collect();
                self.status = format!("pasted {} objects", top.len());
                self.select("select.set", top);
            }
            Err(e) => self.status = format!("clipboard.paste: unexpected result: {e}"),
        }
    }

    fn server_clipboard_text(&mut self) -> Option<String> {
        let success = self
            .remote
            .call(Request::new("clipboard.get", serde_json::json!({})))
            .ok()?;
        serde_json::from_value::<ClipboardState>(success.result)
            .ok()?
            .text
    }

    fn duplicate_selection(&mut self) {
        let ids = self.selection_ids();
        if ids.is_empty() {
            return;
        }
        let args = serde_json::json!({ "ids": ids, "offset": [self.grid, self.grid, 0.0] });
        let Some(success) = self.send("object.duplicate", args, None) else {
            return;
        };
        if let Ok(d) = serde_json::from_value::<Duplicated>(success.result) {
            let copies: Vec<ObjectId> = d.ids.into_iter().take(ids.len()).collect();
            self.select("select.set", copies);
        }
    }

    fn delete_selection(&mut self) {
        let ids = self.selection_ids();
        if !ids.is_empty() {
            self.call::<_, Deleted>("object.delete", &Delete { ids }, None);
        }
    }

    fn save(&mut self) {
        let args = Save {
            path: None,
            overwrite: false,
        };
        if self.call::<_, Saved>("file.save", &args, None).is_some() {
            self.status = format!(
                "saved {}",
                self.remote.file.path.as_deref().unwrap_or("(unnamed)")
            );
        }
    }

    fn open(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("scene", &["toml"])
            .pick_file()
        else {
            return;
        };
        match self.remote.attach(Some(&path)) {
            Ok(_) => {
                self.status.clear();
                self.grid = self.remote.doc.editor.grid;
                self.reset_cameras();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    fn frame(&mut self) {
        match self.selection_bounds() {
            Some(bounds) => self.frame_bounds(bounds),
            None => self.reset_cameras(),
        }
    }

    fn frame_bounds(&mut self, bounds: Aabb) {
        for pane in self.panes.values_mut() {
            if let Some(camera) = &pane.camera {
                pane.camera = Some(frame_camera(
                    camera,
                    &bounds,
                    pane.want_px.0,
                    pane.want_px.1,
                ));
            }
        }
        self.relink = true;
    }

    fn sync_zoom(&mut self) {
        if !self.link_zoom {
            return;
        }
        let zooms: Vec<(PaneId, f64)> = PaneId::ALL
            .iter()
            .filter_map(|&id| {
                let camera = self.panes.get(&id)?.camera.as_ref()?;
                nav::ortho_view(camera)?;
                Some((id, nav::zoom_of(camera)))
            })
            .collect();
        let target = if self.relink {
            if self.panes.values().any(|p| p.visible && p.camera.is_none()) {
                return;
            }
            self.relink = false;
            zooms.iter().map(|z| z.1).fold(f64::INFINITY, f64::min)
        } else {
            let moved = zooms.iter().find(|(id, z)| {
                self.last_zoom
                    .get(id)
                    .is_some_and(|last| (last - z).abs() > 1e-9)
            });
            match moved {
                Some(&(_, zoom)) => zoom,
                None => {
                    self.last_zoom = zooms.into_iter().collect();
                    return;
                }
            }
        };
        if !target.is_finite() {
            return;
        }
        for (id, _) in &zooms {
            if let Some(camera) = self.panes.get_mut(id).and_then(|p| p.camera.as_mut()) {
                nav::set_ortho_zoom(camera, target);
            }
        }
        self.repaint = true;
        self.last_zoom = zooms.iter().map(|(id, _)| (*id, target)).collect();
    }

    fn reset_cameras(&mut self) {
        for pane in self.panes.values_mut() {
            pane.reset_camera();
        }
        self.relink = true;
    }

    fn set_layout(&mut self, layout: Layout) {
        self.saved_dock = None;
        self.dock = build_dock(layout);
    }

    fn toggle_maximize(&mut self) {
        if let Some(saved) = self.saved_dock.take() {
            self.dock = saved;
        } else if let Some(id) = self.active {
            self.saved_dock = Some(self.dock.clone());
            self.dock = DockState::new(vec![id]);
        }
    }

    fn prep_key(&self) -> PrepKey {
        (
            self.remote.revision,
            self.remote.selection.iter().copied().collect(),
            self.hover,
            self.remote.doc.open_group(),
            self.remote.face_selection.iter().copied().collect(),
            self.texture_generation,
            self.remote.defs_generation,
            self.show_links,
        )
    }

    fn refresh_prepared(&mut self, renderer: &Renderer) {
        let key = self.prep_key();
        if self.prepared.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let hidden: HashSet<ObjectId> = self.remote.doc.hidden_ids().into_iter().collect();
        let tinted = self.remote.doc.with_members(&self.remote.selection);
        let scene_dir = self
            .remote
            .file
            .path
            .as_deref()
            .and_then(|p| Path::new(p).parent());
        let links = if self.show_links {
            doc_links(&self.remote.doc, &self.remote.catalog)
        } else {
            Vec::new()
        };
        let looks = component_looks(&self.remote.catalog);
        let env = SceneEnv {
            scene_dir,
            textures: renderer.textures(),
            faces: &self.remote.face_selection,
            looks: &looks,
            links: &links,
        };
        let scene = PreparedScene::new(&self.remote.doc, &tinted, self.hover, &hidden, &env);
        self.links = doc_links(&self.remote.doc, &self.remote.catalog);
        self.component_looks = looks;
        self.prepared = Some((key, scene));
        self.repaint = true;
    }

    /// Starts the game client, which sits next to this binary, on the current session.
    fn play(&mut self) {
        let exe = match std::env::current_exe() {
            Ok(p) => p.with_file_name("editor-game"),
            Err(e) => {
                self.status = format!("cannot find editor-game: {e}");
                return;
            }
        };
        match std::process::Command::new(&exe)
            .arg("--session")
            .arg(&self.remote.session)
            .spawn()
        {
            Ok(mut child) => {
                self.status = "started the game client".into();
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(e) => self.status = format!("cannot start {}: {e}", exe.display()),
        }
    }

    fn keys(&mut self, ctx: &Context) {
        if ctx.text_edit_focused() {
            return;
        }
        if ctx.input(|i| i.key_pressed(Key::F5)) {
            self.play();
        }
        let keys = ctx.input(Keys::read);
        if keys.copy {
            self.copy_selection(ctx);
        }
        if keys.cut {
            self.cut_selection(ctx);
        }
        if keys.paste {
            self.paste_pending = Some(PasteRequest {
                os_text: keys.paste_text.clone(),
                in_place: keys.paste_in_place,
            });
        }
        if keys.duplicate {
            self.duplicate_selection();
        }
        if keys.undo {
            self.undo();
        }
        if keys.redo {
            self.redo();
        }
        if self.tool == Tool::Surface && !self.remote.face_selection.is_empty() {
            let faces = self.face_refs();
            let step = if keys.shift { 1.0 } else { self.grid };
            if let Some((dx, dy)) = keys.arrow {
                let args = SetUv {
                    faces: faces.clone(),
                    offset: Some(glam::DVec2::new(dx * step, dy * step)),
                    scale: None,
                    rotation: None,
                    mode: Some(UvMode::Relative),
                };
                self.call::<_, FacesChanged>("face.set_uv", &args, None);
            }
            if keys.page != 0.0 {
                let args = SetUv {
                    faces,
                    offset: None,
                    scale: None,
                    rotation: Some(keys.page),
                    mode: Some(UvMode::Relative),
                };
                self.call::<_, FacesChanged>("face.set_uv", &args, None);
            }
        }
        if self.tool == Tool::Clip && keys.backspace && !self.clip.points.is_empty() {
            self.clip.pop();
        } else if keys.delete {
            self.delete_selection();
        }
        if self.tool == Tool::Clip {
            if keys.enter {
                self.clip_apply();
            }
            if keys.reverse {
                self.clip.reverse();
            }
            if keys.tab {
                self.clip.cycle_keep();
            }
            if keys.escape {
                self.clip.clear();
            }
        }
        if keys.space {
            self.toggle_maximize();
        }
        if keys.frame {
            self.frame();
        }
        if keys.snap {
            self.snap = !self.snap;
        }
        if keys.select_all {
            self.select_kind("select.all");
        }
        if keys.select_invert {
            self.select_kind("select.invert");
        }
        if keys.select_none {
            self.call::<_, IdCount>("select.clear", &NoArgs::default(), None);
        }
        if keys.hide {
            self.set_hidden_selection(true);
        }
        if keys.isolate {
            let ids = self.selection_ids();
            if !ids.is_empty() {
                self.call::<_, Updated>("object.isolate", &Isolate { ids }, None);
            }
        }
        if keys.show_all {
            self.call::<_, Updated>("object.show_all", &NoArgs::default(), None);
        }
        if keys.lock {
            self.lock_selection();
        }
        if keys.unlock_all {
            self.call::<_, Updated>("object.unlock_all", &NoArgs::default(), None);
        }
        if keys.escape && !self.dragging && self.remote.doc.open_group().is_some() {
            self.call::<_, OpenGroups>("group.close", &CloseGroup::default(), None);
        }
        if keys.rotate_ccw {
            self.rotate_selection(90.0);
        }
        if keys.rotate_cw {
            self.rotate_selection(-90.0);
        }
        if let Some(axis) = keys.flip {
            self.flip_selection(axis);
        }
        if let Some(tool) = keys.tool {
            self.tool = tool;
        }
        let at = GRIDS
            .iter()
            .position(|&g| g == self.grid)
            .unwrap_or(DEFAULT_GRID);
        if keys.grid_finer && at > 0 {
            self.grid = GRIDS[at - 1];
        }
        if keys.grid_coarser && at + 1 < GRIDS.len() {
            self.grid = GRIDS[at + 1];
        }
    }

    fn undo(&mut self) {
        self.call::<_, Stepped>("history.undo", &NoArgs::default(), None);
    }

    fn redo(&mut self) {
        self.call::<_, Stepped>("history.redo", &NoArgs::default(), None);
    }

    pub fn ui(&mut self, ui: &mut Ui, renderer: &Renderer) {
        let ctx = ui.ctx().clone();
        for pane in self.panes.values_mut() {
            pane.visible = false;
        }
        match self.remote.pump() {
            Ok(true) => self.repaint = true,
            Ok(false) => {}
            Err(e) => self.status = e.to_string(),
        }
        if let Err(e) = self.remote.refresh_defs() {
            self.status = e.to_string();
        }
        if self.remote.closed {
            self.status = "disconnected from editor-server".into();
        }
        self.refresh_prepared(renderer);
        self.keys(&ctx);
        self.refresh_clip_preview();
        self.check_recovery();
        self.menu(ui);
        self.recovery_banner(ui);
        self.toolbar(ui);
        self.status_bar(ui);
        self.outliner(ui);
        self.inspector(ui, renderer);
        self.help(&ctx);

        let paste = self.paste_pending.take();
        let selection = self.remote.selection.clone();
        let sel_bounds = self.selection_bounds();
        let revision = self.remote.revision;
        let Some((_, prepared)) = self.prepared.as_ref() else {
            return;
        };
        let pivot = prepared.bounds().map_or(DVec3::ZERO, |b| b.center());
        let mut hover_next = None;
        let mut active = None;
        let mut actions = Vec::new();
        let mut pick_ms = 0.0;
        let mut pending_next = None;
        {
            let env = PaneEnv {
                prepared,
                renderer,
                revision,
                tool: self.tool,
                grid: if self.snap { self.grid } else { 0.0 },
                selection: &selection,
                sel_bounds,
                doc: &self.remote.doc,
                depth_hint: sel_bounds.or(self.last_box),
                picks: &self.vertex_picks,
                clip: &self.clip,
                clip_preview: &self.clip_preview,
                pending: self.pending_box,
                pending_next: &mut pending_next,
                actions: &mut actions,
                hover_next: &mut hover_next,
                active: &mut active,
                pivot,
                pick_ms: &mut pick_ms,
                want_paste_point: paste.as_ref().is_some_and(|p| !p.in_place),
            };
            let mut viewer = Viewer {
                panes: &mut self.panes,
                env,
            };
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    let mut style = egui_dock::Style::from_egui(ui.style());
                    style.tab_bar.height = 24.0;
                    DockArea::new(&mut self.dock)
                        .style(style)
                        .show_close_buttons(false)
                        .show_leaf_close_all_buttons(false)
                        .show_leaf_collapse_buttons(false)
                        .show_inside(ui, &mut viewer);
                });
        }
        if let Some(request) = paste {
            let at = actions.iter().find_map(|a| match a {
                Action::PasteAt { point, normal } => Some((*point, *normal)),
                _ => None,
            });
            actions.retain(|a| !matches!(a, Action::PasteAt { .. }));
            self.paste(request, at);
        }
        self.sync_zoom();
        if pending_next != self.pending_box {
            self.pending_box = pending_next;
            self.repaint = true;
        }
        Timings::blend(&mut self.timings.pick, pick_ms);
        if active.is_some() {
            self.active = active;
        }
        if hover_next != self.hover && !self.dragging {
            self.hover = hover_next;
        }
        self.actions.extend(actions);
    }

    pub fn finish_frame(&mut self, gfx: &mut Gfx) {
        let actions = std::mem::take(&mut self.actions);
        for action in actions {
            self.apply(action);
        }
        self.refresh_prepared(&gfx.renderer);
        let Some((_, prepared)) = self.prepared.as_ref() else {
            return;
        };
        let options = ViewportOptions {
            grid: self.show_grid.then_some(self.grid),
            axes: self.show_axes,
        };
        for pane in self.panes.values_mut().filter(|p| p.visible) {
            let want = pane.want_px;
            if want.0 > 8192 || want.1 > 8192 {
                continue;
            }
            if pane.target.as_ref().is_none_or(|t| t.size != want) {
                if let Some(old) = pane.target.take() {
                    gfx.free_target(old);
                }
                pane.target = Some(gfx.create_target(want));
                self.repaint = true;
            }
            let (Some(camera), Some(target)) = (&pane.camera, &pane.target) else {
                continue;
            };
            let view = prepared.view(camera, want.0, want.1);
            gfx.renderer
                .draw_viewport(prepared, &view, options, want, &target.view);
        }
    }

    pub fn title(&self) -> String {
        let file = &self.remote.file;
        format!(
            "{}{} - Editor",
            if file.name.is_empty() {
                "untitled"
            } else {
                &file.name
            },
            if file.dirty { " *" } else { "" }
        )
    }

    fn check_recovery(&mut self) {
        let path = self.remote.file.path.clone();
        if self.recovery_for.as_ref() == Some(&path) {
            return;
        }
        self.recovery_for = Some(path.clone());
        self.recovery.clear();
        let Some(path) = path else { return };
        let Ok(success) = self
            .remote
            .call(Request::new("recovery.list", serde_json::json!({})))
        else {
            return;
        };
        if let Ok(list) = serde_json::from_value::<RecoveryList>(success.result) {
            self.recovery = list
                .entries
                .into_iter()
                .filter(|e| {
                    !e.live && e.newer_than_source && e.source_path.as_deref() == Some(&path)
                })
                .collect();
        }
    }

    fn recovery_banner(&mut self, ui: &mut Ui) {
        let Some(entry) = self.recovery.first().cloned() else {
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let age = now.saturating_sub(entry.saved_at);
        let ago = match age {
            0..=89 => format!("{age} s"),
            90..=5399 => format!("{} min", age / 60),
            5400..=129_599 => format!("{} h", age / 3600),
            _ => format!("{} days", age / 86_400),
        };
        let mut action = None;
        egui::Panel::top("recovery").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Unsaved changes from {ago} ago were recovered for this scene (revision {}).",
                    entry.revision
                ));
                if ui.button("Recover").clicked() {
                    action = Some("recovery.restore");
                }
                if ui.button("Discard").clicked() {
                    action = Some("recovery.discard");
                }
                if ui.button("Later").clicked() {
                    action = Some("later");
                }
            });
        });
        match action {
            Some("later") => self.recovery.clear(),
            Some(op)
                if self
                    .send(op, serde_json::json!({ "id": entry.id }), None)
                    .is_some() =>
            {
                self.status = if op == "recovery.restore" {
                    "recovered the autosave; save to keep it".into()
                } else {
                    "discarded the autosave".into()
                };
                self.recovery.clear();
            }
            Some(_) => {}
            None => {}
        }
    }

    fn menu(&mut self, ui: &mut Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open…").clicked() {
                        self.open();
                        ui.close();
                    }
                    if ui.button("Save").clicked() {
                        self.save();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        self.quit = true;
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui
                        .add(egui::Button::new("Undo").shortcut_text("Ctrl+Z"))
                        .clicked()
                    {
                        self.undo();
                        ui.close();
                    }
                    if ui
                        .add(egui::Button::new("Redo").shortcut_text("Ctrl+Shift+Z"))
                        .clicked()
                    {
                        self.redo();
                        ui.close();
                    }
                    ui.separator();
                    let ctx = ui.ctx().clone();
                    if ui
                        .add(egui::Button::new("Cut").shortcut_text("Ctrl+X"))
                        .clicked()
                    {
                        self.cut_selection(&ctx);
                        ui.close();
                    }
                    if ui
                        .add(egui::Button::new("Copy").shortcut_text("Ctrl+C"))
                        .clicked()
                    {
                        self.copy_selection(&ctx);
                        ui.close();
                    }
                    for (name, shortcut, in_place) in [
                        ("Paste", "Ctrl+V", false),
                        ("Paste in place", "Ctrl+Shift+V", true),
                    ] {
                        if ui
                            .add(egui::Button::new(name).shortcut_text(shortcut))
                            .clicked()
                        {
                            self.paste_pending = Some(PasteRequest {
                                os_text: None,
                                in_place,
                            });
                            ui.close();
                        }
                    }
                    if ui
                        .add(egui::Button::new("Duplicate").shortcut_text("Ctrl+D"))
                        .clicked()
                    {
                        self.duplicate_selection();
                        ui.close();
                    }
                    ui.separator();
                    for (name, shortcut, degrees) in
                        [("Rotate 90° CCW", ".", 90.0), ("Rotate 90° CW", ",", -90.0)]
                    {
                        if ui
                            .add(egui::Button::new(name).shortcut_text(shortcut))
                            .clicked()
                        {
                            self.rotate_selection(degrees);
                            ui.close();
                        }
                    }
                    for (name, shortcut, axis) in [
                        ("Flip X", "Alt+X", Axis::X),
                        ("Flip Y", "Alt+Y", Axis::Y),
                        ("Flip Z", "Alt+Z", Axis::Z),
                    ] {
                        if ui
                            .add(egui::Button::new(name).shortcut_text(shortcut))
                            .clicked()
                        {
                            self.flip_selection(axis);
                            ui.close();
                        }
                    }
                    ui.separator();
                    for (name, shortcut, op) in [
                        ("Select all", "Ctrl+A", Some("select.all")),
                        ("Invert selection", "Ctrl+I", Some("select.invert")),
                        ("Select none", "Ctrl+Shift+A", None),
                    ] {
                        if ui
                            .add(egui::Button::new(name).shortcut_text(shortcut))
                            .clicked()
                        {
                            match op {
                                Some(op) => self.select_kind(op),
                                None => {
                                    self.call::<_, IdCount>(
                                        "select.clear",
                                        &NoArgs::default(),
                                        None,
                                    );
                                }
                            }
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui
                        .add(egui::Button::new("Delete").shortcut_text("Del"))
                        .clicked()
                    {
                        self.delete_selection();
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.menu_button("Layout", |ui| {
                        for (name, layout) in [
                            ("1 pane", Layout::One),
                            ("2 panes", Layout::Two),
                            ("3 panes", Layout::Three),
                            ("4 panes", Layout::Four),
                        ] {
                            if ui.button(name).clicked() {
                                self.set_layout(layout);
                                ui.close();
                            }
                        }
                    });
                    if ui
                        .add(egui::Button::new("Maximize pane").shortcut_text("Space"))
                        .clicked()
                    {
                        self.toggle_maximize();
                        ui.close();
                    }
                    if ui
                        .add(egui::Button::new("Frame selection or all").shortcut_text("F"))
                        .clicked()
                    {
                        self.frame();
                        ui.close();
                    }
                    ui.separator();
                    ui.checkbox(&mut self.link_zoom, "Link 2D zoom");
                    ui.checkbox(&mut self.show_grid, "Grid");
                    ui.checkbox(&mut self.show_axes, "Axes");
                    ui.checkbox(&mut self.show_links, "Entity links");
                    ui.separator();
                    ui.menu_button("Theme", |ui| {
                        for t in Theme::ALL {
                            if ui.radio_value(&mut self.theme, t, t.name()).clicked() {
                                theme::apply(ui.ctx(), t);
                            }
                        }
                    });
                });
                if ui
                    .add(egui::Button::new("Play").shortcut_text("F5"))
                    .on_hover_text("Open the scene in the game client")
                    .clicked()
                {
                    self.play();
                }
                ui.menu_button("Help", |ui| {
                    if ui.button("Controls").clicked() {
                        self.show_help = true;
                        ui.close();
                    }
                });
            });
        });
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                for (tool, key) in [
                    (Tool::Select, "1"),
                    (Tool::Move, "2"),
                    (Tool::Box, "3"),
                    (Tool::Rotate, "4"),
                    (Tool::Scale, "5"),
                    (Tool::Vertex, "6"),
                    (Tool::Clip, "7"),
                    (Tool::Edge, "8"),
                    (Tool::Face, "9"),
                    (Tool::Surface, "0"),
                ] {
                    ui.selectable_value(&mut self.tool, tool, format!("{}  {key}", tool.name()));
                }
                ui.separator();
                ui.add_enabled(false, egui::Button::new("Extrude"))
                    .on_disabled_hover_text("Not in the spike");
                if self.tool == Tool::Clip {
                    ui.separator();
                    for (keep, name) in [
                        (Keep::Front, "Front"),
                        (Keep::Back, "Back"),
                        (Keep::Both, "Both"),
                    ] {
                        ui.selectable_value(&mut self.clip.keep, keep, name);
                    }
                    if ui
                        .add_enabled(self.clip.plane().is_some(), egui::Button::new("Apply"))
                        .on_hover_text("Enter")
                        .clicked()
                    {
                        self.clip_apply();
                    }
                    if ui.button("Clear").on_hover_text("Esc").clicked() {
                        self.clip.clear();
                    }
                }
                ui.separator();
                ui.label("Grid");
                egui::ComboBox::from_id_salt("grid")
                    .selected_text(format!("{}", self.grid))
                    .width(56.0)
                    .show_ui(ui, |ui| {
                        for g in GRIDS {
                            ui.selectable_value(&mut self.grid, g, format!("{g}"));
                        }
                    });
                ui.toggle_value(&mut self.snap, "Snap")
                    .on_hover_text("Snap to the grid (\\)");
                ui.toggle_value(&mut self.texture_lock, "Tex lock")
                    .on_hover_text("Keep textures attached to the geometry when it moves");
            });
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(self.tool.name());
                ui.separator();
                ui.label(if self.snap {
                    format!("grid {}", self.grid)
                } else {
                    format!("grid {} (snap off)", self.grid)
                });
                ui.separator();
                let cursor = self
                    .active
                    .and_then(|a| self.panes.get(&a))
                    .and_then(|p| p.cursor.map(|c| format!("{:.1}, {:.1}", c.x, c.y)));
                ui.monospace(cursor.unwrap_or_else(|| "—".into()));
                ui.separator();
                if let Some(g) = self.remote.doc.open_group() {
                    ui.label(format!("editing {g} (Esc closes)"));
                    ui.separator();
                }
                ui.label(match self.hover {
                    Some(id) => format!("hover {id}"),
                    None => "hover —".into(),
                });
                ui.separator();
                ui.label(format!(
                    "{} selected   rev {}{}",
                    self.remote.selection.len(),
                    self.remote.revision,
                    if self.remote.file.dirty { " *" } else { "" }
                ));
                if let Some((holder, label)) = self.remote.busy_holder() {
                    ui.separator();
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("{holder} has a transaction open ({label}): edits are refused until it ends"),
                    );
                }
                if !self.status.is_empty() {
                    ui.separator();
                    ui.colored_label(ui.visuals().warn_fg_color, &self.status);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let t = self.timings;
                    ui.weak(format!(
                        "ui {:.1}  panes {:.1}  pick {:.1}  present {:.1}  frame {:.1} ms   {}",
                        t.ui, t.panes, t.pick, t.present, t.frame, self.adapter
                    ));
                });
            });
        });
    }

    fn outliner(&mut self, ui: &mut Ui) {
        let mut events = Vec::new();
        egui::Panel::left("outliner")
            .default_size(200.0)
            .size_range(140.0..=320.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.left_tab, LeftTab::Outliner, "Outliner");
                    ui.selectable_value(&mut self.left_tab, LeftTab::Entities, "Entities");
                });
                ui.separator();
                events = match self.left_tab {
                    LeftTab::Outliner => self.panels.outliner(
                        ui,
                        &self.remote.doc,
                        self.remote.revision,
                        &self.remote.selection,
                    ),
                    LeftTab::Entities => {
                        self.panels
                            .browser(ui, &self.remote.catalog, &self.remote.defs)
                    }
                };
            });
        for event in events {
            self.panel_event(event);
        }
    }

    fn inspector(&mut self, ui: &mut Ui, renderer: &Renderer) {
        let mut events = Vec::new();
        let mut surface_events = Vec::new();
        let scene_dir = self
            .remote
            .file
            .path
            .as_deref()
            .and_then(|p| Path::new(p).parent());
        egui::Panel::right("inspector")
            .default_size(270.0)
            .size_range(220.0..=460.0)
            .show(ui, |ui| {
                ui.heading("Inspector");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.remote.face_selection.is_empty() {
                        events = self.panels.inspector(
                            ui,
                            &self.remote.doc,
                            &self.remote.selection,
                            &self.remote.catalog,
                            &self.links,
                        );
                    } else {
                        self.surface.faces(
                            ui,
                            &self.remote.doc,
                            &self.remote.face_selection,
                            &mut surface_events,
                        );
                    }
                    ui.add_space(8.0);
                    self.surface.materials(
                        ui,
                        &self.remote.doc,
                        renderer.textures(),
                        scene_dir,
                        &mut surface_events,
                    );
                });
            });
        for event in events {
            self.panel_event(event);
        }
        for event in surface_events {
            self.surface_event(event);
        }
    }

    fn surface_event(&mut self, event: surface::Event) {
        match event {
            surface::Event::SetUv { args, merge } => {
                self.call::<_, FacesChanged>("face.set_uv", &args, merge.as_deref());
            }
            surface::Event::Align(args) => {
                self.call::<_, FacesChanged>("face.align_uv", &args, None);
            }
            surface::Event::WrapFromFirst => {
                let mut faces = self.face_refs().into_iter();
                if let Some(from) = faces.next() {
                    let args = TransferUv {
                        from,
                        to: faces.collect(),
                        mode: Some(TransferMode::Wrap),
                    };
                    self.call::<_, FacesChanged>("face.transfer_uv", &args, None);
                }
            }
            surface::Event::Assign(material) => {
                let faces = if self.remote.face_selection.is_empty() {
                    self.remote
                        .selection
                        .iter()
                        .filter(|id| matches!(id, ObjectId::Brush(_) | ObjectId::Mesh(_)))
                        .map(|&object| ElementRef {
                            object,
                            part: Part::All,
                        })
                        .collect()
                } else {
                    self.face_refs()
                };
                if !faces.is_empty() {
                    let args = SetMaterial { faces, material };
                    self.call::<_, FacesChanged>("face.set_material", &args, None);
                }
            }
            surface::Event::CreateMaterial(args) => {
                self.call::<_, Created>("material.create", &args, None);
            }
            surface::Event::SetMaterial(args) => {
                self.call::<_, IdOnly>("material.set", &args, None);
                self.texture_generation += 1;
            }
            surface::Event::RenameMaterial(args) => {
                self.call::<_, FacesChanged>("material.rename", &args, None);
            }
            surface::Event::DeleteMaterial(args) => {
                self.call::<_, Deleted>("material.delete", &args, None);
            }
            surface::Event::EmbedMaterial(material) => {
                let args = editor_cmd::args::Embed {
                    materials: vec![material],
                };
                self.call::<_, Updated>("material.embed", &args, None);
            }
        }
    }

    fn panel_event(&mut self, event: Event) {
        match event {
            Event::Select { id, additive } => self.apply(Action::Select {
                id: Some(id),
                additive,
            }),
            Event::Frame(id) => {
                if let Some(bounds) = editor_cmd::extent(&self.remote.doc, id) {
                    self.frame_bounds(bounds);
                }
            }
            Event::Rename { id, name } => {
                self.call::<_, IdOnly>("object.rename", &Rename { id, name }, None);
            }
            Event::SetLayer { ids, layer } => {
                self.call::<_, Moved>("object.set_layer", &SetLayer { ids, layer }, None);
            }
            Event::SetHidden { id, hidden } => {
                let args = SetHidden {
                    ids: vec![id],
                    hidden,
                };
                self.call::<_, Updated>("object.set_hidden", &args, None);
            }
            Event::SetOmitFromExport { id, omit } => {
                self.call::<_, IdOnly>(
                    "layer.set_omit_from_export",
                    &SetOmitFromExport { id, omit },
                    None,
                );
            }
            Event::SetLocked { id, locked } => {
                let args = SetLocked {
                    ids: vec![id],
                    locked,
                };
                self.call::<_, Updated>("object.set_locked", &args, None);
            }
            Event::SetTransform {
                id,
                origin,
                angles,
                members,
                merge,
            } => {
                let args = SetTransform {
                    id,
                    origin,
                    angles,
                    members,
                };
                self.call::<_, IdOnly>("entity.set_transform", &args, merge.as_deref());
            }
            Event::SetComponent { id, name, data } => {
                let args = SetComponent {
                    ids: vec![id],
                    name,
                    data,
                };
                self.call::<_, Updated>("entity.set_component", &args, None);
            }
            Event::AddComponent { ids, name } => {
                let args = AddComponent {
                    ids,
                    name,
                    data: None,
                };
                self.call::<_, Updated>("entity.add_component", &args, None);
            }
            Event::RemoveComponent { id, name } => {
                let args = RemoveComponent {
                    ids: vec![id],
                    name,
                };
                self.call::<_, Updated>("entity.remove_component", &args, None);
            }
            Event::PlaceComponent { name, point } => self.place_component(name, point),
            Event::EntityFromSelection => self.entity_from_selection(None),
            Event::ReloadDefs => self.remote.reload_defs(),
            Event::AddDefsFile => {
                let Some(path) = rfd::FileDialog::new()
                    .add_filter("entity definitions", &["json"])
                    .pick_file()
                else {
                    return;
                };
                let args = AddDefs::File {
                    path: path.to_string_lossy().into_owned(),
                    store: None,
                };
                self.call::<_, Created>("component_defs.add", &args, None);
            }
            Event::EmbedDefs(id) => {
                self.call::<_, Updated>("component_defs.embed", &EmbedDefs { ids: vec![id] }, None);
            }
            Event::RemoveDefs(id) => {
                self.call::<_, Deleted>(
                    "component_defs.remove",
                    &RemoveDefs { ids: vec![id] },
                    None,
                );
            }
        }
    }

    fn snapped_center(&self) -> DVec3 {
        let center = self
            .selection_bounds()
            .or_else(|| {
                self.prepared
                    .as_ref()
                    .and_then(|(_, p)| p.bounds().copied())
            })
            .map_or(DVec3::ZERO, |b| b.center());
        DVec3::new(
            crate::pane::snap(center.x, self.grid),
            crate::pane::snap(center.y, self.grid),
            crate::pane::snap(center.z, self.grid),
        )
    }

    fn world_geometry_selection(&self) -> Vec<ObjectId> {
        self.remote
            .selection
            .iter()
            .copied()
            .filter(|id| {
                self.remote.doc.get(*id).is_some_and(|o| {
                    o.entity().is_none()
                        && matches!(id.kind(), ObjectKind::Brush | ObjectKind::Mesh)
                })
            })
            .collect()
    }

    fn component_data(&self, name: &str) -> serde_json::Map<String, Value> {
        self.remote.catalog.defaults(name)
    }

    /// Adds a component to the selected entities; failing that, makes an entity of the selected world geometry, or places a point entity.
    fn place_component(&mut self, name: String, point: Option<DVec3>) {
        if point.is_none() {
            let entities: Vec<ObjectId> = self
                .remote
                .selection
                .iter()
                .copied()
                .filter(|id| matches!(id, ObjectId::Entity(_)))
                .collect();
            if !entities.is_empty() {
                let args = AddComponent {
                    ids: entities,
                    name,
                    data: None,
                };
                self.call::<_, Updated>("entity.add_component", &args, None);
                return;
            }
            if !self.world_geometry_selection().is_empty() {
                self.entity_from_selection(Some(name));
                return;
            }
        }
        let origin = point.unwrap_or_else(|| self.snapped_center());
        let components = [(name.clone(), self.component_data(&name))].into();
        let args = CreateEntity {
            origin,
            angles: None,
            components: Some(components),
            layer: None,
            group: None,
        };
        let created = self.call::<_, Created>("entity.create", &args, None);
        if let Some(id) = created.and_then(|c| c.id) {
            self.select("select.set", vec![id]);
        }
    }

    /// Turns the selected world brushes and meshes into one entity, optionally with a component.
    fn entity_from_selection(&mut self, component: Option<String>) {
        let objects = self.world_geometry_selection();
        if objects.is_empty() {
            self.status = "select world brushes or meshes first".into();
            return;
        }
        let components = component.map(|name| [(name.clone(), self.component_data(&name))].into());
        let args = CreateBrushEntity {
            objects,
            origin: None,
            angles: None,
            components,
        };
        let created = self.call::<_, Created>("entity.create_brush_entity", &args, None);
        if let Some(id) = created.and_then(|c| c.id) {
            self.select("select.set", vec![id]);
        }
    }

    fn help(&mut self, ctx: &Context) {
        let mut open = self.show_help;
        egui::Window::new("Controls")
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("controls").striped(true).show(ui, |ui| {
                    for (k, v) in [
                        ("Perspective", ""),
                        ("RMB drag", "look; with W A S D Q E fly (Shift faster)"),
                        ("MMB drag", "pan"),
                        ("Alt + LMB drag", "orbit the scene"),
                        ("Wheel", "dolly"),
                        ("2D views", ""),
                        ("MMB / RMB drag", "pan"),
                        ("Wheel", "zoom at cursor"),
                        ("Box tool", "LMB drag a rectangle"),
                        ("Move tool", "LMB drag a selected object"),
                        ("Everywhere", ""),
                        ("LMB click", "select (Shift adds or removes)"),
                        (
                            "Ctrl + LMB click",
                            "select the next object along the ray (Shift adds)",
                        ),
                        ("Shift + LMB drag", "paint-select (Select tool)"),
                        ("F5", "open the scene in the game client"),
                        ("Ctrl+A / Ctrl+I", "select all / invert"),
                        ("Ctrl+H / Alt+H", "hide the selection / show all"),
                        ("Ctrl+Shift+H", "isolate the selection"),
                        (
                            "Ctrl+L / Alt+L",
                            "lock or unlock the selection / unlock all",
                        ),
                        (
                            "Double-click",
                            "open the group under the cursor to edit its members",
                        ),
                        ("1 … 9", "Select, Move, Box, Rotate, Scale, Vertex, Clip, Edge, Face"),
                        (
                            "Vertex, Edge, Face tools",
                            "LMB drag a handle of the selected brushes (a corner, an edge midpoint, a face centre); in the perspective view it moves on the axis plane facing the camera",
                        ),
                        (
                            "Clip tool",
                            "click two points (2D: a line through the view; 3D: on a face) or three, drag a point to move it; Enter applies, Tab front / back / both, Shift+Enter swaps sides, Backspace removes a point, Esc clears",
                        ),
                        (", / .", "rotate the selection 90° in the active view"),
                        ("Alt+X / Y / Z", "flip the selection"),
                        ("[ ]", "grid size"),
                        ("\\", "snap on/off"),
                        ("F", "frame the selection or all"),
                        ("Space", "maximize the pane under the cursor"),
                        ("Del", "delete selection"),
                        ("Ctrl+Z / Ctrl+Shift+Z", "undo / redo"),
                        ("Esc", "cancel a drag, or close the open group"),
                    ] {
                        if v.is_empty() {
                            ui.strong(k);
                        } else {
                            ui.monospace(k);
                        }
                        ui.label(v);
                        ui.end_row();
                    }
                });
            });
        self.show_help = open;
    }
}

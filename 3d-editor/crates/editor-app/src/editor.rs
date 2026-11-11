use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use egui::{Context, Key, Ui};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};
use glam::DVec3;
use level_cmd::args::{
    Axis, AxisArg, CreateBox, Delete, Factors, Flip, NoArgs, Pivot, Rename, Rotate, Save, Scale,
    SelectIds, SelectKind, SetClassname, SetLayer, SetMaterial, SetProperties, SetTransform,
    Translate,
};
use level_cmd::results::{
    Created, Deleted, FacesChanged, IdCount, IdOnly, Moved, Saved, Stepped, Transformed, Updated,
};
use level_cmd::{Request, Success};
use level_core::{ElementRef, ObjectId, ObjectKind, Part, Properties};
use level_geom::Aabb;
use level_geom::camera::{frame_camera, ortho_axes};
use level_proto::WakeFn;
use level_render::{PreparedScene, Renderer, ViewportOptions};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::drag::DragOp;
use crate::drill;
use crate::error::AppError;
use crate::gfx::Gfx;
use crate::nav;
use crate::pane::{Action, Pane, PaneEnv, PaneId, Tool};
use crate::panels::{Event, Panels};
use crate::remote::Remote;
use crate::theme::{self, Theme};

const GRIDS: [f64; 12] = [
    0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0,
];
const DEFAULT_GRID: usize = 7;

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
}

impl Keys {
    fn read(i: &egui::InputState) -> Keys {
        let cmd = i.modifiers.command;
        let shift = i.modifiers.shift;
        let plain = |key| !cmd && i.key_pressed(key);
        Keys {
            undo: cmd && !shift && i.key_pressed(Key::Z),
            redo: (cmd && shift && i.key_pressed(Key::Z)) || (cmd && i.key_pressed(Key::Y)),
            delete: i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
            space: i.key_pressed(Key::Space),
            frame: plain(Key::F),
            snap: plain(Key::Backslash),
            select_all: cmd && !shift && i.key_pressed(Key::A),
            select_invert: cmd && !shift && i.key_pressed(Key::I),
            select_none: cmd && shift && i.key_pressed(Key::A),
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
            ]
            .into_iter()
            .find_map(|(key, tool)| plain(key).then_some(tool)),
        }
    }
}

pub struct Editor {
    remote: Remote,
    dock: DockState<PaneId>,
    saved_dock: Option<DockState<PaneId>>,
    panes: HashMap<PaneId, Pane>,
    tool: Tool,
    grid: f64,
    snap: bool,
    pub theme: Theme,
    show_grid: bool,
    show_axes: bool,
    show_help: bool,
    hover: Option<ObjectId>,
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

type PrepKey = (u64, Vec<ObjectId>, Option<ObjectId>);

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
            grid,
            snap: true,
            theme: Theme::Dark,
            show_grid: true,
            show_axes: true,
            show_help: false,
            hover: None,
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
                self.status.clear();
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
            .filter_map(|&id| level_cmd::extent(doc, id))
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
        };
        self.call::<_, Transformed>("transform.flip", &args, None);
    }

    fn select(&mut self, op: &str, ids: Vec<ObjectId>) {
        self.call::<_, IdCount>(op, &SelectIds { ids }, None);
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
            DragOp::Translate(delta) => self
                .call::<_, Transformed>("transform.translate", &Translate { ids, delta }, key)
                .is_some(),
            DragOp::Rotate { axis, degrees } => self
                .call::<_, Transformed>(
                    "transform.rotate",
                    &Rotate {
                        ids,
                        axis: AxisArg::Vector(axis),
                        degrees,
                        pivot,
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
                    },
                    key,
                )
                .is_some(),
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Select { id, additive } => {
                let selected = id.is_some_and(|id| self.remote.selection.contains(&id));
                match (id, additive, selected) {
                    (Some(id), false, _) => self.select("select.set", vec![id]),
                    (Some(id), true, true) => self.select("select.remove", vec![id]),
                    (Some(id), true, false) => self.select("select.add", vec![id]),
                    (None, false, _) => {
                        self.call::<_, IdCount>("select.clear", &NoArgs::default(), None);
                    }
                    (None, true, _) => {}
                }
            }
            Action::Paint(id) => self.select("select.add", vec![id]),
            Action::Drill {
                origin,
                dir,
                additive,
            } => {
                let kinds =
                    BTreeSet::from([ObjectKind::Brush, ObjectKind::Mesh, ObjectKind::Entity]);
                let hits: Vec<ObjectId> = level_cmd::ray_hits(
                    &self.remote.doc,
                    origin,
                    dir,
                    &kinds,
                    &BTreeSet::new(),
                    None,
                )
                .into_iter()
                .map(|h| h.id)
                .collect();
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
            .add_filter("level", &["toml"])
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
        )
    }

    fn refresh_prepared(&mut self) {
        let key = self.prep_key();
        if self.prepared.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let scene = PreparedScene::new(
            &self.remote.doc,
            &self.remote.selection,
            self.hover,
            &HashSet::new(),
        );
        self.prepared = Some((key, scene));
        self.repaint = true;
    }

    fn keys(&mut self, ctx: &Context) {
        if ctx.text_edit_focused() {
            return;
        }
        let keys = ctx.input(Keys::read);
        if keys.undo {
            self.undo();
        }
        if keys.redo {
            self.redo();
        }
        if keys.delete {
            self.delete_selection();
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

    pub fn ui(&mut self, ui: &mut Ui, level: &Renderer) {
        let ctx = ui.ctx().clone();
        for pane in self.panes.values_mut() {
            pane.visible = false;
        }
        match self.remote.pump() {
            Ok(true) => self.repaint = true,
            Ok(false) => {}
            Err(e) => self.status = e.to_string(),
        }
        if self.remote.closed {
            self.status = "disconnected from level-server".into();
        }
        self.refresh_prepared();
        self.keys(&ctx);
        self.menu(ui);
        self.toolbar(ui);
        self.status_bar(ui);
        self.outliner(ui);
        self.inspector(ui);
        self.help(&ctx);

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
                renderer: level,
                revision,
                tool: self.tool,
                grid: if self.snap { self.grid } else { 0.0 },
                selection: &selection,
                sel_bounds,
                doc: &self.remote.doc,
                depth_hint: sel_bounds.or(self.last_box),
                pending: self.pending_box,
                pending_next: &mut pending_next,
                actions: &mut actions,
                hover_next: &mut hover_next,
                active: &mut active,
                pivot,
                pick_ms: &mut pick_ms,
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
        self.refresh_prepared();
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
            gfx.level
                .draw_viewport(prepared, &view, options, want, &target.view);
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
                    ui.separator();
                    ui.menu_button("Theme", |ui| {
                        for t in Theme::ALL {
                            if ui.radio_value(&mut self.theme, t, t.name()).clicked() {
                                theme::apply(ui.ctx(), t);
                            }
                        }
                    });
                });
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
                ] {
                    ui.selectable_value(&mut self.tool, tool, format!("{}  {key}", tool.name()));
                }
                ui.separator();
                for stub in ["Clip", "Vertex", "Extrude"] {
                    ui.add_enabled(false, egui::Button::new(stub))
                        .on_disabled_hover_text("Not in the spike");
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
                ui.heading("Outliner");
                ui.separator();
                events = self.panels.outliner(
                    ui,
                    &self.remote.doc,
                    self.remote.revision,
                    &self.remote.selection,
                );
            });
        for event in events {
            self.panel_event(event);
        }
    }

    fn inspector(&mut self, ui: &mut Ui) {
        let mut events = Vec::new();
        egui::Panel::right("inspector")
            .default_size(250.0)
            .size_range(200.0..=420.0)
            .show(ui, |ui| {
                ui.heading("Inspector");
                ui.separator();
                events = self.panels.inspector(
                    ui,
                    &self.remote.doc,
                    self.remote.revision,
                    &self.remote.selection,
                );
            });
        for event in events {
            self.panel_event(event);
        }
    }

    fn panel_event(&mut self, event: Event) {
        match event {
            Event::Select { id, additive } => self.apply(Action::Select {
                id: Some(id),
                additive,
            }),
            Event::Frame(id) => {
                if let Some(bounds) = level_cmd::extent(&self.remote.doc, id) {
                    self.frame_bounds(bounds);
                }
            }
            Event::Rename { id, name } => {
                self.call::<_, IdOnly>("object.rename", &Rename { id, name }, None);
            }
            Event::SetLayer { ids, layer } => {
                self.call::<_, Moved>("object.set_layer", &SetLayer { ids, layer }, None);
            }
            Event::SetClassname { id, classname } => {
                let args = SetClassname {
                    ids: vec![id],
                    classname,
                };
                self.call::<_, Updated>("entity.set_classname", &args, None);
            }
            Event::SetTransform {
                id,
                origin,
                angles,
                merge,
            } => {
                let args = SetTransform { id, origin, angles };
                self.call::<_, IdOnly>("entity.set_transform", &args, merge.as_deref());
            }
            Event::SetProperty {
                id,
                key,
                value,
                merge,
            } => {
                let args = SetProperties {
                    ids: vec![id],
                    set: [(key, value)].into(),
                    remove: Vec::new(),
                };
                self.call::<_, Updated>("entity.set_properties", &args, merge.as_deref());
            }
            Event::RemoveProperty { id, key } => {
                let args = SetProperties {
                    ids: vec![id],
                    set: Properties::new(),
                    remove: vec![key],
                };
                self.call::<_, Updated>("entity.set_properties", &args, None);
            }
            Event::SetMaterial { ids, material } => {
                let faces = ids
                    .into_iter()
                    .map(|object| ElementRef {
                        object,
                        part: Part::All,
                    })
                    .collect();
                let args = SetMaterial { faces, material };
                self.call::<_, FacesChanged>("face.set_material", &args, None);
            }
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
                        ("Ctrl+A / Ctrl+I", "select all / invert"),
                        ("1 2 3 4 5", "Select, Move, Box, Rotate, Scale"),
                        (", / .", "rotate the selection 90° in the active view"),
                        ("Alt+X / Y / Z", "flip the selection"),
                        ("[ ]", "grid size"),
                        ("\\", "snap on/off"),
                        ("F", "frame the selection or all"),
                        ("Space", "maximize the pane under the cursor"),
                        ("Del", "delete selection"),
                        ("Ctrl+Z / Ctrl+Shift+Z", "undo / redo"),
                        ("Esc", "cancel a drag"),
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

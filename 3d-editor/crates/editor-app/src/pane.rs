use std::collections::BTreeSet;

use editor_cmd::args::{BrushVertexAt, Keep};
use editor_core::{Camera, Document, FaceId, GroupId, ObjectId, ObjectKind, OrthoView, Projection};
use editor_geom::Aabb;
use editor_geom::camera::{View, viewport_camera};
use editor_render::{PickMap, PreparedScene, Renderer};
use egui::{
    Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use glam::{DVec2, DVec3};

use crate::clip::{ClipState, Preview};
use crate::cuboid::{self, BoxPlane};
use crate::drag::{DragOp, wrap_degrees};
use crate::elements::{self, ElementHandle, Kind};
use crate::gfx::Target;
use crate::gizmo::{Gizmo, GizmoDrag, Handle, Mode};
use crate::nav;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneId {
    Persp,
    Top,
    Front,
    Side,
}

impl PaneId {
    pub const ALL: [PaneId; 4] = [PaneId::Persp, PaneId::Top, PaneId::Front, PaneId::Side];

    pub fn title(self) -> &'static str {
        match self {
            PaneId::Persp => "Perspective",
            PaneId::Top => "Top (XY)",
            PaneId::Front => "Front (XZ)",
            PaneId::Side => "Side (YZ)",
        }
    }

    pub fn camera_name(self) -> &'static str {
        match self {
            PaneId::Persp => "viewport.persp",
            PaneId::Top => "viewport.top",
            PaneId::Front => "viewport.front",
            PaneId::Side => "viewport.side",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Move,
    Rotate,
    Scale,
    Box,
    Vertex,
    Edge,
    Face,
    Clip,
    Surface,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Move => "Move",
            Tool::Rotate => "Rotate",
            Tool::Scale => "Scale",
            Tool::Box => "Box",
            Tool::Vertex => "Vertex",
            Tool::Edge => "Edge",
            Tool::Face => "Face",
            Tool::Clip => "Clip",
            Tool::Surface => "Surface",
        }
    }

    pub fn element_kind(self) -> Option<Kind> {
        match self {
            Tool::Vertex => Some(Kind::Vertex),
            Tool::Edge => Some(Kind::Edge),
            Tool::Face => Some(Kind::Face),
            _ => None,
        }
    }

    fn gizmo_mode(self) -> Option<Mode> {
        match self {
            Tool::Move => Some(Mode::Move),
            Tool::Rotate => Some(Mode::Rotate),
            Tool::Scale => Some(Mode::Scale),
            Tool::Select
            | Tool::Box
            | Tool::Vertex
            | Tool::Edge
            | Tool::Face
            | Tool::Clip
            | Tool::Surface => None,
        }
    }
}

/// What a click with the Surface tool does to the face under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceClick {
    Select,
    Apply,
    Wrap,
}

#[derive(Debug, Clone)]
pub enum Action {
    SurfaceClick {
        face: Option<(ObjectId, FaceId)>,
        additive: bool,
        mode: SurfaceClick,
    },
    Select {
        id: Option<ObjectId>,
        additive: bool,
    },
    Paint(ObjectId),
    OpenGroupAt(ObjectId),
    VertexBegin(Vec<BrushVertexAt>),
    Drill {
        origin: DVec3,
        dir: DVec3,
        additive: bool,
    },
    DragBegin {
        pivot: Option<DVec3>,
    },
    DragTo(DragOp),
    DragEnd {
        commit: bool,
    },
    CreateBox {
        min: DVec3,
        max: DVec3,
    },
    ClipAdd {
        point: DVec3,
        dir: DVec3,
    },
    ClipSet {
        index: usize,
        point: DVec3,
    },
}

pub struct PaneEnv<'a> {
    pub prepared: &'a PreparedScene,
    pub renderer: &'a Renderer,
    pub revision: u64,
    pub tool: Tool,
    pub grid: f64,
    pub selection: &'a BTreeSet<ObjectId>,
    pub sel_bounds: Option<Aabb>,
    pub doc: &'a Document,
    pub open_group: Option<GroupId>,
    pub depth_hint: Option<Aabb>,
    pub picks: &'a [BrushVertexAt],
    pub clip: &'a ClipState,
    pub clip_preview: &'a Preview,
    pub pending: Option<Aabb>,
    pub pending_next: &'a mut Option<Aabb>,
    pub actions: &'a mut Vec<Action>,
    pub hover_next: &'a mut Option<ObjectId>,
    pub active: &'a mut Option<PaneId>,
    pub pivot: DVec3,
    pub pick_ms: &'a mut f32,
}

#[derive(Default)]
enum Interaction {
    #[default]
    Idle,
    Box {
        start: DVec2,
        now: DVec2,
    },
    Move {
        accumulated: DVec2,
        total: DVec3,
    },
    Rotate {
        pivot: DVec2,
        last: f64,
        turned: f64,
        degrees: f64,
    },
    Scale {
        pivot: DVec2,
        handle: DVec2,
        size: DVec2,
        factors: DVec2,
    },
    Gizmo {
        gizmo: Box<Gizmo>,
        drag: GizmoDrag,
        last: Option<DragOp>,
    },
    BoxPlane {
        plane: BoxPlane,
        start: DVec3,
        now: DVec3,
    },
    Paint(BTreeSet<ObjectId>),
    ClipDrag(usize),
    Element {
        plane: BoxPlane,
        start: DVec3,
        total: DVec3,
    },
    Cancelled,
}

fn angle_of(v: DVec2) -> f64 {
    v.y.atan2(v.x).to_degrees()
}

type PickKey = (u64, [u64; 6], (u32, u32));

pub struct Pane {
    pub id: PaneId,
    pub camera: Option<Camera>,
    pub target: Option<Target>,
    pub want_px: (u32, u32),
    pub visible: bool,
    pick: Option<(PickKey, PickMap)>,
    interaction: Interaction,
    gizmo_hover: Option<Handle>,
    pub cursor: Option<DVec2>,
}

const HANDLE_PICK_PX: f64 = 9.0;

pub fn snap(v: f64, grid: f64) -> f64 {
    if grid > 0.0 {
        (v / grid).round() * grid
    } else {
        v
    }
}

/// Snaps the two in-face axes of a point on a face to the grid and keeps it on the face's plane.
fn snap_to_face(point: DVec3, normal: DVec3, grid: f64) -> DVec3 {
    let abs = normal.abs();
    let axis = if abs.x >= abs.y && abs.x >= abs.z {
        0
    } else if abs.y >= abs.z {
        1
    } else {
        2
    };
    if abs[axis] < 1e-6 {
        return point;
    }
    let mut p = point;
    let mut off = 0.0;
    for i in (0..3).filter(|&i| i != axis) {
        p[i] = snap(point[i], grid);
        off += normal[i] * (p[i] - point[i]);
    }
    p[axis] = point[axis] - off / normal[axis];
    p
}

fn camera_bits(c: &Camera) -> [u64; 6] {
    let extra = match c.projection {
        Projection::Perspective { fov } => fov,
        Projection::Orthographic { zoom, .. } => zoom,
    };
    [
        c.position.x.to_bits(),
        c.position.y.to_bits(),
        c.position.z.to_bits(),
        c.yaw.to_bits(),
        c.pitch.to_bits(),
        extra.to_bits(),
    ]
}

impl Pane {
    pub fn new(id: PaneId) -> Self {
        Self {
            id,
            camera: None,
            target: None,
            want_px: (1, 1),
            visible: false,
            pick: None,
            interaction: Interaction::Idle,
            gizmo_hover: None,
            cursor: None,
        }
    }

    pub fn reset_camera(&mut self) {
        self.camera = None;
    }

    fn ensure_pick(&mut self, env: &mut PaneEnv, camera: &Camera, view: &View, px: (u32, u32)) {
        let key = (env.revision, camera_bits(camera), px);
        if self.pick.as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let started = std::time::Instant::now();
        if let Ok(map) = env.renderer.pick_map(env.prepared, view, px) {
            self.pick = Some((key, map));
        }
        *env.pick_ms += started.elapsed().as_secs_f32() * 1000.0;
    }

    fn hit(&self, env: &PaneEnv, local_px: Pos2) -> Option<ObjectId> {
        let (_, map) = self.pick.as_ref()?;
        if local_px.x < 0.0 || local_px.y < 0.0 {
            return None;
        }
        map.at(local_px.x as u32, local_px.y as u32)
            .filter(|&id| env.doc.pickable(id, env.open_group))
    }

    fn ortho_world(view: &View, px: Pos2) -> Option<DVec2> {
        let (lo, hi) = view.ortho_extent()?;
        Some(DVec2::new(
            lo.x + f64::from(px.x) / view.width * (hi.x - lo.x),
            hi.y - f64::from(px.y) / view.height * (hi.y - lo.y),
        ))
    }

    fn ortho_to_screen(view: &View, uv: DVec2, rect: Rect, ppp: f32) -> Pos2 {
        let p = view.right * uv.x + view.up * uv.y;
        let projected = view.project(p).map_or(DVec2::ZERO, |p| p.px);
        rect.min + vec2(projected.x as f32, projected.y as f32) / ppp
    }

    pub fn show(&mut self, ui: &mut Ui, env: &mut PaneEnv) {
        let avail = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let px = (
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        );
        self.want_px = px;
        self.visible = true;

        if self.camera.is_none() {
            self.camera = viewport_camera(self.id.camera_name(), env.prepared.bounds(), px.0, px.1);
        }
        let Some(mut camera) = self.camera.clone() else {
            return;
        };

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(0x29, 0x2b, 0x30));
        if let Some(t) = &self.target {
            painter.image(
                t.id,
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        if resp.clicked() || resp.drag_started() {
            resp.request_focus();
        }
        if resp.hovered() {
            *env.active = Some(self.id);
        }

        let mods = ui.input(|i| i.modifiers);
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let delta = resp.drag_delta();
        let secondary = resp.dragged_by(PointerButton::Secondary);
        let middle = resp.dragged_by(PointerButton::Middle);
        let primary = resp.dragged_by(PointerButton::Primary);
        let is_ortho = nav::ortho_view(&camera).is_some();
        let view = env.prepared.view(&camera, px.0, px.1);
        let local_px = |p: Pos2| -> Pos2 {
            let l = (p - rect.min) * ppp;
            pos2(l.x, l.y)
        };
        let pointer = resp.hover_pos().or_else(|| resp.interact_pointer_pos());

        let navigating = secondary || middle || (primary && mods.alt);
        if resp.hovered()
            && !navigating
            && !primary
            && let Some(p) = pointer
        {
            let l = local_px(p);
            self.ensure_pick(env, &camera, &view, px);
            *env.hover_next = self.hit(env, l);
        }
        self.cursor = pointer
            .filter(|_| is_ortho)
            .and_then(|p| Self::ortho_world(&view, local_px(p)));

        if is_ortho {
            if secondary || middle {
                nav::pan_ortho(
                    &mut camera,
                    f64::from(delta.x * ppp),
                    f64::from(delta.y * ppp),
                );
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if resp.hovered()
                && scroll != 0.0
                && let Some(p) = pointer
            {
                let l = local_px(p);
                let (cx, cy) = (
                    f64::from(l.x) - view.width / 2.0,
                    f64::from(l.y) - view.height / 2.0,
                );
                nav::zoom_ortho_at(&mut camera, cx, cy, (f64::from(scroll) * 0.004).exp());
            }
        } else {
            let center = env.pivot;
            if secondary {
                nav::look(&mut camera, f64::from(delta.x), f64::from(delta.y));
                let dt = ui.input(|i| f64::from(i.stable_dt));
                let mut local = DVec3::ZERO;
                ui.input(|i| {
                    use egui::Key::*;
                    for (key, axis) in [
                        (W, DVec3::X),
                        (S, DVec3::NEG_X),
                        (D, DVec3::Y),
                        (A, DVec3::NEG_Y),
                        (E, DVec3::Z),
                        (Q, DVec3::NEG_Z),
                    ] {
                        if i.key_down(key) {
                            local += axis;
                        }
                    }
                });
                if local != DVec3::ZERO {
                    let speed = if mods.shift { 1200.0 } else { 400.0 };
                    nav::fly(&mut camera, local.normalize(), speed * dt);
                    ui.ctx().request_repaint();
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            }
            if middle {
                let distance = (camera.position - center).length().max(32.0);
                let fov = match camera.projection {
                    Projection::Perspective { fov } => fov,
                    Projection::Orthographic { .. } => 60.0,
                };
                let per_point =
                    distance * 2.0 * (fov.to_radians() / 2.0).tan() / f64::from(rect.height());
                nav::pan_persp(
                    &mut camera,
                    f64::from(delta.x),
                    f64::from(delta.y),
                    per_point,
                );
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if primary && mods.alt {
                nav::orbit(&mut camera, center, f64::from(delta.x), f64::from(delta.y));
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if resp.hovered() && scroll != 0.0 {
                nav::dolly(&mut camera, center, f64::from(scroll));
            }
        }

        let gizmo = (!is_ortho)
            .then(|| env.tool.gizmo_mode())
            .flatten()
            .zip(env.sel_bounds)
            .map(|(mode, b)| Gizmo::new(b.center(), mode, &view, b.max - b.min));
        self.drive_gizmo(
            env,
            &resp,
            &view,
            gizmo.as_ref(),
            (navigating, escape, mods.shift),
            pointer.map(&local_px),
        );

        if resp.double_clicked_by(PointerButton::Primary)
            && env.tool != Tool::Clip
            && !mods.alt
            && !mods.command
            && self.gizmo_hover.is_none()
            && let Some(p) = resp.interact_pointer_pos()
        {
            self.ensure_pick(env, &camera, &view, px);
            if let Some(id) = self.hit(env, local_px(p)) {
                env.actions.push(Action::OpenGroupAt(id));
            }
        }

        if env.tool == Tool::Surface
            && resp.clicked_by(PointerButton::Primary)
            && !mods.alt
            && let Some(p) = resp.interact_pointer_pos()
        {
            let l = local_px(p);
            let ray = view.ray(DVec2::new(f64::from(l.x), f64::from(l.y)));
            let mode = match (mods.command, mods.shift) {
                (true, true) => SurfaceClick::Wrap,
                (true, false) => SurfaceClick::Apply,
                _ => SurfaceClick::Select,
            };
            env.actions.push(Action::SurfaceClick {
                face: Self::surface_face(env, ray),
                additive: mods.shift && !mods.command,
                mode,
            });
        }

        if resp.clicked_by(PointerButton::Primary)
            && env.tool != Tool::Clip
            && env.tool != Tool::Surface
            && !mods.alt
            && self.gizmo_hover.is_none()
        {
            if mods.command {
                if let Some(p) = resp.interact_pointer_pos() {
                    let l = local_px(p);
                    let (origin, dir) = view.ray(DVec2::new(f64::from(l.x), f64::from(l.y)));
                    env.actions.push(Action::Drill {
                        origin,
                        dir,
                        additive: mods.shift,
                    });
                }
            } else {
                let hit = resp.interact_pointer_pos().and_then(|p| {
                    self.ensure_pick(env, &camera, &view, px);
                    self.hit(env, local_px(p))
                });
                env.actions.push(Action::Select {
                    id: hit,
                    additive: mods.shift,
                });
            }
        }

        if env.tool == Tool::Select && mods.shift && !mods.alt {
            self.paint_select(env, &resp, &camera, &view, px, pointer.map(&local_px));
        }

        if !is_ortho && env.tool == Tool::Box && !mods.alt {
            self.box_persp(env, &resp, &view, escape, pointer.map(&local_px));
        }

        if !is_ortho && env.tool.element_kind().is_some() && !mods.alt {
            self.element_persp(env, &resp, &view, escape, pointer.map(&local_px));
        }

        if is_ortho && !mods.alt {
            self.tools(
                env,
                &resp,
                &view,
                &camera,
                delta,
                escape,
                mods.shift,
                pointer.map(&local_px),
            );
        }

        if env.tool == Tool::Clip && !mods.alt && !navigating {
            self.clip_tool(env, &resp, &view, is_ortho, pointer.map(&local_px));
        }

        self.camera = Some(camera.clone());
        let view = env.prepared.view(&camera, px.0, px.1);
        self.overlays(
            ui,
            &painter,
            rect,
            &resp,
            &view,
            &camera,
            env,
            ppp,
            gizmo.as_ref(),
        );
    }

    fn box_persp(
        &mut self,
        env: &mut PaneEnv,
        resp: &egui::Response,
        view: &View,
        escape: bool,
        local: Option<Pos2>,
    ) {
        let grid = env.grid;
        let ray_of = |p: Pos2| view.ray(DVec2::new(f64::from(p.x), f64::from(p.y)));
        let snapped = |mut p: DVec3, plane: &BoxPlane| {
            for i in (0..3).filter(|&i| i != plane.axis) {
                p[i] = snap(p[i], grid);
            }
            p
        };
        if resp.drag_started_by(PointerButton::Primary)
            && let Some(origin) = resp.ctx.input(|i| i.pointer.press_origin())
        {
            let ppp = resp.ctx.pixels_per_point();
            let l = (origin - resp.rect.min) * ppp;
            let ray = ray_of(pos2(l.x, l.y));
            let kinds = BTreeSet::from([ObjectKind::Brush, ObjectKind::Mesh, ObjectKind::Entity]);
            let hidden = env.doc.hidden_ids();
            let plane = editor_cmd::ray_hits(env.doc, ray.0, ray.1, &kinds, &hidden, None)
                .first()
                .map_or(BoxPlane::ground(), |h| {
                    BoxPlane::from_hit(h.point, h.normal)
                });
            if let Some(p) = plane.point_at(ray) {
                let p = snapped(p, &plane);
                self.interaction = Interaction::BoxPlane {
                    plane,
                    start: p,
                    now: p,
                };
            }
        }
        if let Interaction::BoxPlane { plane, start, now } = &mut self.interaction {
            if let Some(p) = local.and_then(|l| plane.point_at(ray_of(l))) {
                *now = snapped(p, plane);
            }
            let (plane, a, b) = (*plane, *start, *now);
            let depth = cuboid::depth_along(plane.axis, env.depth_hint);
            let cuboid = plane.cuboid(a, b, depth);
            *env.pending_next = Some(cuboid);
            if escape {
                self.interaction = Interaction::Cancelled;
            } else if resp.drag_stopped_by(PointerButton::Primary) {
                self.interaction = Interaction::Idle;
                if plane.has_area(a, b) {
                    env.actions.push(Action::CreateBox {
                        min: cuboid.min,
                        max: cuboid.max,
                    });
                }
            }
        }
    }

    fn drive_gizmo(
        &mut self,
        env: &mut PaneEnv,
        resp: &egui::Response,
        view: &View,
        gizmo: Option<&Gizmo>,
        (navigating, escape, free): (bool, bool, bool),
        pointer: Option<Pos2>,
    ) {
        let ray_at = |p: Pos2| view.ray(DVec2::new(f64::from(p.x), f64::from(p.y)));
        if let Interaction::Gizmo {
            gizmo: frozen,
            drag,
            last,
        } = &mut self.interaction
        {
            self.gizmo_hover = Some(drag.handle);
            if escape {
                env.actions.push(Action::DragEnd { commit: false });
                self.interaction = Interaction::Cancelled;
                return;
            }
            if let Some(p) = pointer
                && let Some(op) = drag.op(frozen, ray_at(p), env.grid, free)
                && *last != Some(op)
            {
                *last = Some(op);
                env.actions.push(Action::DragTo(op));
            }
            if resp.drag_stopped_by(PointerButton::Primary) {
                env.actions.push(Action::DragEnd { commit: true });
                self.interaction = Interaction::Idle;
                self.gizmo_hover = None;
            }
            return;
        }
        self.gizmo_hover = None;
        let Some(gizmo) = gizmo else { return };
        if navigating {
            return;
        }
        let ppp = resp.ctx.pixels_per_point();
        let local = |p: Pos2| {
            let l = (p - resp.rect.min) * ppp;
            DVec2::new(f64::from(l.x), f64::from(l.y))
        };
        if resp.drag_started_by(PointerButton::Primary)
            && let Some(origin) = resp.ctx.input(|i| i.pointer.press_origin())
            && let Some(handle) = gizmo.hit(local(origin))
            && let Some(drag) = gizmo.begin(handle, view.ray(local(origin)))
        {
            env.actions.push(Action::DragBegin {
                pivot: Some(gizmo.pivot()),
            });
            self.gizmo_hover = Some(handle);
            self.interaction = Interaction::Gizmo {
                gizmo: Box::new(Gizmo::new(
                    gizmo.pivot(),
                    gizmo.mode,
                    view,
                    env.sel_bounds.map_or(DVec3::ONE, |b| b.max - b.min),
                )),
                drag,
                last: None,
            };
            return;
        }
        if resp.hovered()
            && let Some(p) = pointer
        {
            self.gizmo_hover = gizmo.hit(local(p));
        }
    }

    /// Where the primary button went down, in the 2D view's world coordinates.
    fn press_world(resp: &egui::Response, view: &View) -> Option<DVec2> {
        let origin = resp.ctx.input(|i| i.pointer.press_origin())?;
        let l = (origin - resp.rect.min) * resp.ctx.pixels_per_point();
        Self::ortho_world(view, pos2(l.x, l.y))
    }

    fn rotate_tool(
        &mut self,
        env: &mut PaneEnv,
        resp: &egui::Response,
        view: &View,
        escape: bool,
        free: bool,
        local: Option<Pos2>,
    ) {
        let cursor = local.and_then(|l| Self::ortho_world(view, l));
        if resp.drag_started_by(PointerButton::Primary)
            && let Some(b) = env.sel_bounds
            && let Some(c) = Self::press_world(resp, view).or(cursor)
        {
            let center = b.center();
            let pivot = DVec2::new(center.dot(view.right), center.dot(view.up));
            env.actions.push(Action::DragBegin {
                pivot: Some(center),
            });
            self.interaction = Interaction::Rotate {
                pivot,
                last: angle_of(c - pivot),
                turned: 0.0,
                degrees: 0.0,
            };
        }
        if let Interaction::Rotate {
            pivot,
            last,
            turned,
            degrees,
        } = &mut self.interaction
        {
            if escape {
                env.actions.push(Action::DragEnd { commit: false });
                self.interaction = Interaction::Cancelled;
                return;
            }
            if let Some(c) = cursor {
                let now = angle_of(c - *pivot);
                *turned += wrap_degrees(now - *last);
                *last = now;
                let next = if free { *turned } else { snap(*turned, 15.0) };
                if next != *degrees {
                    *degrees = next;
                    env.actions.push(Action::DragTo(DragOp::Rotate {
                        axis: view.right.cross(view.up),
                        degrees: next,
                    }));
                }
            }
            if resp.drag_stopped_by(PointerButton::Primary) {
                env.actions.push(Action::DragEnd { commit: true });
                self.interaction = Interaction::Idle;
            }
        }
    }

    fn scale_tool(
        &mut self,
        env: &mut PaneEnv,
        resp: &egui::Response,
        view: &View,
        escape: bool,
        local: Option<Pos2>,
    ) {
        let cursor = local.and_then(|l| Self::ortho_world(view, l));
        if resp.drag_started_by(PointerButton::Primary)
            && let Some(b) = env.sel_bounds
            && let Some(c) = Self::press_world(resp, view).or(cursor)
        {
            let lo = DVec2::new(b.min.dot(view.right), b.min.dot(view.up));
            let hi = DVec2::new(b.max.dot(view.right), b.max.dot(view.up));
            let mid = (lo + hi) / 2.0;
            let handle = DVec2::new(
                if c.x > mid.x { hi.x } else { lo.x },
                if c.y > mid.y { hi.y } else { lo.y },
            );
            let pivot = lo + hi - handle;
            let center = b.center();
            env.actions.push(Action::DragBegin {
                pivot: Some(
                    view.right * pivot.x
                        + view.up * pivot.y
                        + view.forward * center.dot(view.forward),
                ),
            });
            self.interaction = Interaction::Scale {
                pivot,
                handle,
                size: hi - lo,
                factors: DVec2::ONE,
            };
        }
        if let Interaction::Scale {
            pivot,
            handle,
            size,
            factors,
        } = &mut self.interaction
        {
            if escape {
                env.actions.push(Action::DragEnd { commit: false });
                self.interaction = Interaction::Cancelled;
                return;
            }
            if let Some(c) = cursor {
                let step = if env.grid > 0.0 { env.grid } else { 1.0 };
                let axis = |c: f64, pivot: f64, handle: f64, size: f64| {
                    let span = handle - pivot;
                    if size < 1e-9 || span.abs() < 1e-9 {
                        return 1.0;
                    }
                    let ratio = ((c - pivot) / span).max(0.0);
                    snap(size * ratio, env.grid).max(step) / size
                };
                let next = DVec2::new(
                    axis(c.x, pivot.x, handle.x, size.x),
                    axis(c.y, pivot.y, handle.y, size.y),
                );
                if next != *factors {
                    *factors = next;
                    let world = DVec3::ONE
                        + view.right.abs() * (next.x - 1.0)
                        + view.up.abs() * (next.y - 1.0);
                    env.actions.push(Action::DragTo(DragOp::Scale(world)));
                }
            }
            if resp.drag_stopped_by(PointerButton::Primary) {
                env.actions.push(Action::DragEnd { commit: true });
                self.interaction = Interaction::Idle;
            }
        }
    }

    fn paint_select(
        &mut self,
        env: &mut PaneEnv,
        resp: &egui::Response,
        camera: &Camera,
        view: &View,
        px: (u32, u32),
        local: Option<Pos2>,
    ) {
        if resp.drag_started_by(PointerButton::Primary) {
            self.interaction = Interaction::Paint(BTreeSet::new());
        }
        if matches!(self.interaction, Interaction::Paint(_)) {
            let hit = local.and_then(|l| {
                self.ensure_pick(env, camera, view, px);
                self.hit(env, l)
            });
            let Interaction::Paint(painted) = &mut self.interaction else {
                return;
            };
            if let Some(id) = hit
                && painted.insert(id)
            {
                env.actions.push(Action::Paint(id));
            }
            if resp.drag_stopped_by(PointerButton::Primary) {
                self.interaction = Interaction::Idle;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tools(
        &mut self,
        env: &mut PaneEnv,
        resp: &egui::Response,
        view: &View,
        camera: &Camera,
        delta: egui::Vec2,
        escape: bool,
        shift: bool,
        local: Option<Pos2>,
    ) {
        let zoom = nav::zoom_of(camera);
        let ppp = resp.ctx.pixels_per_point();
        let world = |p: Pos2| {
            Self::ortho_world(view, p).map(|w| DVec2::new(snap(w.x, env.grid), snap(w.y, env.grid)))
        };
        match env.tool {
            Tool::Select | Tool::Clip | Tool::Surface => {}
            Tool::Rotate => self.rotate_tool(env, resp, view, escape, shift, local),
            Tool::Scale => self.scale_tool(env, resp, view, escape, local),
            Tool::Box => {
                if resp.drag_started_by(PointerButton::Primary)
                    && let Some(w) = local.and_then(world)
                {
                    self.interaction = Interaction::Box { start: w, now: w };
                }
                if let Interaction::Box { start, now } = &mut self.interaction {
                    if let Some(w) = local.and_then(world) {
                        *now = w;
                    }
                    let (a, b) = (*start, *now);
                    if let Some((min, max)) = self.box_corners(camera, a, b, env.depth_hint) {
                        *env.pending_next = Some(Aabb { min, max });
                    }
                    if escape {
                        self.interaction = Interaction::Cancelled;
                    } else if resp.drag_stopped_by(PointerButton::Primary) {
                        self.interaction = Interaction::Idle;
                        if (a.x - b.x).abs() > 0.0
                            && (a.y - b.y).abs() > 0.0
                            && let Some((min, max)) = self.box_corners(camera, a, b, env.depth_hint)
                        {
                            env.actions.push(Action::CreateBox { min, max });
                        }
                    }
                }
            }
            Tool::Vertex | Tool::Edge | Tool::Face => {
                if resp.drag_started_by(PointerButton::Primary)
                    && let Some(origin) = resp.ctx.input(|i| i.pointer.press_origin())
                {
                    let l = (origin - resp.rect.min) * ppp;
                    let at = DVec2::new(f64::from(l.x), f64::from(l.y));
                    let picks = elements::merged_corners(&Self::handles_near(env, view, at));
                    if !picks.is_empty() {
                        env.actions.push(Action::VertexBegin(picks));
                        env.actions.push(Action::DragBegin { pivot: None });
                        self.interaction = Interaction::Move {
                            accumulated: DVec2::ZERO,
                            total: DVec3::ZERO,
                        };
                    }
                }
                self.drive_move(env, resp, view, zoom, delta, escape);
            }
            Tool::Move => {
                if resp.drag_started_by(PointerButton::Primary) {
                    let hit = local.and_then(|l| self.hit(env, l));
                    if let Some(id) = hit {
                        if !env.selection.contains(&id) {
                            env.actions.push(Action::Select {
                                id: Some(id),
                                additive: false,
                            });
                        }
                        env.actions.push(Action::DragBegin { pivot: None });
                        self.interaction = Interaction::Move {
                            accumulated: DVec2::ZERO,
                            total: DVec3::ZERO,
                        };
                    }
                }
                self.drive_move(env, resp, view, zoom, delta, escape);
            }
        }
        if matches!(self.interaction, Interaction::Cancelled) && !resp.dragged() {
            self.interaction = Interaction::Idle;
        }
    }

    fn clip_tool(
        &mut self,
        env: &mut PaneEnv,
        resp: &egui::Response,
        view: &View,
        is_ortho: bool,
        local: Option<Pos2>,
    ) {
        let ppp = resp.ctx.pixels_per_point();
        let to_px = |p: Pos2| {
            let l = (p - resp.rect.min) * ppp;
            DVec2::new(f64::from(l.x), f64::from(l.y))
        };
        if resp.drag_started_by(PointerButton::Primary)
            && let Some(origin) = resp.ctx.input(|i| i.pointer.press_origin())
        {
            let at = to_px(origin);
            let grabbed = env.clip.points.iter().position(|p| {
                view.project(*p)
                    .is_some_and(|q| (q.px - at).length() <= HANDLE_PICK_PX)
            });
            if let Some(index) = grabbed {
                self.interaction = Interaction::ClipDrag(index);
            }
        }
        if let Interaction::ClipDrag(index) = self.interaction {
            if let Some(l) = local
                && let Some(&current) = env.clip.points.get(index)
                && let Some((point, _)) =
                    Self::clip_point(env, view, is_ortho, to_px(l), Some(current))
                && point != current
            {
                env.actions.push(Action::ClipSet { index, point });
            }
            if resp.drag_stopped_by(PointerButton::Primary) {
                self.interaction = Interaction::Idle;
            }
        } else if resp.clicked_by(PointerButton::Primary)
            && let Some(p) = resp.interact_pointer_pos()
            && let Some((point, dir)) = Self::clip_point(env, view, is_ortho, to_px(p), None)
        {
            env.actions.push(Action::ClipAdd { point, dir });
        }
    }

    /// The grid-snapped point under `px` and the direction the clip plane should contain when this is its second point.
    /// In 2D that is the view direction; in 3D the plane runs through the face hit, so it is that face's normal.
    /// `existing` is a point being dragged: it stays at its depth instead of picking a new face.
    fn clip_point(
        env: &PaneEnv,
        view: &View,
        is_ortho: bool,
        px: DVec2,
        existing: Option<DVec3>,
    ) -> Option<(DVec3, DVec3)> {
        let grid = env.grid;
        if is_ortho {
            let w = Self::ortho_world(view, pos2(px.x as f32, px.y as f32))?;
            let depth = existing
                .or_else(|| env.sel_bounds.map(|b| b.center()))
                .map_or(0.0, |p| p.dot(view.forward));
            let point =
                view.right * snap(w.x, grid) + view.up * snap(w.y, grid) + view.forward * depth;
            return Some((point, view.forward));
        }
        let ray = view.ray(px);
        if let Some(current) = existing {
            let f = view.forward.abs();
            let axis = if f.x >= f.y && f.x >= f.z {
                0
            } else if f.y >= f.z {
                1
            } else {
                2
            };
            let plane = BoxPlane {
                axis,
                offset: current[axis],
                sign: 1.0,
            };
            let mut p = plane.point_at(ray)?;
            for i in (0..3).filter(|&i| i != axis) {
                p[i] = snap(p[i], grid);
            }
            return Some((p, DVec3::ZERO));
        }
        let kinds = BTreeSet::from([ObjectKind::Brush]);
        let hidden = env.doc.hidden_ids();
        let hit = editor_cmd::ray_hits(env.doc, ray.0, ray.1, &kinds, &hidden, None)
            .into_iter()
            .next()?;
        Some((snap_to_face(hit.point, hit.normal, grid), hit.normal))
    }

    fn drive_move(
        &mut self,
        env: &mut PaneEnv,
        resp: &egui::Response,
        view: &View,
        zoom: f64,
        delta: egui::Vec2,
        escape: bool,
    ) {
        let ppp = resp.ctx.pixels_per_point();
        let Interaction::Move { accumulated, total } = &mut self.interaction else {
            return;
        };
        if escape {
            env.actions.push(Action::DragEnd { commit: false });
            self.interaction = Interaction::Cancelled;
            return;
        }
        *accumulated += DVec2::new(f64::from(delta.x * ppp), f64::from(delta.y * ppp));
        let du = snap(accumulated.x / zoom, env.grid);
        let dv = snap(-accumulated.y / zoom, env.grid);
        let next = view.right * du + view.up * dv;
        if next != *total {
            *total = next;
            env.actions.push(Action::DragTo(DragOp::Translate(next)));
        }
        if resp.drag_stopped_by(PointerButton::Primary) {
            env.actions.push(Action::DragEnd { commit: true });
            self.interaction = Interaction::Idle;
        }
    }

    /// The handles of the active element tool that project within a few pixels of `at`,
    /// so stacked ones (an edge seen end-on, two faces edge-on) move together.
    fn handles_near(env: &PaneEnv, view: &View, at: DVec2) -> Vec<ElementHandle> {
        let Some(kind) = env.tool.element_kind() else {
            return Vec::new();
        };
        elements::handles(env.doc, env.selection, kind)
            .into_iter()
            .filter(|h| {
                view.project(h.pos)
                    .is_some_and(|p| (p.px - at).length() <= HANDLE_PICK_PX)
            })
            .collect()
    }

    /// Drags the vertex, edge or face handle under the press along the axis plane that faces the camera most.
    fn element_persp(
        &mut self,
        env: &mut PaneEnv,
        resp: &egui::Response,
        view: &View,
        escape: bool,
        local: Option<Pos2>,
    ) {
        let grid = env.grid;
        let ray_of = |p: Pos2| view.ray(DVec2::new(f64::from(p.x), f64::from(p.y)));
        if resp.drag_started_by(PointerButton::Primary)
            && let Some(origin) = resp.ctx.input(|i| i.pointer.press_origin())
        {
            let ppp = resp.ctx.pixels_per_point();
            let l = (origin - resp.rect.min) * ppp;
            let ray = ray_of(pos2(l.x, l.y));
            let mut near =
                Self::handles_near(env, view, DVec2::new(f64::from(l.x), f64::from(l.y)));
            if near.is_empty() && env.tool == Tool::Face {
                near.extend(Self::face_under(env, ray));
            }
            let anchor = near
                .iter()
                .map(|h| h.pos)
                .min_by(|a, b| (*a - ray.0).length().total_cmp(&(*b - ray.0).length()));
            if let Some(anchor) = anchor {
                let f = view.forward.abs();
                let axis = if f.x >= f.y && f.x >= f.z {
                    0
                } else if f.y >= f.z {
                    1
                } else {
                    2
                };
                let plane = BoxPlane {
                    axis,
                    offset: anchor[axis],
                    sign: 1.0,
                };
                if let Some(start) = plane.point_at(ray) {
                    env.actions
                        .push(Action::VertexBegin(elements::merged_corners(&near)));
                    env.actions.push(Action::DragBegin { pivot: None });
                    self.interaction = Interaction::Element {
                        plane,
                        start,
                        total: DVec3::ZERO,
                    };
                }
            }
        }
        if let Interaction::Element {
            plane,
            start,
            total,
        } = &mut self.interaction
        {
            if escape {
                env.actions.push(Action::DragEnd { commit: false });
                self.interaction = Interaction::Cancelled;
                return;
            }
            if let Some(hit) = local.and_then(|l| plane.point_at(ray_of(l))) {
                let mut delta = hit - *start;
                for i in 0..3 {
                    delta[i] = if i == plane.axis {
                        0.0
                    } else {
                        snap(delta[i], grid)
                    };
                }
                if delta != *total {
                    *total = delta;
                    env.actions.push(Action::DragTo(DragOp::Translate(delta)));
                }
            }
            if resp.drag_stopped_by(PointerButton::Primary) {
                env.actions.push(Action::DragEnd { commit: true });
                self.interaction = Interaction::Idle;
            }
        }
        if matches!(self.interaction, Interaction::Cancelled) && !resp.dragged() {
            self.interaction = Interaction::Idle;
        }
    }

    /// The first pickable brush or mesh face under the ray.
    fn surface_face(env: &PaneEnv, ray: (DVec3, DVec3)) -> Option<(ObjectId, FaceId)> {
        let kinds = BTreeSet::from([ObjectKind::Brush, ObjectKind::Mesh]);
        let hidden = env.doc.hidden_ids();
        editor_cmd::ray_hits(env.doc, ray.0, ray.1, &kinds, &hidden, None)
            .into_iter()
            .filter(|h| env.doc.pickable(h.id, env.open_group))
            .find_map(|h| {
                let r = h.face?.parse::<editor_core::ElementRef>().ok()?;
                match r.part {
                    editor_core::Part::Face(f) => Some((h.id, f)),
                    _ => None,
                }
            })
    }

    /// The face of a selected brush under the ray, as a handle.
    fn face_under(env: &PaneEnv, ray: (DVec3, DVec3)) -> Option<ElementHandle> {
        let kinds = BTreeSet::from([ObjectKind::Brush]);
        let hidden = env.doc.hidden_ids();
        let hit = editor_cmd::ray_hits(env.doc, ray.0, ray.1, &kinds, &hidden, None)
            .into_iter()
            .next()?;
        let ObjectId::Brush(brush) = hit.id else {
            return None;
        };
        if !env.doc.with_members(env.selection).contains(&hit.id) {
            return None;
        }
        let editor_core::Part::Face(face) = hit.face?.parse::<editor_core::ElementRef>().ok()?.part
        else {
            return None;
        };
        elements::face_handle(env.doc, brush, face)
    }

    fn clip_overlay(
        painter: &egui::Painter,
        rect: Rect,
        view: &View,
        env: &PaneEnv,
        ppp: f32,
        accent: Color32,
        is_ortho: bool,
    ) {
        let to_screen = |p: DVec3| {
            view.project(p)
                .map(|q| rect.min + vec2(q.px.x as f32, q.px.y as f32) / ppp)
        };
        let dim = Color32::from_white_alpha(60);
        let other = Color32::from_rgb(0xe0, 0x9a, 0x3c);
        let (front, back) = match env.clip.keep {
            Keep::Front => (accent, dim),
            Keep::Back => (dim, accent),
            Keep::Both => (accent, other),
        };
        for (segments, color) in [
            (&env.clip_preview.back, back),
            (&env.clip_preview.front, front),
        ] {
            for &(a, b) in segments {
                if let (Some(a), Some(b)) = (to_screen(a), to_screen(b)) {
                    painter.line_segment([a, b], Stroke::new(1.5, color));
                }
            }
        }
        let points = &env.clip.points;
        if let [a, b, ..] = points.as_slice()
            && let (Some(pa), Some(pb)) = (to_screen(*a), to_screen(*b))
        {
            let stroke = Stroke::new(1.0, Color32::WHITE);
            if is_ortho && (pb - pa).length() > 0.5 {
                let d = (pb - pa).normalized();
                painter.line_segment([pa - d * 5000.0, pa + d * 5000.0], stroke);
            } else {
                painter.line_segment([pa, pb], stroke);
            }
            if let [_, _, c] = points.as_slice()
                && let Some(pc) = to_screen(*c)
            {
                painter.line_segment([pb, pc], stroke);
                painter.line_segment([pc, pa], stroke);
            }
        }
        for (i, p) in points.iter().enumerate() {
            let Some(at) = to_screen(*p) else { continue };
            painter.circle_filled(at, 5.0, accent);
            painter.circle_stroke(at, 5.0, Stroke::new(1.0, Color32::BLACK));
            painter.text(
                at + vec2(8.0, -8.0),
                Align2::LEFT_BOTTOM,
                format!("{}", i + 1),
                FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
        let hint = match points.len() {
            0 => "click to place the first clip point",
            1 => "click the second point",
            _ => "Enter apply · Tab keep · Backspace point · Esc clear",
        };
        painter.text(
            rect.center_bottom() + vec2(0.0, -6.0),
            Align2::CENTER_BOTTOM,
            hint,
            FontId::proportional(12.0),
            Color32::from_white_alpha(200),
        );
    }

    fn box_corners(
        &self,
        camera: &Camera,
        a: DVec2,
        b: DVec2,
        hint: Option<Aabb>,
    ) -> Option<(DVec3, DVec3)> {
        let view = nav::ortho_view(camera)?;
        let (u, v, w) = match view {
            OrthoView::Top => (0, 1, 2),
            OrthoView::Front => (0, 2, 1),
            OrthoView::Side => (1, 2, 0),
        };
        let (mut min, mut max) = ([0.0; 3], [0.0; 3]);
        min[u] = a.x.min(b.x);
        max[u] = a.x.max(b.x);
        min[v] = a.y.min(b.y);
        max[v] = a.y.max(b.y);
        (min[w], max[w]) = cuboid::range_along(w, hint);
        Some((DVec3::from(min), DVec3::from(max)))
    }

    #[allow(clippy::too_many_arguments)]
    fn overlays(
        &self,
        ui: &Ui,
        painter: &egui::Painter,
        rect: Rect,
        resp: &egui::Response,
        view: &View,
        camera: &Camera,
        env: &PaneEnv,
        ppp: f32,
        gizmo: Option<&Gizmo>,
    ) {
        let visuals = ui.visuals();
        let accent = visuals.selection.stroke.color;
        let label = match camera.projection {
            Projection::Perspective { fov } => format!("{}   fov {fov:.0}", self.id.title()),
            Projection::Orthographic { zoom, .. } => {
                format!("{}   {:.0}%", self.id.title(), zoom * 100.0)
            }
        };
        let font = FontId::proportional(12.0);
        painter.text(
            rect.min + vec2(8.0, 6.0),
            Align2::LEFT_TOP,
            label,
            font.clone(),
            Color32::from_white_alpha(170),
        );
        if let Some(c) = self.cursor {
            painter.text(
                rect.left_bottom() + vec2(8.0, -6.0),
                Align2::LEFT_BOTTOM,
                format!("{:.1}, {:.1}", c.x, c.y),
                FontId::monospace(11.5),
                Color32::from_white_alpha(150),
            );
        }
        if let Some(kind) = env.tool.element_kind() {
            let near = resp
                .hover_pos()
                .map(|p| (p - rect.min) * ppp)
                .map(|l| DVec2::new(f64::from(l.x), f64::from(l.y)));
            let dragging = matches!(
                self.interaction,
                Interaction::Move { .. } | Interaction::Element { .. }
            );
            for h in elements::handles(env.doc, env.selection, kind) {
                let Some(p) = view.project(h.pos) else {
                    continue;
                };
                let hot = if dragging {
                    h.corners.iter().all(|c| {
                        env.picks
                            .iter()
                            .any(|k| k.brush == c.brush && (k.at - c.at).length() <= 1e-3)
                    })
                } else {
                    near.is_some_and(|n| (p.px - n).length() <= HANDLE_PICK_PX)
                };
                let color = if hot { accent } else { Color32::WHITE };
                if hot {
                    for (a, b) in &h.outline {
                        if let (Some(a), Some(b)) = (view.project(*a), view.project(*b)) {
                            let to = |q: DVec2| rect.min + vec2(q.x as f32, q.y as f32) / ppp;
                            painter.line_segment([to(a.px), to(b.px)], Stroke::new(2.5, accent));
                        }
                    }
                }
                let at = rect.min + vec2(p.px.x as f32, p.px.y as f32) / ppp;
                match kind {
                    Kind::Vertex => {
                        let square = Rect::from_center_size(at, vec2(7.0, 7.0));
                        painter.rect_filled(square, 1.0, color);
                        painter.rect_stroke(
                            square,
                            1.0,
                            Stroke::new(1.0, Color32::BLACK),
                            StrokeKind::Inside,
                        );
                    }
                    Kind::Edge => {
                        let r = 4.5;
                        painter.add(egui::Shape::convex_polygon(
                            vec![
                                at + vec2(0.0, -r),
                                at + vec2(r, 0.0),
                                at + vec2(0.0, r),
                                at + vec2(-r, 0.0),
                            ],
                            color,
                            Stroke::new(1.0, Color32::BLACK),
                        ));
                    }
                    Kind::Face => {
                        painter.circle_filled(at, 4.5, color);
                        painter.circle_stroke(at, 4.5, Stroke::new(1.0, Color32::BLACK));
                    }
                }
            }
        }
        if env.tool == Tool::Clip {
            Self::clip_overlay(
                painter,
                rect,
                view,
                env,
                ppp,
                accent,
                nav::ortho_view(camera).is_some(),
            );
        }
        if let Some(b) = env.pending
            && !matches!(self.interaction, Interaction::Box { .. })
        {
            let to_screen = |p: DVec2| rect.min + vec2(p.x as f32, p.y as f32) / ppp;
            for (p, q) in cuboid::edges(&b) {
                if let (Some(p), Some(q)) = (view.project(p), view.project(q)) {
                    painter
                        .line_segment([to_screen(p.px), to_screen(q.px)], Stroke::new(1.5, accent));
                }
            }
        }
        match &self.interaction {
            Interaction::Box { start, now } => {
                let a = Self::ortho_to_screen(view, *start, rect, ppp);
                let b = Self::ortho_to_screen(view, *now, rect, ppp);
                let r = Rect::from_two_pos(a, b);
                painter.rect_filled(r, 0.0, accent.gamma_multiply(0.2));
                painter.rect_stroke(r, 0.0, Stroke::new(1.5, accent), StrokeKind::Inside);
                painter.text(
                    r.right_bottom() + vec2(6.0, 4.0),
                    Align2::LEFT_TOP,
                    format!(
                        "{:.0} x {:.0}",
                        (now.x - start.x).abs(),
                        (now.y - start.y).abs()
                    ),
                    FontId::monospace(11.5),
                    Color32::WHITE,
                );
            }
            Interaction::BoxPlane { plane, start, now } => {
                let size = plane
                    .cuboid(
                        *start,
                        *now,
                        cuboid::depth_along(plane.axis, env.depth_hint),
                    )
                    .size();
                painter.text(
                    rect.center_top() + vec2(0.0, 6.0),
                    Align2::CENTER_TOP,
                    format!("box  {:.0} x {:.0} x {:.0}", size.x, size.y, size.z),
                    FontId::monospace(12.0),
                    Color32::WHITE,
                );
            }
            Interaction::Rotate { degrees, .. } => {
                painter.text(
                    rect.center_top() + vec2(0.0, 6.0),
                    Align2::CENTER_TOP,
                    format!("rotate  {degrees:.0}°"),
                    FontId::monospace(12.0),
                    Color32::WHITE,
                );
            }
            Interaction::Scale { size, factors, .. } => {
                painter.text(
                    rect.center_top() + vec2(0.0, 6.0),
                    Align2::CENTER_TOP,
                    format!(
                        "scale  {:.0} x {:.0}",
                        size.x * factors.x,
                        size.y * factors.y
                    ),
                    FontId::monospace(12.0),
                    Color32::WHITE,
                );
            }
            Interaction::Move { total, .. } => {
                painter.text(
                    rect.center_top() + vec2(0.0, 6.0),
                    Align2::CENTER_TOP,
                    format!("move  {:.0}, {:.0}, {:.0}", total.x, total.y, total.z),
                    FontId::monospace(12.0),
                    Color32::WHITE,
                );
            }
            _ => {}
        }
        if let Some(g) = gizmo {
            g.paint(painter, rect.min, ppp, self.gizmo_hover);
        }
        let stroke = if resp.has_focus() {
            Stroke::new(1.5, accent)
        } else if resp.hovered() {
            Stroke::new(1.0, accent.gamma_multiply(0.35))
        } else {
            Stroke::NONE
        };
        painter.rect_stroke(rect.shrink(0.5), 0.0, stroke, StrokeKind::Inside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapping_on_a_slanted_face_stays_on_the_plane() {
        let normal = DVec3::new(1.0, 0.0, 1.0).normalize();
        let point = DVec3::new(3.3, 5.1, 7.2);
        let p = snap_to_face(point, normal, 1.0);
        assert!(normal.dot(p - point).abs() < 1e-9);
        assert_eq!(p.y, 5.0);
    }

    #[test]
    fn snapping_on_an_axis_face_snaps_both_other_axes() {
        let p = snap_to_face(DVec3::new(3.4, 5.6, 8.0), DVec3::Z, 2.0);
        assert_eq!(p, DVec3::new(4.0, 6.0, 8.0));
    }
}

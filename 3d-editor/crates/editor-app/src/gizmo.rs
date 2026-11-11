use std::f64::consts::TAU;

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, vec2};
use glam::{DVec2, DVec3};
use level_geom::camera::{View, ViewKind};

use crate::drag::{DragOp, wrap_degrees};
use crate::pane::snap;

const HANDLE_PX: f64 = 96.0;
const HIT_PX: f64 = 8.0;
const RING_STEPS: usize = 64;
const RING_RADIUS: f64 = 0.8;
const ROTATE_SNAP: f64 = 15.0;
const HOVER: Color32 = Color32::from_rgb(0xff, 0xd2, 0x4d);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Move,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    X,
    Y,
    Z,
}

impl Handle {
    pub const ALL: [Handle; 3] = [Handle::X, Handle::Y, Handle::Z];

    pub fn dir(self) -> DVec3 {
        match self {
            Handle::X => DVec3::X,
            Handle::Y => DVec3::Y,
            Handle::Z => DVec3::Z,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn color(self) -> Color32 {
        match self {
            Handle::X => Color32::from_rgb(0xe5, 0x4b, 0x4b),
            Handle::Y => Color32::from_rgb(0x5c, 0xc8, 0x5c),
            Handle::Z => Color32::from_rgb(0x4d, 0x80, 0xf2),
        }
    }

    /// Unit vectors spanning the plane normal to the axis, with `u × v` along the axis.
    fn basis(self) -> (DVec3, DVec3) {
        match self {
            Handle::X => (DVec3::Y, DVec3::Z),
            Handle::Y => (DVec3::Z, DVec3::X),
            Handle::Z => (DVec3::X, DVec3::Y),
        }
    }
}

/// A move, rotate or scale gizmo at a pivot, drawn at a constant size on screen.
pub struct Gizmo {
    pub mode: Mode,
    pivot: DVec3,
    view: View,
    len: f64,
    size: DVec3,
}

impl Gizmo {
    /// `size` is the extent of the selection, which scaling snaps against.
    pub fn new(pivot: DVec3, mode: Mode, view: &View, size: DVec3) -> Gizmo {
        let per_px = match view.kind {
            ViewKind::Perspective { fov_y } => {
                let depth = (pivot - view.eye).dot(view.forward).max(1.0);
                2.0 * (fov_y.to_radians() / 2.0).tan() * depth / view.height
            }
            ViewKind::Orthographic { zoom } => 1.0 / zoom,
        };
        Gizmo {
            mode,
            pivot,
            view: *view,
            len: HANDLE_PX * per_px,
            size,
        }
    }

    pub fn pivot(&self) -> DVec3 {
        self.pivot
    }

    fn px(&self, p: DVec3) -> Option<DVec2> {
        self.view.project(p).map(|p| p.px)
    }

    fn ring_point(&self, handle: Handle, k: usize) -> DVec3 {
        let (u, v) = handle.basis();
        let a = k as f64 / RING_STEPS as f64 * TAU;
        self.pivot + (u * a.cos() + v * a.sin()) * self.len * RING_RADIUS
    }

    fn outline(&self, handle: Handle) -> Vec<DVec3> {
        match self.mode {
            Mode::Rotate => (0..=RING_STEPS)
                .map(|k| self.ring_point(handle, k))
                .collect(),
            Mode::Move | Mode::Scale => vec![self.pivot, self.pivot + handle.dir() * self.len],
        }
    }

    /// The handle under a pixel (row 0 at the top) of the view the gizmo was built for.
    pub fn hit(&self, cursor: DVec2) -> Option<Handle> {
        Handle::ALL
            .into_iter()
            .filter_map(|handle| {
                let points: Vec<DVec2> = self
                    .outline(handle)
                    .into_iter()
                    .filter_map(|p| self.px(p))
                    .collect();
                let distance = points
                    .windows(2)
                    .map(|w| distance_to_segment(cursor, w[0], w[1]))
                    .fold(f64::INFINITY, f64::min);
                (distance <= HIT_PX).then_some((handle, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(handle, _)| handle)
    }

    /// Where a ray touches a handle: distance from the pivot along the axis (move, scale) or the angle
    /// in degrees around it (rotate).
    pub fn value(&self, handle: Handle, (origin, dir): (DVec3, DVec3)) -> Option<f64> {
        match self.mode {
            Mode::Move | Mode::Scale => {
                let axis = handle.dir();
                let b = axis.dot(dir);
                let denom = 1.0 - b * b;
                if denom.abs() < 1e-6 {
                    return None;
                }
                let w = self.pivot - origin;
                Some((b * dir.dot(w) - axis.dot(w)) / denom)
            }
            Mode::Rotate => {
                let normal = handle.dir();
                let facing = dir.dot(normal);
                if facing.abs() < 1e-6 {
                    return None;
                }
                let t = (self.pivot - origin).dot(normal) / facing;
                if t < 0.0 {
                    return None;
                }
                let offset = origin + dir * t - self.pivot;
                let (u, v) = handle.basis();
                Some(offset.dot(v).atan2(offset.dot(u)).to_degrees())
            }
        }
    }

    pub fn begin(&self, handle: Handle, ray: (DVec3, DVec3)) -> Option<GizmoDrag> {
        let start = self.value(handle, ray)?;
        Some(GizmoDrag {
            handle,
            start,
            last: start,
            turned: 0.0,
        })
    }

    pub fn paint(&self, painter: &Painter, origin: Pos2, ppp: f32, hovered: Option<Handle>) {
        let to_screen = |p: DVec2| origin + vec2(p.x as f32, p.y as f32) / ppp;
        for handle in Handle::ALL {
            let lit = hovered == Some(handle);
            let color = if lit { HOVER } else { handle.color() };
            let stroke = Stroke::new(if lit { 3.0 } else { 2.0 }, color);
            let points: Vec<Pos2> = self
                .outline(handle)
                .into_iter()
                .filter_map(|p| self.px(p))
                .map(to_screen)
                .collect();
            for pair in points.windows(2) {
                painter.line_segment([pair[0], pair[1]], stroke);
            }
            let (Some(&tip), Some(&base)) = (points.last(), points.first()) else {
                continue;
            };
            match self.mode {
                Mode::Move => {
                    let along = (tip - base).normalized();
                    let across = vec2(-along.y, along.x);
                    painter.add(Shape::convex_polygon(
                        vec![tip + along * 12.0, tip + across * 5.0, tip - across * 5.0],
                        color,
                        Stroke::NONE,
                    ));
                }
                Mode::Scale => {
                    painter.rect_filled(Rect::from_center_size(tip, vec2(9.0, 9.0)), 0.0, color);
                }
                Mode::Rotate => {}
            }
        }
    }
}

/// One drag of a handle; turns pointer rays into the running total of the drag.
pub struct GizmoDrag {
    pub handle: Handle,
    start: f64,
    last: f64,
    turned: f64,
}

impl GizmoDrag {
    pub fn op(
        &mut self,
        gizmo: &Gizmo,
        ray: (DVec3, DVec3),
        grid: f64,
        free: bool,
    ) -> Option<DragOp> {
        let now = gizmo.value(self.handle, ray)?;
        let index = self.handle.index();
        match gizmo.mode {
            Mode::Move => Some(DragOp::Translate(
                self.handle.dir() * snap(now - self.start, grid),
            )),
            Mode::Rotate => {
                self.turned += wrap_degrees(now - self.last);
                self.last = now;
                let degrees = if free {
                    self.turned
                } else {
                    snap(self.turned, ROTATE_SNAP)
                };
                Some(DragOp::Rotate {
                    axis: self.handle.dir(),
                    degrees,
                })
            }
            Mode::Scale => {
                if self.start.abs() < 1e-3 {
                    return None;
                }
                let ratio = (now / self.start).max(0.0);
                let size = gizmo.size[index];
                let factor = if size > 1e-9 && grid > 0.0 {
                    snap(size * ratio, grid).max(grid) / size
                } else {
                    ratio.max(1e-3)
                };
                let mut factors = DVec3::ONE;
                factors[index] = factor;
                Some(DragOp::Scale(factors))
            }
        }
    }
}

fn distance_to_segment(p: DVec2, a: DVec2, b: DVec2) -> f64 {
    let ab = b - a;
    let len2 = ab.length_squared();
    if len2 < 1e-12 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

#[cfg(test)]
mod tests {
    use level_geom::Aabb;
    use level_geom::camera::viewport_camera;

    use super::*;

    const SIZE: (u32, u32) = (800, 600);

    fn gizmo(mode: Mode) -> Gizmo {
        let bounds = Aabb {
            min: DVec3::new(-64.0, -64.0, 0.0),
            max: DVec3::new(64.0, 64.0, 64.0),
        };
        let camera = viewport_camera("viewport.persp", Some(&bounds), SIZE.0, SIZE.1).unwrap();
        let view = View::new(&camera, SIZE.0, SIZE.1, bounds.reach());
        Gizmo::new(bounds.center(), mode, &view, bounds.max - bounds.min)
    }

    fn ray_at(g: &Gizmo, world: DVec3) -> (DVec3, DVec3) {
        g.view.ray(g.px(world).unwrap())
    }

    #[test]
    fn a_pixel_on_an_axis_hits_that_handle() {
        let g = gizmo(Mode::Move);
        for handle in Handle::ALL {
            let px = g.px(g.pivot + handle.dir() * g.len * 0.9).unwrap();
            assert_eq!(g.hit(px), Some(handle));
        }
        assert_eq!(g.hit(DVec2::new(2.0, 2.0)), None);
    }

    #[test]
    fn a_pixel_on_a_ring_hits_that_ring() {
        let g = gizmo(Mode::Rotate);
        for handle in Handle::ALL {
            let px = g.px(g.ring_point(handle, 9)).unwrap();
            assert_eq!(g.hit(px), Some(handle));
        }
    }

    #[test]
    fn an_axis_ray_reads_the_distance_along_the_axis() {
        let g = gizmo(Mode::Move);
        for handle in Handle::ALL {
            let value = g
                .value(handle, ray_at(&g, g.pivot + handle.dir() * g.len * 0.7))
                .unwrap();
            assert!(
                (value - g.len * 0.7).abs() < 1e-6 * g.len,
                "{handle:?}: {value}"
            );
        }
    }

    #[test]
    fn a_ring_ray_reads_the_angle_around_the_axis() {
        let g = gizmo(Mode::Rotate);
        for handle in Handle::ALL {
            let at = |k: usize| {
                g.value(handle, ray_at(&g, g.ring_point(handle, k)))
                    .unwrap()
            };
            assert!(at(0).abs() < 1e-6, "{handle:?}");
            assert!((at(RING_STEPS / 4) - 90.0).abs() < 1e-6, "{handle:?}");
        }
    }

    #[test]
    fn moving_snaps_to_the_grid() {
        let g = gizmo(Mode::Move);
        let from = g.pivot + DVec3::X * g.len * 0.5;
        let mut drag = g.begin(Handle::X, ray_at(&g, from)).unwrap();
        let op = drag
            .op(&g, ray_at(&g, from + DVec3::X * 17.0), 8.0, false)
            .unwrap();
        let DragOp::Translate(d) = op else {
            panic!("{op:?}")
        };
        assert!((d - DVec3::X * 16.0).length() < 1e-6, "{d}");
    }

    #[test]
    fn rotating_accumulates_through_the_branch_cut_and_snaps() {
        let g = gizmo(Mode::Rotate);
        let mut drag = g
            .begin(Handle::Z, ray_at(&g, g.ring_point(Handle::Z, 28)))
            .unwrap();
        let mut last = 0.0;
        for k in [30, 32, 34, 36, 38] {
            let op = drag
                .op(&g, ray_at(&g, g.ring_point(Handle::Z, k)), 0.0, false)
                .unwrap();
            let DragOp::Rotate { degrees, .. } = op else {
                panic!("{op:?}")
            };
            assert!(degrees >= last, "{degrees} after {last}");
            assert_eq!(degrees % ROTATE_SNAP, 0.0);
            last = degrees;
        }
        assert!(last > 45.0, "turned {last}");
    }

    #[test]
    fn scaling_snaps_the_resulting_size() {
        let g = gizmo(Mode::Scale);
        let from = g.pivot + DVec3::X * g.len;
        let mut drag = g.begin(Handle::X, ray_at(&g, from)).unwrap();
        let op = drag
            .op(
                &g,
                ray_at(&g, g.pivot + DVec3::X * g.len * 1.5),
                16.0,
                false,
            )
            .unwrap();
        let DragOp::Scale(f) = op else {
            panic!("{op:?}")
        };
        let size = 128.0 * f.x;
        assert_eq!(size % 16.0, 0.0, "{size}");
        assert_eq!((f.y, f.z), (1.0, 1.0));
    }
}

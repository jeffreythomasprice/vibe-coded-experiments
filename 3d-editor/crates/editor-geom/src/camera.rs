use editor_core::{Camera, CameraId, Extra, OrthoView, Projection};
use glam::{DMat4, DVec2, DVec3, DVec4};

use crate::aabb::Aabb;

pub const REFERENCE_WIDTH: u32 = 1024;
pub const REFERENCE_HEIGHT: u32 = 768;
pub const MAX_IMAGE_DIM: u32 = 4096;
pub const MAX_PITCH: f64 = 89.0;
pub const DEFAULT_FOV: f64 = 60.0;
pub const VIEWPORTS: [&str; 4] = [
    "viewport.persp",
    "viewport.top",
    "viewport.front",
    "viewport.side",
];

const FRAME_MARGIN: f64 = 1.1;

pub fn forward_from_angles(yaw: f64, pitch: f64) -> DVec3 {
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    DVec3::new(p.cos() * y.cos(), p.cos() * y.sin(), p.sin())
}

/// `(forward, right, up)` of an orthographic view.
pub fn ortho_axes(view: OrthoView) -> (DVec3, DVec3, DVec3) {
    match view {
        OrthoView::Top => (DVec3::NEG_Z, DVec3::X, DVec3::Y),
        OrthoView::Front => (DVec3::Y, DVec3::X, DVec3::Z),
        OrthoView::Side => (DVec3::NEG_X, DVec3::Y, DVec3::Z),
    }
}

/// Yaw and pitch in degrees that aim `eye` at `target`; `None` if they coincide.
pub fn look_at(eye: DVec3, target: DVec3) -> Option<(f64, f64)> {
    let d = target - eye;
    let len = d.length();
    if len < 1e-9 {
        return None;
    }
    let yaw = d.y.atan2(d.x).to_degrees();
    let pitch = (d.z / len).asin().to_degrees().clamp(-MAX_PITCH, MAX_PITCH);
    Some((yaw, pitch))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewKind {
    Perspective { fov_y: f64 },
    Orthographic { zoom: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projected {
    pub px: DVec2,
    pub depth: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub eye: DVec3,
    pub forward: DVec3,
    pub right: DVec3,
    pub up: DVec3,
    pub kind: ViewKind,
    pub width: f64,
    pub height: f64,
    pub near: f64,
    pub far: f64,
}

impl View {
    /// `reach` is the largest absolute coordinate in the scene; it sizes the clip range.
    pub fn new(camera: &Camera, width: u32, height: u32, reach: f64) -> View {
        let (width, height) = (width.max(1) as f64, height.max(1) as f64);
        let reach = reach.max(1.0);
        match camera.projection {
            Projection::Perspective { fov } => {
                let forward = forward_from_angles(camera.yaw, camera.pitch);
                let right = forward.cross(DVec3::Z).normalize();
                let up = right.cross(forward);
                let eye = camera.position;
                View {
                    eye,
                    forward,
                    right,
                    up,
                    kind: ViewKind::Perspective { fov_y: fov },
                    width,
                    height,
                    near: 0.5,
                    far: (eye.length() + reach * 1.8) * 1.1 + 100.0,
                }
            }
            Projection::Orthographic { view, zoom } => {
                let (forward, right, up) = ortho_axes(view);
                let plane_point = camera.position - forward * camera.position.dot(forward);
                let back = reach * 1.8 + 64.0;
                View {
                    eye: plane_point - forward * back,
                    forward,
                    right,
                    up,
                    kind: ViewKind::Orthographic { zoom },
                    width,
                    height,
                    near: 1.0,
                    far: back * 2.0,
                }
            }
        }
    }

    pub fn is_ortho(&self) -> bool {
        matches!(self.kind, ViewKind::Orthographic { .. })
    }

    pub fn view_proj(&self) -> DMat4 {
        let (r, u, f, e) = (self.right, self.up, self.forward, self.eye);
        #[rustfmt::skip]
        let view = DMat4::from_cols_array(&[
            r.x, u.x, -f.x, 0.0,
            r.y, u.y, -f.y, 0.0,
            r.z, u.z, -f.z, 0.0,
            -r.dot(e), -u.dot(e), f.dot(e), 1.0,
        ]);
        let (n, fa) = (self.near, self.far);
        #[rustfmt::skip]
        let proj = match self.kind {
            ViewKind::Perspective { fov_y } => {
                let s = 1.0 / (fov_y.to_radians() * 0.5).tan();
                DMat4::from_cols_array(&[
                    s * self.height / self.width, 0.0, 0.0, 0.0,
                    0.0, s, 0.0, 0.0,
                    0.0, 0.0, fa / (n - fa), -1.0,
                    0.0, 0.0, n * fa / (n - fa), 0.0,
                ])
            }
            ViewKind::Orthographic { zoom } => DMat4::from_cols_array(&[
                2.0 * zoom / self.width, 0.0, 0.0, 0.0,
                0.0, 2.0 * zoom / self.height, 0.0, 0.0,
                0.0, 0.0, 1.0 / (n - fa), 0.0,
                0.0, 0.0, n / (n - fa), 1.0,
            ]),
        };
        proj * view
    }

    /// Pixel position (row 0 at the top) and NDC depth; `None` for points behind a perspective eye.
    pub fn project(&self, p: DVec3) -> Option<Projected> {
        let clip = self.view_proj() * DVec4::new(p.x, p.y, p.z, 1.0);
        if clip.w <= 1e-9 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(Projected {
            px: DVec2::new(
                (ndc.x + 1.0) * 0.5 * self.width,
                (1.0 - ndc.y) * 0.5 * self.height,
            ),
            depth: ndc.z,
        })
    }

    /// World-space ray through a pixel (row 0 at the top): origin and unit direction. The inverse of `project`.
    pub fn ray(&self, px: DVec2) -> (DVec3, DVec3) {
        let ndc_x = px.x / self.width * 2.0 - 1.0;
        let ndc_y = 1.0 - px.y / self.height * 2.0;
        match self.kind {
            ViewKind::Perspective { fov_y } => {
                let tan_y = (fov_y.to_radians() * 0.5).tan();
                let tan_x = tan_y * self.width / self.height;
                let dir = (self.forward + self.right * (ndc_x * tan_x) + self.up * (ndc_y * tan_y))
                    .normalize();
                (self.eye, dir)
            }
            ViewKind::Orthographic { zoom } => {
                let origin = self.eye
                    + self.right * (ndc_x * self.width / (2.0 * zoom))
                    + self.up * (ndc_y * self.height / (2.0 * zoom));
                (origin, self.forward)
            }
        }
    }

    /// Visible world range along `(right, up)` of an orthographic view.
    pub fn ortho_extent(&self) -> Option<(DVec2, DVec2)> {
        let ViewKind::Orthographic { zoom } = self.kind else {
            return None;
        };
        let centre = DVec2::new(self.eye.dot(self.right), self.eye.dot(self.up));
        let half = DVec2::new(self.width, self.height) / (2.0 * zoom);
        Some((centre - half, centre + half))
    }
}

fn base_camera(name: &str, projection: Projection, yaw: f64, pitch: f64) -> Camera {
    Camera {
        id: CameraId(0),
        name: name.to_string(),
        position: DVec3::ZERO,
        yaw,
        pitch,
        projection,
        extra: Extra::new(),
    }
}

/// Moves a camera so `bounds` fills the image, keeping its orientation (and fov).
pub fn frame_camera(camera: &Camera, bounds: &Aabb, width: u32, height: u32) -> Camera {
    let (w, h) = (width.max(1) as f64, height.max(1) as f64);
    let mut out = camera.clone();
    match camera.projection {
        Projection::Perspective { fov } => {
            let forward = forward_from_angles(camera.yaw, camera.pitch);
            let right = forward.cross(DVec3::Z).normalize();
            let up = right.cross(forward);
            let tan_y = (fov.to_radians() * 0.5).tan();
            let tan_x = tan_y * w / h;
            let centre = bounds.center();
            let mut back = 1.0_f64;
            for corner in bounds.corners() {
                let d = corner - centre;
                let ahead = d.dot(forward);
                back = back
                    .max(d.dot(right).abs() * FRAME_MARGIN / tan_x - ahead)
                    .max(d.dot(up).abs() * FRAME_MARGIN / tan_y - ahead)
                    .max(1.0 - ahead);
            }
            out.position = centre - forward * back;
        }
        Projection::Orthographic { view, .. } => {
            let (_, right, up) = ortho_axes(view);
            let size = bounds.size();
            let eu = size.dot(right.abs()).max(1.0);
            let ev = size.dot(up.abs()).max(1.0);
            let zoom = (w / (eu * FRAME_MARGIN)).min(h / (ev * FRAME_MARGIN));
            out.position = bounds.center();
            out.projection = Projection::Orthographic { view, zoom };
        }
    }
    out
}

const EMPTY_SCENE: Aabb = Aabb {
    min: DVec3::splat(-64.0),
    max: DVec3::splat(64.0),
};

/// The session cameras `viewport.persp|top|front|side`, framed on `bounds`.
pub fn viewport_camera(
    name: &str,
    bounds: Option<&Aabb>,
    width: u32,
    height: u32,
) -> Option<Camera> {
    let base = match name {
        "viewport.persp" => base_camera(
            name,
            Projection::Perspective { fov: DEFAULT_FOV },
            45.0,
            -30.0,
        ),
        "viewport.top" => base_camera(
            name,
            Projection::Orthographic {
                view: OrthoView::Top,
                zoom: 1.0,
            },
            0.0,
            0.0,
        ),
        "viewport.front" => base_camera(
            name,
            Projection::Orthographic {
                view: OrthoView::Front,
                zoom: 1.0,
            },
            0.0,
            0.0,
        ),
        "viewport.side" => base_camera(
            name,
            Projection::Orthographic {
                view: OrthoView::Side,
                zoom: 1.0,
            },
            0.0,
            0.0,
        ),
        _ => return None,
    };
    Some(frame_camera(
        &base,
        bounds.unwrap_or(&EMPTY_SCENE),
        width,
        height,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Aabb {
        Aabb {
            min: DVec3::new(-30.0, 10.0, 0.0),
            max: DVec3::new(90.0, 50.0, 64.0),
        }
    }

    fn assert_corners_inside(view: &View, b: &Aabb) {
        for c in b.corners() {
            let p = view.project(c).expect("in front of the eye");
            assert!(
                p.px.x >= 0.0 && p.px.x <= view.width && p.px.y >= 0.0 && p.px.y <= view.height,
                "{c} projected to {}",
                p.px
            );
            assert!((-1e-9..=1.0 + 1e-9).contains(&p.depth), "depth {}", p.depth);
        }
    }

    #[test]
    fn ray_through_a_projected_point_hits_it() {
        let b = bounds();
        let target = b.center() + DVec3::new(7.0, -3.0, 5.0);
        for name in VIEWPORTS {
            let cam = viewport_camera(name, Some(&b), 800, 600).unwrap();
            let view = View::new(&cam, 800, 600, b.reach());
            let px = view.project(target).unwrap().px;
            let (origin, dir) = view.ray(px);
            assert!((dir.length() - 1.0).abs() < 1e-9);
            let t = (target - origin).dot(dir);
            let closest = origin + dir * t;
            assert!(
                closest.distance(target) < 1e-6,
                "{name}: ray misses by {}",
                closest.distance(target)
            );
        }
    }

    #[test]
    fn framed_bounds_fit_in_every_viewport() {
        let b = bounds();
        for name in VIEWPORTS {
            for (w, h) in [(1024, 768), (300, 900), (900, 300)] {
                let cam = viewport_camera(name, Some(&b), w, h).unwrap();
                let view = View::new(&cam, w, h, b.reach());
                assert_corners_inside(&view, &b);
            }
        }
    }

    #[test]
    fn framing_is_tight_in_the_limiting_axis() {
        let b = bounds();
        let cam = viewport_camera("viewport.top", Some(&b), 1000, 1000).unwrap();
        let view = View::new(&cam, 1000, 1000, b.reach());
        let lo = view.project(b.min).unwrap().px;
        let hi = view.project(b.max).unwrap().px;
        assert!(
            (lo.x - 1000.0 * (1.0 - 1.0 / 1.1) / 2.0).abs() < 1e-6,
            "{lo}"
        );
        assert!(
            (hi.x - 1000.0 * (1.0 + 1.0 / 1.1) / 2.0).abs() < 1e-6,
            "{hi}"
        );
    }

    #[test]
    fn ortho_axes_match_the_documented_orientation() {
        for (name, dx, dy) in [
            ("viewport.top", DVec3::X, DVec3::Y),
            ("viewport.front", DVec3::X, DVec3::Z),
            ("viewport.side", DVec3::Y, DVec3::Z),
        ] {
            let cam = viewport_camera(name, Some(&bounds()), 800, 600).unwrap();
            let view = View::new(&cam, 800, 600, 200.0);
            let origin = view.project(DVec3::ZERO).unwrap().px;
            let right = view.project(dx * 10.0).unwrap().px;
            let up = view.project(dy * 10.0).unwrap().px;
            assert!(
                right.x > origin.x && (right.y - origin.y).abs() < 1e-9,
                "{name}"
            );
            assert!(up.y < origin.y && (up.x - origin.x).abs() < 1e-9, "{name}");
        }
    }

    #[test]
    fn yaw_zero_looks_along_x_with_y_on_the_left() {
        let cam = Camera {
            position: DVec3::new(-100.0, 0.0, 0.0),
            ..base_camera("t", Projection::Perspective { fov: 60.0 }, 0.0, 0.0)
        };
        let view = View::new(&cam, 800, 600, 200.0);
        let centre = view.project(DVec3::ZERO).unwrap().px;
        let left = view.project(DVec3::new(0.0, 10.0, 0.0)).unwrap().px;
        assert!((centre.x - 400.0).abs() < 1e-6 && (centre.y - 300.0).abs() < 1e-6);
        assert!(left.x < centre.x);
    }

    #[test]
    fn look_at_round_trips() {
        let eye = DVec3::new(10.0, -20.0, 30.0);
        for target in [
            DVec3::ZERO,
            DVec3::new(50.0, 40.0, -10.0),
            DVec3::new(-5.0, 7.0, 90.0),
        ] {
            let (yaw, pitch) = look_at(eye, target).unwrap();
            let f = forward_from_angles(yaw, pitch);
            let want = (target - eye).normalize();
            assert!(f.dot(want) > 1.0 - 1e-12, "{target}");
        }
        assert_eq!(look_at(eye, eye), None);
    }

    #[test]
    fn behind_the_eye_does_not_project() {
        let cam = base_camera("t", Projection::Perspective { fov: 60.0 }, 0.0, 0.0);
        let view = View::new(&cam, 100, 100, 100.0);
        assert_eq!(view.project(DVec3::new(-5.0, 0.0, 0.0)), None);
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn wide_scene_fits_every_corner() {
        let bounds = Aabb {
            min: DVec3::new(-1000.0, -100.0, 0.0),
            max: DVec3::new(1000.0, 100.0, 120.0),
        };
        let camera = viewport_camera("viewport.persp", Some(&bounds), 1024, 768).unwrap();
        let view = View::new(&camera, 1024, 768, bounds.reach());
        for corner in bounds.corners() {
            let p = view.project(corner).expect("in front of the eye");
            assert!((0.0..1024.0).contains(&p.px.x), "{corner} -> {}", p.px);
            assert!((0.0..768.0).contains(&p.px.y), "{corner} -> {}", p.px);
        }
        let sphere = bounds.radius() * FRAME_MARGIN;
        let distance = (camera.position - bounds.center()).length();
        assert!(distance < sphere / ((DEFAULT_FOV.to_radians() * 0.5).tan().atan()).sin());
    }
}

use editor_core::{Camera, OrthoView, Projection};
use editor_geom::camera::ortho_axes;
pub use editor_geom::camera::{fly, forward, look, right_up};
use glam::DVec3;

const MIN_ZOOM: f64 = 0.01;
const MAX_ZOOM: f64 = 200.0;

pub fn ortho_view(camera: &Camera) -> Option<OrthoView> {
    match camera.projection {
        Projection::Orthographic { view, .. } => Some(view),
        Projection::Perspective { .. } => None,
    }
}

pub fn zoom_of(camera: &Camera) -> f64 {
    match camera.projection {
        Projection::Orthographic { zoom, .. } => zoom,
        Projection::Perspective { .. } => 1.0,
    }
}

/// Moves the camera so the scene follows a drag of `(dx, dy)` pixels.
pub fn pan_ortho(camera: &mut Camera, dx: f64, dy: f64) {
    let Some(view) = ortho_view(camera) else {
        return;
    };
    let (_, right, up) = ortho_axes(view);
    let zoom = zoom_of(camera);
    camera.position += -right * dx / zoom + up * dy / zoom;
}

/// Zooms by `factor`, keeping the world point under the cursor (`cx`, `cy` pixels from the view centre, y down) fixed.
pub fn zoom_ortho_at(camera: &mut Camera, cx: f64, cy: f64, factor: f64) {
    let Some(view) = ortho_view(camera) else {
        return;
    };
    let (_, right, up) = ortho_axes(view);
    let old = zoom_of(camera);
    let new = (old * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    let shift = (right * cx - up * cy) * (1.0 / old - 1.0 / new);
    camera.position += shift;
    camera.projection = Projection::Orthographic { view, zoom: new };
}

/// Sets the zoom of an orthographic camera, keeping what is at its centre.
pub fn set_ortho_zoom(camera: &mut Camera, zoom: f64) {
    if let Some(view) = ortho_view(camera) {
        camera.projection = Projection::Orthographic {
            view,
            zoom: zoom.clamp(MIN_ZOOM, MAX_ZOOM),
        };
    }
}

pub fn pan_persp(camera: &mut Camera, dx: f64, dy: f64, units_per_pixel: f64) {
    let (right, up) = right_up(camera);
    camera.position += (-right * dx + up * dy) * units_per_pixel;
}

pub fn orbit(camera: &mut Camera, pivot: DVec3, dx: f64, dy: f64) {
    let distance = (camera.position - pivot).length().max(1.0);
    look(camera, dx, dy);
    camera.position = pivot - forward(camera) * distance;
}

pub fn dolly(camera: &mut Camera, pivot: DVec3, scroll: f64) {
    let distance = (camera.position - pivot).length().max(8.0);
    let step = scroll * distance * 0.0015;
    camera.position += forward(camera) * step;
}

#[cfg(test)]
mod tests {
    use editor_core::{CameraId, Extra};
    use glam::DVec3;

    use super::*;

    fn top(zoom: f64) -> Camera {
        Camera {
            id: CameraId(0),
            name: "t".into(),
            position: DVec3::new(10.0, 20.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            projection: Projection::Orthographic {
                view: OrthoView::Top,
                zoom,
            },
            extra: Extra::new(),
        }
    }

    #[test]
    fn setting_the_zoom_keeps_the_centre_and_stays_in_range() {
        let mut c = top(1.0);
        set_ortho_zoom(&mut c, 4.0);
        assert_eq!(zoom_of(&c), 4.0);
        assert_eq!(c.position, DVec3::new(10.0, 20.0, 0.0));
        set_ortho_zoom(&mut c, 1.0e6);
        assert_eq!(zoom_of(&c), MAX_ZOOM);
    }

    #[test]
    fn zooming_at_the_cursor_keeps_that_point_fixed() {
        let mut c = top(2.0);
        let (cx, cy) = (120.0, -40.0);
        let world = |c: &Camera| {
            let z = zoom_of(c);
            DVec3::new(c.position.x + cx / z, c.position.y - cy / z, 0.0)
        };
        let before = world(&c);
        zoom_ortho_at(&mut c, cx, cy, 1.7);
        assert!((world(&c) - before).length() < 1e-9);
    }
}

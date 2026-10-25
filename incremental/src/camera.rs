use crate::config::game::{MAX_ZOOM, MIN_ZOOM};
use crate::hex::Point;

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub center: Point,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            center: Point::default(),
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub fn world_to_screen(&self, world: Point, viewport: Point) -> Point {
        (world - self.center) * self.zoom + viewport * 0.5
    }

    pub fn screen_to_world(&self, screen: Point, viewport: Point) -> Point {
        (screen - viewport * 0.5) * (1.0 / self.zoom) + self.center
    }

    pub fn pan_by_screen(&mut self, delta: Point) {
        self.center = self.center - delta * (1.0 / self.zoom);
    }

    pub fn zoom_at(&mut self, screen: Point, viewport: Point, factor: f64) {
        let anchor = self.screen_to_world(screen, viewport);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let moved = self.screen_to_world(screen, viewport);
        self.center = self.center + (anchor - moved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEWPORT: Point = Point::new(800.0, 600.0);

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    #[test]
    fn transforms_round_trip() {
        let cam = Camera {
            center: Point::new(13.0, -7.0),
            zoom: 2.5,
        };
        let p = Point::new(100.0, 42.0);
        assert!(close(
            cam.screen_to_world(cam.world_to_screen(p, VIEWPORT), VIEWPORT),
            p
        ));
    }

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut cam = Camera::default();
        let anchor = Point::new(123.0, 456.0);
        let world = cam.screen_to_world(anchor, VIEWPORT);
        cam.zoom_at(anchor, VIEWPORT, 1.7);
        assert!(close(cam.screen_to_world(anchor, VIEWPORT), world));
    }

    #[test]
    fn zoom_is_clamped() {
        let mut cam = Camera::default();
        cam.zoom_at(Point::default(), VIEWPORT, 1e6);
        assert_eq!(cam.zoom, MAX_ZOOM);
        cam.zoom_at(Point::default(), VIEWPORT, 1e-12);
        assert_eq!(cam.zoom, MIN_ZOOM);
    }

    #[test]
    fn pan_moves_content_with_cursor() {
        let mut cam = Camera {
            center: Point::default(),
            zoom: 2.0,
        };
        let world = Point::new(10.0, 10.0);
        let before = cam.world_to_screen(world, VIEWPORT);
        cam.pan_by_screen(Point::new(30.0, -20.0));
        assert!(close(
            cam.world_to_screen(world, VIEWPORT),
            before + Point::new(30.0, -20.0)
        ));
    }
}

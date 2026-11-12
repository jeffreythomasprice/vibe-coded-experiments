use std::collections::HashSet;

use editor_core::Camera;
use editor_geom::camera::{fly, look};
use glam::DVec3;
use winit::keyboard::KeyCode;

const SPEED: f64 = 320.0;
const FAST: f64 = 3.0;
const SENSITIVITY: f64 = 0.6;

#[derive(Default)]
pub struct Input {
    keys: HashSet<KeyCode>,
    pub look: (f64, f64),
}

impl Input {
    pub fn key(&mut self, code: KeyCode, down: bool) {
        if down {
            self.keys.insert(code);
        } else {
            self.keys.remove(&code);
        }
    }

    pub fn clear(&mut self) {
        self.keys.clear();
        self.look = (0.0, 0.0);
    }

    fn any(&self, codes: &[KeyCode]) -> bool {
        codes.iter().any(|c| self.keys.contains(c))
    }

    /// `(forward, right, up)` each in -1..=1.
    fn axes(&self) -> DVec3 {
        let axis = |plus: &[KeyCode], minus: &[KeyCode]| {
            f64::from(i32::from(self.any(plus)) - i32::from(self.any(minus)))
        };
        DVec3::new(
            axis(
                &[KeyCode::KeyW, KeyCode::ArrowUp],
                &[KeyCode::KeyS, KeyCode::ArrowDown],
            ),
            axis(
                &[KeyCode::KeyD, KeyCode::ArrowRight],
                &[KeyCode::KeyA, KeyCode::ArrowLeft],
            ),
            axis(
                &[KeyCode::Space, KeyCode::KeyE],
                &[KeyCode::ControlLeft, KeyCode::KeyQ],
            ),
        )
    }

    /// Applies the accumulated mouse movement and held keys to the camera for a frame of `dt` seconds.
    pub fn apply(&mut self, camera: &mut Camera, dt: f64) {
        let (dx, dy) = std::mem::take(&mut self.look);
        look(camera, dx * SENSITIVITY, dy * SENSITIVITY);
        let axes = self.axes();
        if axes != DVec3::ZERO {
            let speed = if self.any(&[KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
                SPEED * FAST
            } else {
                SPEED
            };
            fly(camera, axes.normalize(), speed * dt);
        }
    }
}

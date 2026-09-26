use std::collections::HashSet;

use glam::Vec2;
use winit::keyboard::KeyCode;

#[derive(Default)]
pub struct InputState {
    keys_down: HashSet<KeyCode>,
}

impl InputState {
    pub fn set_key(&mut self, code: KeyCode, pressed: bool) {
        if pressed {
            self.keys_down.insert(code);
        } else {
            self.keys_down.remove(&code);
        }
    }

    pub fn is_down(&self, code: KeyCode) -> bool {
        self.keys_down.contains(&code)
    }

    /// Normalized WASD/arrow-key movement vector in world space (+x right, +y up).
    pub fn movement(&self) -> Vec2 {
        let mut v = Vec2::ZERO;
        if self.is_down(KeyCode::KeyW) || self.is_down(KeyCode::ArrowUp) {
            v.y += 1.0;
        }
        if self.is_down(KeyCode::KeyS) || self.is_down(KeyCode::ArrowDown) {
            v.y -= 1.0;
        }
        if self.is_down(KeyCode::KeyD) || self.is_down(KeyCode::ArrowRight) {
            v.x += 1.0;
        }
        if self.is_down(KeyCode::KeyA) || self.is_down(KeyCode::ArrowLeft) {
            v.x -= 1.0;
        }
        v.normalize_or_zero()
    }
}

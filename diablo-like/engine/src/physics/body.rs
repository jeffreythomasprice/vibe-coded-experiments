use glam::Vec2;

/// A circle actor. Capsule shapes and a broadphase are deliberately
/// deferred until there are enough actors for either to matter (see the
/// design plan's Physics details note).
#[derive(Copy, Clone, Debug)]
pub struct Actor {
    pub pos: Vec2,
    pub vel: Vec2,
    pub radius: f32,
}

impl Actor {
    pub fn new(pos: Vec2, radius: f32) -> Self {
        Self {
            pos,
            vel: Vec2::ZERO,
            radius,
        }
    }
}

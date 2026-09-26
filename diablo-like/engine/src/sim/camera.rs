use glam::camera::rh::proj::directx::orthographic;
use glam::{Mat4, Vec2};

/// Top-down orthographic camera in world-space XY (+y up). See the "Camera"
/// row of the design plan: isometric later is meant to be a projection swap,
/// not a rewrite of anything that reads world-space positions.
pub struct Camera {
    pub position: Vec2,
    pub half_extent_y: f32,
    aspect: f32,
}

impl Camera {
    pub fn new(half_extent_y: f32) -> Self {
        Self {
            position: Vec2::ZERO,
            half_extent_y,
            aspect: 1.0,
        }
    }

    pub fn set_viewport(&mut self, width: f32, height: f32) {
        if height > 0.0 {
            self.aspect = width / height;
        }
    }

    pub fn view_proj(&self) -> Mat4 {
        let half_x = self.half_extent_y * self.aspect;
        let half_y = self.half_extent_y;
        orthographic(
            self.position.x - half_x,
            self.position.x + half_x,
            self.position.y - half_y,
            self.position.y + half_y,
            -1.0,
            1.0,
        )
    }
}

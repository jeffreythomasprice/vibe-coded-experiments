use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CameraUniforms {
    pub origin: [f32; 3],
    _pad0: f32,
    pub forward: [f32; 3],
    _pad1: f32,
    pub right: [f32; 3],
    _pad2: f32,
    pub up: [f32; 3],
    pub fov: f32,
    pub aspect: f32,
    _pad3: [f32; 3],
}

pub struct Camera {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
}

const PITCH_LIMIT: f32 = 89.0 * std::f32::consts::PI / 180.0;

impl Camera {
    pub fn new(position: [f32; 3], yaw: f32, pitch: f32, fov: f32) -> Self {
        Self {
            position,
            yaw,
            pitch: pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT),
            fov,
        }
    }

    pub fn forward(&self) -> [f32; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [-sy * cp, sp, -cy * cp]
    }

    pub fn right(&self) -> [f32; 3] {
        let fwd = self.forward();
        normalize(cross(fwd, [0.0, 1.0, 0.0]))
    }

    pub fn up(&self) -> [f32; 3] {
        let r = self.right();
        let f = self.forward();
        cross(r, f)
    }

    pub fn to_uniforms(&self, aspect: f32) -> CameraUniforms {
        CameraUniforms {
            origin: self.position,
            _pad0: 0.0,
            forward: self.forward(),
            _pad1: 0.0,
            right: self.right(),
            _pad2: 0.0,
            up: self.up(),
            fov: self.fov,
            aspect,
            _pad3: [0.0; 3],
        }
    }

    pub fn move_forward(&mut self, distance: f32) {
        let (sy, cy) = self.yaw.sin_cos();
        let horizontal_forward = normalize([-sy, 0.0, -cy]);
        self.position[0] += horizontal_forward[0] * distance;
        self.position[1] += horizontal_forward[1] * distance;
        self.position[2] += horizontal_forward[2] * distance;
    }

    pub fn move_right(&mut self, distance: f32) {
        let r = self.right();
        self.position[0] += r[0] * distance;
        self.position[1] += r[1] * distance;
        self.position[2] += r[2] * distance;
    }

    pub fn move_up(&mut self, distance: f32) {
        self.position[1] += distance;
    }

    pub fn rotate(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }
}

impl Default for Camera {
    fn default() -> Self {
        let position = [12.0_f32, 10.0, 20.0];
        let target = [0.0_f32, 0.0, 0.0];
        let dx = target[0] - position[0];
        let dy = target[1] - position[1];
        let dz = target[2] - position[2];
        let horizontal_dist = (dx * dx + dz * dz).sqrt();
        let yaw = f32::atan2(-dx, -dz);
        let pitch = f32::atan2(dy, horizontal_dist);
        Self::new(position, yaw, pitch, 60.0_f32.to_radians())
    }
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / len, v[1] / len, v[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        (a[0] - b[0]).abs() < eps && (a[1] - b[1]).abs() < eps && (a[2] - b[2]).abs() < eps
    }

    #[test]
    fn forward_at_zero_yaw_pitch() {
        let cam = Camera::new([0.0; 3], 0.0, 0.0, 1.0);
        assert!(approx_eq(cam.forward(), [0.0, 0.0, -1.0], 1e-6));
    }

    #[test]
    fn right_is_perpendicular_to_forward() {
        let cam = Camera::default();
        let f = cam.forward();
        let r = cam.right();
        let dot = f[0] * r[0] + f[1] * r[1] + f[2] * r[2];
        assert!(dot.abs() < 1e-5);
    }

    #[test]
    fn pitch_clamp() {
        let cam = Camera::new([0.0; 3], 0.0, 100.0_f32.to_radians(), 1.0);
        assert!(cam.pitch <= PITCH_LIMIT);
    }

    #[test]
    fn default_looks_toward_origin() {
        let cam = Camera::default();
        let f = cam.forward();
        let expected_dir = normalize([-12.0, -10.0, -20.0]);
        assert!(approx_eq(f, expected_dir, 1e-4));
    }

    #[test]
    fn move_forward_stays_on_xz_plane() {
        let mut cam = Camera::new([0.0, 5.0, 0.0], 0.0, 0.5, 1.0);
        let y_before = cam.position[1];
        cam.move_forward(1.0);
        assert!((cam.position[1] - y_before).abs() < 1e-6);
    }

    #[test]
    fn move_up_only_changes_y() {
        let mut cam = Camera::new([1.0, 2.0, 3.0], 0.5, 0.3, 1.0);
        let x = cam.position[0];
        let z = cam.position[2];
        cam.move_up(1.0);
        assert!((cam.position[0] - x).abs() < 1e-6);
        assert!((cam.position[2] - z).abs() < 1e-6);
        assert!((cam.position[1] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_clamps_pitch() {
        let mut cam = Camera::new([0.0; 3], 0.0, 0.0, 1.0);
        cam.rotate(0.0, 100.0);
        assert!(cam.pitch <= PITCH_LIMIT);
        cam.rotate(0.0, -200.0);
        assert!(cam.pitch >= -PITCH_LIMIT);
    }
}

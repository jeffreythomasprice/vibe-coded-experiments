use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CameraUniforms {
    pub origin: Vec3,
    _pad0: f32,
    pub forward: Vec3,
    _pad1: f32,
    pub right: Vec3,
    _pad2: f32,
    pub up: Vec3,
    pub fov: f32,
    pub aspect: f32,
    _pad3: Vec3,
}

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
}

const PITCH_LIMIT: f32 = 89.0 * std::f32::consts::PI / 180.0;

impl Camera {
    pub fn new(position: Vec3, yaw: f32, pitch: f32, fov: f32) -> Self {
        Self {
            position,
            yaw,
            pitch: pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT),
            fov,
        }
    }

    pub fn forward(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        Vec3::new(-sy * cp, sp, -cy * cp)
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward())
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
            _pad3: Vec3::ZERO,
        }
    }

    pub fn move_forward(&mut self, distance: f32) {
        let (sy, cy) = self.yaw.sin_cos();
        let horizontal_forward = Vec3::new(-sy, 0.0, -cy).normalize();
        self.position += horizontal_forward * distance;
    }

    pub fn move_right(&mut self, distance: f32) {
        self.position += self.right() * distance;
    }

    pub fn move_up(&mut self, distance: f32) {
        self.position.y += distance;
    }

    pub fn rotate(&mut self, delta_yaw: f32, delta_pitch: f32) {
        self.yaw += delta_yaw;
        self.pitch = (self.pitch + delta_pitch).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let data: [f32; 5] = [
            self.position.x,
            self.position.y,
            self.position.z,
            self.yaw,
            self.pitch,
        ];
        std::fs::write(path, bytemuck::cast_slice::<f32, u8>(&data))
            .with_context(|| format!("failed to write camera to {}", path.display()))
    }

    pub fn load_from_file(path: &Path) -> Result<Camera> {
        let data = std::fs::read(path)
            .with_context(|| format!("failed to read camera from {}", path.display()))?;
        if data.len() != 20 {
            anyhow::bail!(
                "camera file {} has wrong size: expected 20 bytes, got {}",
                path.display(),
                data.len()
            );
        }
        let floats: &[f32; 5] = bytemuck::from_bytes(&data);
        Ok(Camera::new(
            Vec3::new(floats[0], floats[1], floats[2]),
            floats[3],
            floats[4],
            60.0_f32.to_radians(),
        ))
    }
}

pub fn camera_file_path(dir: &Path) -> PathBuf {
    dir.join("camera")
}

impl Default for Camera {
    fn default() -> Self {
        let position = Vec3::new(12.0, 10.0, 20.0);
        let target = Vec3::ZERO;
        let delta = target - position;
        let horizontal_dist = Vec3::new(delta.x, 0.0, delta.z).length();
        let yaw = f32::atan2(-delta.x, -delta.z);
        let pitch = f32::atan2(delta.y, horizontal_dist);
        Self::new(position, yaw, pitch, 60.0_f32.to_radians())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: Vec3, b: Vec3, eps: f32) -> bool {
        (a - b).abs().max_element() < eps
    }

    #[test]
    fn forward_at_zero_yaw_pitch() {
        let cam = Camera::new(Vec3::ZERO, 0.0, 0.0, 1.0);
        assert!(approx_eq(cam.forward(), Vec3::new(0.0, 0.0, -1.0), 1e-6));
    }

    #[test]
    fn right_is_perpendicular_to_forward() {
        let cam = Camera::default();
        let dot = cam.forward().dot(cam.right());
        assert!(dot.abs() < 1e-5);
    }

    #[test]
    fn pitch_clamp() {
        let cam = Camera::new(Vec3::ZERO, 0.0, 100.0_f32.to_radians(), 1.0);
        assert!(cam.pitch <= PITCH_LIMIT);
    }

    #[test]
    fn default_looks_toward_origin() {
        let cam = Camera::default();
        let f = cam.forward();
        let expected_dir = Vec3::new(-12.0, -10.0, -20.0).normalize();
        assert!(approx_eq(f, expected_dir, 1e-4));
    }

    #[test]
    fn move_forward_stays_on_xz_plane() {
        let mut cam = Camera::new(Vec3::new(0.0, 5.0, 0.0), 0.0, 0.5, 1.0);
        let y_before = cam.position.y;
        cam.move_forward(1.0);
        assert!((cam.position.y - y_before).abs() < 1e-6);
    }

    #[test]
    fn move_up_only_changes_y() {
        let mut cam = Camera::new(Vec3::new(1.0, 2.0, 3.0), 0.5, 0.3, 1.0);
        let x = cam.position.x;
        let z = cam.position.z;
        cam.move_up(1.0);
        assert!((cam.position.x - x).abs() < 1e-6);
        assert!((cam.position.z - z).abs() < 1e-6);
        assert!((cam.position.y - 3.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_clamps_pitch() {
        let mut cam = Camera::new(Vec3::ZERO, 0.0, 0.0, 1.0);
        cam.rotate(0.0, 100.0);
        assert!(cam.pitch <= PITCH_LIMIT);
        cam.rotate(0.0, -200.0);
        assert!(cam.pitch >= -PITCH_LIMIT);
    }

    #[test]
    fn save_load_roundtrip() -> anyhow::Result<()> {
        let dir = std::env::temp_dir().join("camera_test_save_load_roundtrip");
        std::fs::create_dir_all(&dir)?;
        let path = camera_file_path(&dir);

        let cam = Camera::new(Vec3::new(1.5, 2.5, 3.5), 0.7, -0.3, 60.0_f32.to_radians());
        cam.save_to_file(&path)?;

        let loaded = Camera::load_from_file(&path)?;
        assert!((cam.position - loaded.position).length() < 1e-6);
        assert!((cam.yaw - loaded.yaw).abs() < 1e-6);
        assert!((cam.pitch - loaded.pitch).abs() < 1e-6);

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn load_wrong_size_returns_error() -> anyhow::Result<()> {
        let dir = std::env::temp_dir().join("camera_test_load_wrong_size");
        std::fs::create_dir_all(&dir)?;
        let path = camera_file_path(&dir);

        std::fs::write(&path, &[0u8; 10])?;
        let result = Camera::load_from_file(&path);
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn camera_file_path_format() {
        let dir = Path::new("/world/data");
        assert_eq!(camera_file_path(dir), PathBuf::from("/world/data/camera"));
    }
}

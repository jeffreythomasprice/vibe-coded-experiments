use bytemuck::{Pod, Zeroable};
use glam::Vec3;

pub const MESH_VOXEL_BASE: u8 = 128;
pub const MESH_TORCH: u8 = MESH_VOXEL_BASE;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuTriangle {
    pub v0: [f32; 3],
    pub color_r: f32,
    pub v1: [f32; 3],
    pub color_g: f32,
    pub v2: [f32; 3],
    pub color_b: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuMeshInfo {
    pub tri_offset: u32,
    pub tri_count: u32,
    pub _pad0: u32,
    pub _pad1: u32,
}

pub struct MeshCatalog {
    pub triangles: Vec<GpuTriangle>,
    pub mesh_infos: Vec<GpuMeshInfo>,
}

fn tri(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3], color: [f32; 3]) -> GpuTriangle {
    GpuTriangle {
        v0,
        color_r: color[0],
        v1,
        color_g: color[1],
        v2,
        color_b: color[2],
    }
}

fn build_torch() -> Vec<GpuTriangle> {
    let mut tris = Vec::with_capacity(25);

    // Stick dimensions
    let x0 = 0.375_f32;
    let x1 = 0.625_f32;
    let z0 = 0.375_f32;
    let z1 = 0.625_f32;
    let y_bot = 0.0_f32;
    let y_top = 0.6_f32;

    let light_brown = [0.55, 0.35, 0.15];
    let dark_brown = [0.40, 0.25, 0.10];
    let med_brown = [0.48, 0.30, 0.12];

    // Front face (z = z1) — light brown
    tris.push(tri([x0, y_bot, z1], [x1, y_bot, z1], [x1, y_top, z1], light_brown));
    tris.push(tri([x0, y_bot, z1], [x1, y_top, z1], [x0, y_top, z1], light_brown));

    // Back face (z = z0) — light brown
    tris.push(tri([x1, y_bot, z0], [x0, y_bot, z0], [x0, y_top, z0], light_brown));
    tris.push(tri([x1, y_bot, z0], [x0, y_top, z0], [x1, y_top, z0], light_brown));

    // Right face (x = x1) — dark brown
    tris.push(tri([x1, y_bot, z1], [x1, y_bot, z0], [x1, y_top, z0], dark_brown));
    tris.push(tri([x1, y_bot, z1], [x1, y_top, z0], [x1, y_top, z1], dark_brown));

    // Left face (x = x0) — dark brown
    tris.push(tri([x0, y_bot, z0], [x0, y_bot, z1], [x0, y_top, z1], dark_brown));
    tris.push(tri([x0, y_bot, z0], [x0, y_top, z1], [x0, y_top, z0], dark_brown));

    // Top cap (y = y_top) — medium brown
    tris.push(tri([x0, y_top, z0], [x0, y_top, z1], [x1, y_top, z1], med_brown));
    tris.push(tri([x0, y_top, z0], [x1, y_top, z1], [x1, y_top, z0], med_brown));

    // Bottom cap (y = y_bot) — medium brown
    tris.push(tri([x0, y_bot, z1], [x0, y_bot, z0], [x1, y_bot, z0], med_brown));
    tris.push(tri([x0, y_bot, z1], [x1, y_bot, z0], [x1, y_bot, z1], med_brown));

    // Flame base — slightly wider prism from y=0.6 to y=0.8
    let fx0 = 0.35_f32;
    let fx1 = 0.65_f32;
    let fz0 = 0.35_f32;
    let fz1 = 0.65_f32;
    let fy0 = 0.6_f32;
    let fy1 = 0.8_f32;
    let yellow = [1.0, 0.85, 0.2];

    // Flame base — 4 side faces x 2 tris = 8 triangles
    // Front
    tris.push(tri([fx0, fy0, fz1], [fx1, fy0, fz1], [fx1, fy1, fz1], yellow));
    tris.push(tri([fx0, fy0, fz1], [fx1, fy1, fz1], [fx0, fy1, fz1], yellow));
    // Back
    tris.push(tri([fx1, fy0, fz0], [fx0, fy0, fz0], [fx0, fy1, fz0], yellow));
    tris.push(tri([fx1, fy0, fz0], [fx0, fy1, fz0], [fx1, fy1, fz0], yellow));
    // Right
    tris.push(tri([fx1, fy0, fz1], [fx1, fy0, fz0], [fx1, fy1, fz0], yellow));
    tris.push(tri([fx1, fy0, fz1], [fx1, fy1, fz0], [fx1, fy1, fz1], yellow));
    // Left
    tris.push(tri([fx0, fy0, fz0], [fx0, fy0, fz1], [fx0, fy1, fz1], yellow));
    tris.push(tri([fx0, fy0, fz0], [fx0, fy1, fz1], [fx0, fy1, fz0], yellow));

    // Flame tips — 4 upward-pointing triangles, one per side
    let orange_red = [1.0, 0.45, 0.05];
    let tip_y = 0.95_f32;
    let tip_inset = 0.05_f32;
    let cx = 0.5_f32;
    let cz = 0.5_f32;

    // Front tip
    tris.push(tri(
        [fx0 + tip_inset, fy1, fz1],
        [fx1 - tip_inset, fy1, fz1],
        [cx, tip_y, cz],
        orange_red,
    ));
    // Back tip
    tris.push(tri(
        [fx1 - tip_inset, fy1, fz0],
        [fx0 + tip_inset, fy1, fz0],
        [cx, tip_y, cz],
        orange_red,
    ));
    // Right tip
    tris.push(tri(
        [fx1, fy1, fz1 - tip_inset],
        [fx1, fy1, fz0 + tip_inset],
        [cx, tip_y, cz],
        orange_red,
    ));
    // Left tip
    tris.push(tri(
        [fx0, fy1, fz0 + tip_inset],
        [fx0, fy1, fz1 - tip_inset],
        [cx, tip_y, cz],
        orange_red,
    ));

    // Flame peak — 1 small triangle, offset slightly off-center
    let red = [0.9, 0.2, 0.0];
    tris.push(tri(
        [0.45, 0.9, 0.45],
        [0.55, 0.9, 0.50],
        [0.48, 1.0, 0.52],
        red,
    ));

    tris
}

/// Moller-Trumbore ray-triangle intersection.
/// Returns Some((t, normal)) if the ray hits the triangle within (epsilon, max_t).
fn ray_triangle_intersect(
    origin: Vec3,
    dir: Vec3,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
    max_t: f32,
) -> Option<(f32, Vec3)> {
    let epsilon = 1e-6;
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = dir.cross(edge2);
    let a = edge1.dot(h);

    if a.abs() < epsilon {
        return None;
    }

    let f = 1.0 / a;
    let s = origin - v0;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = f * dir.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * edge2.dot(q);
    if t < epsilon || t > max_t {
        return None;
    }

    // Geometric normal, oriented to face the ray
    let mut normal = edge1.cross(edge2).normalize();
    if normal.dot(dir) > 0.0 {
        normal = -normal;
    }

    Some((t, normal))
}

impl MeshCatalog {
    pub fn build() -> Self {
        let torch_tris = build_torch();

        let mesh_infos = vec![GpuMeshInfo {
            tri_offset: 0,
            tri_count: torch_tris.len() as u32,
            _pad0: 0,
            _pad1: 0,
        }];

        Self {
            triangles: torch_tris,
            mesh_infos,
        }
    }

    /// Cast a ray against a mesh type in voxel-local [0,1]³ space.
    /// Returns (t, normal) of the closest triangle hit, or None.
    pub fn raycast(&self, mesh_type: u8, local_origin: Vec3, direction: Vec3) -> Option<(f32, Vec3)> {
        let info = self.mesh_infos.get(mesh_type as usize)?;
        let offset = info.tri_offset as usize;
        let count = info.tri_count as usize;

        let mut best: Option<(f32, Vec3)> = None;

        for i in offset..offset + count {
            if let Some(tri) = self.triangles.get(i) {
                let v0 = Vec3::from(tri.v0);
                let v1 = Vec3::from(tri.v1);
                let v2 = Vec3::from(tri.v2);
                let max_t = best.map_or(f32::MAX, |(t, _)| t);
                if let Some((t, normal)) = ray_triangle_intersect(local_origin, direction, v0, v1, v2, max_t) {
                    best = Some((t, normal));
                }
            }
        }

        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn mesh_catalog_builds_without_panic() {
        let catalog = MeshCatalog::build();
        assert_eq!(catalog.triangles.len(), 25);
        assert!(catalog.mesh_infos.len() >= 1);
    }

    #[test]
    fn gpu_triangle_size() {
        assert_eq!(size_of::<GpuTriangle>(), 48);
    }

    #[test]
    fn gpu_mesh_info_size() {
        assert_eq!(size_of::<GpuMeshInfo>(), 16);
    }

    #[test]
    fn torch_triangles_within_unit_cube() {
        let catalog = MeshCatalog::build();
        for (i, t) in catalog.triangles.iter().enumerate() {
            for (vi, v) in [t.v0, t.v1, t.v2].iter().enumerate() {
                for (ci, &c) in v.iter().enumerate() {
                    assert!(
                        (0.0..=1.0).contains(&c),
                        "triangle {i} vertex {vi} component {ci} = {c} is outside [0.0, 1.0]"
                    );
                }
            }
        }
    }

    #[test]
    fn raycast_hits_mesh_triangle() {
        let catalog = MeshCatalog::build();
        // Cast a ray straight down through the center of the torch (should hit the flame/stick)
        let origin = Vec3::new(0.5, 1.5, 0.5);
        let direction = Vec3::new(0.0, -1.0, 0.0);
        let result = catalog.raycast(0, origin, direction);
        assert!(result.is_some(), "ray through center of torch should hit");
        let (t, normal) = match result {
            Some(v) => v,
            None => panic!("expected hit"),
        };
        assert!(t > 0.0, "hit distance should be positive");
        // Normal should point upward (facing the ray)
        assert!(normal.y > 0.0, "normal should face up toward the ray origin");
    }

    #[test]
    fn raycast_misses_mesh_triangle() {
        let catalog = MeshCatalog::build();
        // Cast a ray through the corner of the voxel where there's no torch geometry
        let origin = Vec3::new(0.05, 0.5, 0.05);
        let direction = Vec3::new(0.0, 1.0, 0.0);
        let result = catalog.raycast(0, origin, direction);
        assert!(result.is_none(), "ray through corner of voxel should miss torch");
    }
}

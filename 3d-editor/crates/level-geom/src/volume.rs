use glam::DVec3;
use level_core::Mesh;

use crate::winding::newell;

/// Signed volume contribution of one face, as a fan from its first vertex to the origin.
pub fn face_volume(poly: &[DVec3]) -> f64 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut v = 0.0;
    for i in 1..poly.len() - 1 {
        v += poly[0].dot(poly[i].cross(poly[i + 1]));
    }
    v / 6.0
}

pub fn mesh_volume(mesh: &Mesh) -> f64 {
    let mut total = 0.0;
    for face in &mesh.faces {
        let poly: Vec<DVec3> = face
            .verts
            .iter()
            .filter_map(|&v| mesh.position(v))
            .collect();
        total += face_volume(&poly);
    }
    total
}

pub fn face_area(poly: &[DVec3]) -> f64 {
    newell(poly).length()
}

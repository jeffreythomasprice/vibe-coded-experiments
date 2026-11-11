use editor_core::{Brush, Mesh};
use glam::DAffine3;

use crate::plane::plane_from_points;
use crate::polytope::{BrushError, normalize};
use crate::tolerance::snap_vec;
use crate::uv;

/// Applies `m` to a brush's plane points. A mirroring `m` flips the winding of each plane, so two points swap to keep the normal outward.
/// With `texture_lock` each face's texture follows the geometry.
pub fn transform_brush(
    brush: &Brush,
    m: &DAffine3,
    texture_lock: bool,
) -> Result<Brush, BrushError> {
    let mirrored = m.matrix3.determinant() < 0.0;
    let mut out = brush.clone();
    for face in &mut out.faces {
        let old = face.points;
        face.points = face.points.map(|p| snap_vec(m.transform_point3(p)));
        let pairs = [
            (old[0], face.points[0]),
            (old[1], face.points[1]),
            (old[2], face.points[2]),
        ];
        if mirrored {
            face.points.swap(1, 2);
        }
        if texture_lock
            && let (Ok(before), Ok(after)) =
                (plane_from_points(old), plane_from_points(face.points))
            && let Some(attrs) = uv::lock_corners(&face.attrs, before.normal, &after, &pairs)
        {
            face.attrs = attrs;
        }
    }
    normalize(&out)
}

pub fn transform_mesh(mesh: &Mesh, m: &DAffine3, texture_lock: bool) -> Mesh {
    let mirrored = m.matrix3.determinant() < 0.0;
    let mut out = mesh.clone();
    for v in &mut out.vertices {
        v.pos = snap_vec(m.transform_point3(v.pos));
    }
    if mirrored {
        for f in &mut out.faces {
            f.verts.reverse();
        }
    }
    if texture_lock {
        uv::lock_mesh_faces(mesh, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{brush_geometry, shapes};
    use glam::{DQuat, DVec3};

    #[test]
    fn mirror_keeps_volume_and_validity() {
        let b = shapes::cuboid(DVec3::new(1.0, 2.0, 3.0), DVec3::new(5.0, 6.0, 9.0));
        let m = DAffine3::from_scale(DVec3::new(-1.0, 1.0, 1.0));
        let t = transform_brush(&b, &m, false).unwrap();
        let (a, c) = (brush_geometry(&b).unwrap(), brush_geometry(&t).unwrap());
        assert!((a.volume() - c.volume()).abs() < 1e-6);
        assert!(c.vertices.iter().all(|v| v.x <= -1.0 + 1e-9));
    }

    #[test]
    fn rotate_quarter_turn_is_exact_on_the_grid() {
        let b = shapes::cuboid(DVec3::new(0.0, 0.0, 0.0), DVec3::new(8.0, 4.0, 2.0));
        let m = DAffine3::from_quat(DQuat::from_axis_angle(DVec3::Z, 90f64.to_radians()));
        let t = transform_brush(&b, &m, false).unwrap();
        let g = brush_geometry(&t).unwrap();
        let min = g.vertices.iter().fold(DVec3::MAX, |a, v| a.min(*v));
        let max = g.vertices.iter().fold(DVec3::MIN, |a, v| a.max(*v));
        assert!((min - DVec3::new(-4.0, 0.0, 0.0)).length() < 1e-5);
        assert!((max - DVec3::new(0.0, 8.0, 2.0)).length() < 1e-5);
    }

    #[test]
    fn collapsing_scale_is_an_error() {
        let b = shapes::cuboid(DVec3::ZERO, DVec3::splat(8.0));
        let m = DAffine3::from_scale(DVec3::new(1.0, 1.0, 1e-12));
        assert!(transform_brush(&b, &m, false).is_err());
    }

    #[test]
    fn mesh_mirror_reverses_loops() {
        let b = shapes::cuboid(DVec3::ZERO, DVec3::splat(8.0));
        let mesh = crate::brush_to_mesh(&b).unwrap();
        let m = DAffine3::from_scale(DVec3::new(1.0, -1.0, 1.0));
        let t = transform_mesh(&mesh, &m, false);
        assert!(crate::mesh::validate(&t).is_empty());
    }
}

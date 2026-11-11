use editor_core::{FaceId, Mesh};
use glam::DVec3;

use crate::aabb::Aabb;
use crate::mesh::triangulate;
use crate::polytope::BrushGeometry;
use crate::winding::newell;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub t: f64,
    pub normal: DVec3,
    pub face: Option<FaceId>,
}

const PARALLEL_EPS: f64 = 1e-12;

/// First hit of a ray (unit `dir`) on a box. An origin inside reports the face it leaves through.
pub fn ray_aabb(origin: DVec3, dir: DVec3, b: &Aabb) -> Option<RayHit> {
    let mut t_near = f64::NEG_INFINITY;
    let mut t_far = f64::INFINITY;
    let mut near_normal = DVec3::ZERO;
    let mut far_normal = DVec3::ZERO;
    for axis in 0..3 {
        let (o, d, lo, hi) = (origin[axis], dir[axis], b.min[axis], b.max[axis]);
        let mut unit = DVec3::ZERO;
        if d.abs() < PARALLEL_EPS {
            if o < lo || o > hi {
                return None;
            }
            continue;
        }
        let (mut t0, mut t1) = ((lo - o) / d, (hi - o) / d);
        unit[axis] = -d.signum();
        let (mut n0, mut n1) = (unit, -unit);
        if t0 > t1 {
            std::mem::swap(&mut t0, &mut t1);
            std::mem::swap(&mut n0, &mut n1);
        }
        if t0 > t_near {
            t_near = t0;
            near_normal = n0;
        }
        if t1 < t_far {
            t_far = t1;
            far_normal = n1;
        }
        if t_near > t_far {
            return None;
        }
    }
    if t_far < 0.0 {
        return None;
    }
    if t_near >= 0.0 {
        Some(RayHit {
            t: t_near,
            normal: near_normal,
            face: None,
        })
    } else {
        Some(RayHit {
            t: t_far,
            normal: far_normal,
            face: None,
        })
    }
}

/// Convex clipping against the face planes. An origin inside the solid reports the face it leaves through.
pub fn ray_brush(origin: DVec3, dir: DVec3, geo: &BrushGeometry) -> Option<RayHit> {
    let mut t_enter = f64::NEG_INFINITY;
    let mut t_exit = f64::INFINITY;
    let mut enter: Option<(DVec3, FaceId)> = None;
    let mut exit: Option<(DVec3, FaceId)> = None;
    for f in &geo.faces {
        let n = f.plane.normal;
        let denom = n.dot(dir);
        let num = f.plane.dist - n.dot(origin);
        if denom.abs() < PARALLEL_EPS {
            if num < 0.0 {
                return None;
            }
            continue;
        }
        let t = num / denom;
        if denom < 0.0 {
            if t > t_enter {
                t_enter = t;
                enter = Some((n, f.face));
            }
        } else if t < t_exit {
            t_exit = t;
            exit = Some((n, f.face));
        }
        if t_enter > t_exit {
            return None;
        }
    }
    if t_exit < 0.0 {
        return None;
    }
    let (t, hit) = if t_enter >= 0.0 {
        (t_enter, enter?)
    } else {
        (t_exit, exit?)
    };
    Some(RayHit {
        t,
        normal: hit.0,
        face: Some(hit.1),
    })
}

fn ray_triangle(origin: DVec3, dir: DVec3, tri: [DVec3; 3]) -> Option<f64> {
    let e1 = tri[1] - tri[0];
    let e2 = tri[2] - tri[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < PARALLEL_EPS {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - tri[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t >= 0.0).then_some(t)
}

/// Two-sided: the nearest triangle of any face, with that face's outward normal.
pub fn ray_mesh(origin: DVec3, dir: DVec3, mesh: &Mesh) -> Option<RayHit> {
    let mut best: Option<RayHit> = None;
    for face in &mesh.faces {
        let poly: Vec<DVec3> = face
            .verts
            .iter()
            .filter_map(|&v| mesh.position(v))
            .collect();
        if poly.len() != face.verts.len() {
            continue;
        }
        let normal = newell(&poly).try_normalize().unwrap_or(DVec3::Z);
        for [a, b, c] in triangulate(&poly) {
            if let Some(t) = ray_triangle(origin, dir, [poly[a], poly[b], poly[c]])
                && best.is_none_or(|h| t < h.t)
            {
                best = Some(RayHit {
                    t,
                    normal,
                    face: Some(face.id),
                });
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{brush_geometry, brush_to_mesh, shapes};

    fn unit_box() -> crate::BrushGeometry {
        brush_geometry(&shapes::cuboid(DVec3::ZERO, DVec3::splat(8.0))).unwrap()
    }

    #[test]
    fn brush_hit_miss_and_inside() {
        let g = unit_box();
        let hit = ray_brush(DVec3::new(-10.0, 4.0, 4.0), DVec3::X, &g).unwrap();
        assert!((hit.t - 10.0).abs() < 1e-9);
        assert!((hit.normal - DVec3::NEG_X).length() < 1e-9);
        assert!(ray_brush(DVec3::new(-10.0, 20.0, 4.0), DVec3::X, &g).is_none());
        assert!(ray_brush(DVec3::new(20.0, 4.0, 4.0), DVec3::X, &g).is_none());
        let inside = ray_brush(DVec3::splat(4.0), DVec3::X, &g).unwrap();
        assert!((inside.t - 4.0).abs() < 1e-9);
        assert!((inside.normal - DVec3::X).length() < 1e-9);
    }

    #[test]
    fn aabb_and_mesh_agree_with_brush() {
        let g = unit_box();
        let b = Aabb {
            min: DVec3::ZERO,
            max: DVec3::splat(8.0),
        };
        let m = brush_to_mesh(&shapes::cuboid(DVec3::ZERO, DVec3::splat(8.0))).unwrap();
        let (o, d) = (DVec3::new(4.0, 4.0, 30.0), DVec3::NEG_Z);
        let (hb, ha, hm) = (
            ray_brush(o, d, &g).unwrap(),
            ray_aabb(o, d, &b).unwrap(),
            ray_mesh(o, d, &m).unwrap(),
        );
        assert!((hb.t - 22.0).abs() < 1e-9);
        assert!((ha.t - hb.t).abs() < 1e-9 && (hm.t - hb.t).abs() < 1e-9);
        assert!((hm.normal - DVec3::Z).length() < 1e-9);
    }
}

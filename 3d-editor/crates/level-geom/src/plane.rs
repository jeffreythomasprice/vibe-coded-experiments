use glam::DVec3;
use level_core::Plane;

use crate::tolerance::SIDE_EPS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GeomError {
    #[error("the three points do not define a plane")]
    DegeneratePlane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Front,
    Back,
    On,
}

/// Quake convention: the normal is `(c - a) x (b - a)`, so the points run clockwise seen from outside.
pub fn plane_from_points(points: [DVec3; 3]) -> Result<Plane, GeomError> {
    let [a, b, c] = points;
    let n = (c - a).cross(b - a);
    let len = n.length();
    if len.is_nan() || len <= 1e-9 {
        return Err(GeomError::DegeneratePlane);
    }
    let normal = n / len;
    Ok(Plane {
        normal,
        dist: normal.dot(a),
    })
}

pub fn signed_distance(plane: &Plane, p: DVec3) -> f64 {
    plane.normal.dot(p) - plane.dist
}

pub fn classify_point(plane: &Plane, p: DVec3) -> Side {
    classify_distance(signed_distance(plane, p))
}

pub fn classify_distance(d: f64) -> Side {
    classify_distance_eps(d, SIDE_EPS)
}

pub fn classify_distance_eps(d: f64, eps: f64) -> Side {
    if d > eps {
        Side::Front
    } else if d < -eps {
        Side::Back
    } else {
        Side::On
    }
}

pub fn flip(plane: &Plane) -> Plane {
    Plane {
        normal: -plane.normal,
        dist: -plane.dist,
    }
}

pub fn same_plane(a: &Plane, b: &Plane) -> bool {
    (a.normal - b.normal).length() < 1e-9 && (a.dist - b.dist).abs() < SIDE_EPS
}

pub fn opposite_plane(a: &Plane, b: &Plane) -> bool {
    same_plane(a, &flip(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_face_points_up() {
        let p = plane_from_points([
            DVec3::new(0.0, 0.0, 64.0),
            DVec3::new(0.0, 64.0, 64.0),
            DVec3::new(64.0, 0.0, 64.0),
        ])
        .unwrap();
        assert_eq!(p.normal, DVec3::Z);
        assert_eq!(p.dist, 64.0);
    }

    #[test]
    fn collinear_is_degenerate() {
        let r = plane_from_points([DVec3::ZERO, DVec3::X, DVec3::X * 2.0]);
        assert_eq!(r, Err(GeomError::DegeneratePlane));
    }
}

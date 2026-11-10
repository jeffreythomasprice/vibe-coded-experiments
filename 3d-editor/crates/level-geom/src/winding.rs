use glam::DVec3;
use level_core::Plane;

use crate::plane::{Side, classify_distance, signed_distance};
use crate::tolerance::WORLD_BOUND;

/// A large square on `plane`, counter-clockwise seen from the side the normal points to.
pub fn base_winding(plane: &Plane) -> Vec<DVec3> {
    let n = plane.normal;
    let abs = n.abs();
    let helper = if abs.x <= abs.y && abs.x <= abs.z {
        DVec3::X
    } else if abs.y <= abs.z {
        DVec3::Y
    } else {
        DVec3::Z
    };
    let u = helper.cross(n).normalize();
    let v = n.cross(u);
    let c = n * plane.dist;
    let b = WORLD_BOUND;
    vec![
        c - u * b - v * b,
        c + u * b - v * b,
        c + u * b + v * b,
        c - u * b + v * b,
    ]
}

/// Keeps the part of a convex polygon on the back of `plane` (points on the plane are kept).
pub fn clip_back(poly: &[DVec3], plane: &Plane) -> Vec<DVec3> {
    let dists: Vec<f64> = poly.iter().map(|&p| signed_distance(plane, p)).collect();
    let sides: Vec<Side> = dists.iter().map(|&d| classify_distance(d)).collect();
    if !sides.contains(&Side::Front) {
        return poly.to_vec();
    }
    if !sides.contains(&Side::Back) {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        if sides[i] != Side::Front {
            out.push(poly[i]);
        }
        let crossing = matches!(
            (sides[i], sides[j]),
            (Side::Front, Side::Back) | (Side::Back, Side::Front)
        );
        if crossing {
            let t = dists[i] / (dists[i] - dists[j]);
            out.push(poly[i] + (poly[j] - poly[i]) * t);
        }
    }
    out
}

pub fn newell(poly: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        n += a.cross(b);
    }
    n * 0.5
}

pub fn area(poly: &[DVec3]) -> f64 {
    newell(poly).length()
}

/// A polygon whose area is no more than a strip of width `eps` along its longest extent.
pub fn is_sliver(poly: &[DVec3], eps: f64) -> bool {
    let (min, max) = poly.iter().fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| {
        (lo.min(*p), hi.max(*p))
    });
    area(poly) <= 2.0 * eps * (max - min).length()
}

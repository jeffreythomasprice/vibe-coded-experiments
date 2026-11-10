use glam::{DVec2, DVec3};

use crate::winding::newell;

/// Ear clipping of a simple polygon; returns index triples into `poly` with the original winding.
pub fn triangulate(poly: &[DVec3]) -> Vec<[usize; 3]> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    if n == 3 {
        return vec![[0, 1, 2]];
    }
    let normal = newell(poly);
    let abs = normal.abs();
    let project = |p: DVec3| -> DVec2 {
        if abs.z >= abs.x && abs.z >= abs.y {
            DVec2::new(p.x, p.y)
        } else if abs.x >= abs.y {
            DVec2::new(p.y, p.z)
        } else {
            DVec2::new(p.z, p.x)
        }
    };
    let pts: Vec<DVec2> = poly.iter().map(|&p| project(p)).collect();
    let mut area2 = 0.0;
    for i in 0..n {
        area2 += pts[i].perp_dot(pts[(i + 1) % n]);
    }
    let sign = if area2 < 0.0 { -1.0 } else { 1.0 };

    let cross = |a: usize, b: usize, c: usize| (pts[b] - pts[a]).perp_dot(pts[c] - pts[a]) * sign;
    let in_triangle = |p: usize, a: usize, b: usize, c: usize| {
        let eps = -1e-12;
        if pts[p] == pts[a] || pts[p] == pts[b] || pts[p] == pts[c] {
            return false;
        }
        cross(a, b, p) >= eps && cross(b, c, p) >= eps && cross(c, a, p) >= eps
    };

    let mut idx: Vec<usize> = (0..n).collect();
    let mut out = Vec::with_capacity(n - 2);
    while idx.len() > 3 {
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (a, b, c) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            if cross(a, b, c) < -1e-12 {
                continue;
            }
            let blocked = idx
                .iter()
                .any(|&p| p != a && p != b && p != c && in_triangle(p, a, b, c));
            if blocked {
                continue;
            }
            out.push([a, b, c]);
            idx.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            let k = 0;
            let (a, b, c) = (idx[m - 1], idx[k], idx[1]);
            out.push([a, b, c]);
            idx.remove(k);
        }
    }
    out.push([idx[0], idx[1], idx[2]]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::winding::area;

    fn total(poly: &[DVec3]) -> f64 {
        triangulate(poly)
            .iter()
            .map(|t| area(&[poly[t[0]], poly[t[1]], poly[t[2]]]))
            .sum()
    }

    #[test]
    fn concave_l_shape() {
        let v = DVec3::new;
        let poly = [
            v(0.0, 0.0, 0.0),
            v(4.0, 0.0, 0.0),
            v(4.0, 2.0, 0.0),
            v(2.0, 2.0, 0.0),
            v(2.0, 4.0, 0.0),
            v(0.0, 4.0, 0.0),
        ];
        assert_eq!(triangulate(&poly).len(), 4);
        assert!((total(&poly) - 12.0).abs() < 1e-9);
    }

    #[test]
    fn clockwise_vertical() {
        let v = DVec3::new;
        let poly = [
            v(0.0, 0.0, 0.0),
            v(0.0, 0.0, 3.0),
            v(2.0, 0.0, 3.0),
            v(2.0, 0.0, 0.0),
        ];
        assert!((total(&poly) - 6.0).abs() < 1e-9);
    }
}

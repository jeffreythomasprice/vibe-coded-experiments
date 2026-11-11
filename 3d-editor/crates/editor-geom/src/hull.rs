use editor_core::Plane;
use glam::{DVec2, DVec3};

use crate::plane::{flip, signed_distance};
use crate::tolerance::{SIDE_EPS, Welder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HullError {
    #[error("the points do not enclose a volume")]
    Flat,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HullFace {
    pub plane: Plane,
    /// Indices into `Hull::points`, counter-clockwise seen from outside.
    pub corners: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hull {
    /// The input points with near-duplicates welded.
    pub points: Vec<DVec3>,
    /// For each input point, its index in `points`.
    pub input: Vec<usize>,
    pub faces: Vec<HullFace>,
}

impl Hull {
    pub fn is_corner(&self, point: usize) -> bool {
        self.faces.iter().any(|f| f.corners.contains(&point))
    }
}

/// Convex hull by enumerating supporting planes; meant for the few dozen points of a brush.
/// Coplanar points share one face, and points on a face's interior or edge are not corners.
pub fn convex_hull(input: &[DVec3]) -> Result<Hull, HullError> {
    let mut welder = Welder::default();
    let mut points: Vec<DVec3> = Vec::new();
    let mut map = Vec::with_capacity(input.len());
    for &p in input {
        let (i, added) = welder.insert(p);
        if added {
            points.push(p);
        }
        map.push(i);
    }
    let n = points.len();
    if n < 4 {
        return Err(HullError::Flat);
    }

    let mut found: Vec<(Plane, Vec<bool>)> = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let cross = (points[j] - points[i]).cross(points[k] - points[i]);
                let len = cross.length();
                if len <= 1e-9 {
                    continue;
                }
                if found.iter().any(|(_, on)| on[i] && on[j] && on[k]) {
                    continue;
                }
                let normal = cross / len;
                let mut plane = Plane {
                    normal,
                    dist: normal.dot(points[i]),
                };
                let (mut front, mut back) = (false, false);
                let mut on = vec![false; n];
                for (idx, &p) in points.iter().enumerate() {
                    let d = signed_distance(&plane, p);
                    if d > SIDE_EPS {
                        front = true;
                    } else if d < -SIDE_EPS {
                        back = true;
                    } else {
                        on[idx] = true;
                    }
                    if front && back {
                        break;
                    }
                }
                if front && back {
                    continue;
                }
                if !front && !back {
                    return Err(HullError::Flat);
                }
                if front {
                    plane = flip(&plane);
                }
                found.push((plane, on));
            }
        }
    }

    let faces: Vec<HullFace> = found
        .into_iter()
        .filter_map(|(plane, on)| {
            let (u, v) = basis(plane.normal);
            let flat: Vec<(usize, DVec2)> = on
                .iter()
                .enumerate()
                .filter(|(_, o)| **o)
                .map(|(i, _)| (i, DVec2::new(points[i].dot(u), points[i].dot(v))))
                .collect();
            let corners = corners_2d(&flat);
            (corners.len() >= 3).then_some(HullFace { plane, corners })
        })
        .collect();
    if faces.len() < 4 {
        return Err(HullError::Flat);
    }
    Ok(Hull {
        points,
        input: map,
        faces,
    })
}

/// `u x v == n`, so counter-clockwise in (u, v) is counter-clockwise seen from outside.
fn basis(n: DVec3) -> (DVec3, DVec3) {
    let abs = n.abs();
    let helper = if abs.x <= abs.y && abs.x <= abs.z {
        DVec3::X
    } else if abs.y <= abs.z {
        DVec3::Y
    } else {
        DVec3::Z
    };
    let u = helper.cross(n).normalize();
    (u, n.cross(u))
}

fn corners_2d(points: &[(usize, DVec2)]) -> Vec<usize> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.1.x.total_cmp(&b.1.x).then(a.1.y.total_cmp(&b.1.y)));
    let keep = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o) > SIDE_EPS * (b - o).length();
    let chain = |order: &mut dyn Iterator<Item = &(usize, DVec2)>| {
        let mut h: Vec<(usize, DVec2)> = Vec::new();
        for &(i, p) in order {
            while h.len() >= 2 && !keep(h[h.len() - 2].1, h[h.len() - 1].1, p) {
                h.pop();
            }
            h.push((i, p));
        }
        h.pop();
        h
    };
    let mut lower = chain(&mut sorted.iter());
    let upper = chain(&mut sorted.iter().rev());
    lower.extend(upper);
    lower.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_points() -> Vec<DVec3> {
        (0..8)
            .map(|i| {
                DVec3::new(
                    (i & 1) as f64 * 64.0,
                    ((i >> 1) & 1) as f64 * 64.0,
                    ((i >> 2) & 1) as f64 * 64.0,
                )
            })
            .collect()
    }

    #[test]
    fn cube_has_six_quads() {
        let h = convex_hull(&cube_points()).unwrap();
        assert_eq!(h.faces.len(), 6);
        assert!(h.faces.iter().all(|f| f.corners.len() == 4));
    }

    #[test]
    fn interior_and_edge_points_are_not_corners() {
        let mut pts = cube_points();
        pts.push(DVec3::new(32.0, 32.0, 32.0));
        pts.push(DVec3::new(32.0, 0.0, 0.0));
        let h = convex_hull(&pts).unwrap();
        assert_eq!(h.faces.len(), 6);
        assert!(!h.is_corner(h.input[8]));
        assert!(!h.is_corner(h.input[9]));
    }

    #[test]
    fn flat_input_fails() {
        let pts = [DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::new(1.0, 1.0, 0.0)];
        assert_eq!(convex_hull(&pts), Err(HullError::Flat));
    }

    #[test]
    fn faces_wind_counter_clockwise_from_outside() {
        let h = convex_hull(&cube_points()).unwrap();
        for f in &h.faces {
            let poly: Vec<DVec3> = f.corners.iter().map(|&i| h.points[i]).collect();
            let n = crate::winding::newell(&poly);
            assert!(n.dot(f.plane.normal) > 0.0);
        }
    }
}

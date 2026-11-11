use editor_geom::Aabb;
use glam::DVec3;

pub const DEFAULT_DEPTH: f64 = 64.0;

/// The axis-aligned plane the box tool draws its rectangle on, and the side the box grows towards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxPlane {
    pub axis: usize,
    pub offset: f64,
    pub sign: f64,
}

impl BoxPlane {
    pub fn ground() -> BoxPlane {
        BoxPlane {
            axis: 2,
            offset: 0.0,
            sign: 1.0,
        }
    }

    /// The plane of the face hit at `point`, through its dominant axis.
    pub fn from_hit(point: DVec3, normal: DVec3) -> BoxPlane {
        let abs = normal.abs();
        let axis = if abs.x >= abs.y && abs.x >= abs.z {
            0
        } else if abs.y >= abs.z {
            1
        } else {
            2
        };
        BoxPlane {
            axis,
            offset: point[axis],
            sign: if normal[axis] < 0.0 { -1.0 } else { 1.0 },
        }
    }

    pub fn point_at(&self, (origin, dir): (DVec3, DVec3)) -> Option<DVec3> {
        if dir[self.axis].abs() < 1e-9 {
            return None;
        }
        let t = (self.offset - origin[self.axis]) / dir[self.axis];
        (t > 0.0).then(|| {
            let mut p = origin + dir * t;
            p[self.axis] = self.offset;
            p
        })
    }

    /// The box with `a` and `b` as opposite corners of its base, `depth` tall away from the plane.
    pub fn cuboid(&self, a: DVec3, b: DVec3, depth: f64) -> Aabb {
        let mut min = a.min(b);
        let mut max = a.max(b);
        if self.sign > 0.0 {
            min[self.axis] = self.offset;
            max[self.axis] = self.offset + depth;
        } else {
            min[self.axis] = self.offset - depth;
            max[self.axis] = self.offset;
        }
        Aabb { min, max }
    }

    pub fn has_area(&self, a: DVec3, b: DVec3) -> bool {
        (0..3)
            .filter(|&i| i != self.axis)
            .all(|i| (a[i] - b[i]).abs() > 0.0)
    }
}

/// How far a new box reaches along `axis`: the hint's extent there, else the default.
pub fn depth_along(axis: usize, hint: Option<Aabb>) -> f64 {
    hint.map(|b| b.max[axis] - b.min[axis])
        .filter(|d| *d > 0.0)
        .unwrap_or(DEFAULT_DEPTH)
}

/// The hint's `[min, max]` along `axis`, else `[0, default]`.
pub fn range_along(axis: usize, hint: Option<Aabb>) -> (f64, f64) {
    hint.filter(|b| b.max[axis] > b.min[axis])
        .map_or((0.0, DEFAULT_DEPTH), |b| (b.min[axis], b.max[axis]))
}

/// The twelve edges of a box as pairs of corners.
pub fn edges(b: &Aabb) -> Vec<(DVec3, DVec3)> {
    let corner = |i: usize| {
        DVec3::new(
            if i & 1 == 0 { b.min.x } else { b.max.x },
            if i & 2 == 0 { b.min.y } else { b.max.y },
            if i & 4 == 0 { b.min.z } else { b.max.z },
        )
    };
    (0..8)
        .flat_map(|i| [1, 2, 4].map(|bit| (i, i | bit)))
        .filter(|(i, j)| i != j)
        .map(|(i, j)| (corner(i), corner(j)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plane_follows_the_dominant_axis_of_the_hit_normal() {
        let p = BoxPlane::from_hit(DVec3::new(1.0, 2.0, 3.0), DVec3::new(-0.9, 0.1, 0.2));
        assert_eq!(
            p,
            BoxPlane {
                axis: 0,
                offset: 1.0,
                sign: -1.0
            }
        );
        let up = BoxPlane::from_hit(DVec3::new(0.0, 0.0, 8.0), DVec3::Z);
        assert_eq!((up.axis, up.offset, up.sign), (2, 8.0, 1.0));
    }

    #[test]
    fn a_ray_meets_the_plane_in_front_of_it_only() {
        let ground = BoxPlane::ground();
        let hit = ground
            .point_at((DVec3::new(3.0, 4.0, 10.0), DVec3::new(0.0, 0.0, -1.0)))
            .unwrap();
        assert_eq!(hit, DVec3::new(3.0, 4.0, 0.0));
        assert!(
            ground
                .point_at((DVec3::new(3.0, 4.0, 10.0), DVec3::Z))
                .is_none()
        );
        assert!(ground.point_at((DVec3::ZERO, DVec3::X)).is_none());
    }

    #[test]
    fn boxes_grow_away_from_the_face_they_are_drawn_on() {
        let up = BoxPlane::from_hit(DVec3::new(0.0, 0.0, 8.0), DVec3::Z);
        let b = up.cuboid(DVec3::new(0.0, 0.0, 8.0), DVec3::new(16.0, 32.0, 8.0), 24.0);
        assert_eq!(
            (b.min, b.max),
            (DVec3::new(0.0, 0.0, 8.0), DVec3::new(16.0, 32.0, 32.0))
        );

        let down = BoxPlane::from_hit(DVec3::new(0.0, 0.0, 8.0), DVec3::NEG_Z);
        let b = down.cuboid(DVec3::new(16.0, 32.0, 8.0), DVec3::new(0.0, 0.0, 8.0), 24.0);
        assert_eq!(
            (b.min, b.max),
            (DVec3::new(0.0, 0.0, -16.0), DVec3::new(16.0, 32.0, 8.0))
        );
    }

    #[test]
    fn a_flat_rectangle_has_no_area() {
        let ground = BoxPlane::ground();
        assert!(!ground.has_area(DVec3::ZERO, DVec3::new(8.0, 0.0, 0.0)));
        assert!(ground.has_area(DVec3::ZERO, DVec3::new(8.0, 8.0, 0.0)));
    }

    #[test]
    fn depth_comes_from_the_hint_or_falls_back() {
        let hint = Aabb {
            min: DVec3::new(0.0, 0.0, 16.0),
            max: DVec3::new(8.0, 8.0, 48.0),
        };
        assert_eq!(depth_along(2, Some(hint)), 32.0);
        assert_eq!(range_along(2, Some(hint)), (16.0, 48.0));
        assert_eq!(depth_along(2, None), DEFAULT_DEPTH);
        assert_eq!(range_along(2, None), (0.0, DEFAULT_DEPTH));
        let flat = Aabb {
            min: DVec3::ZERO,
            max: DVec3::new(8.0, 8.0, 0.0),
        };
        assert_eq!(depth_along(2, Some(flat)), DEFAULT_DEPTH);
    }

    #[test]
    fn a_box_has_twelve_distinct_edges() {
        let b = Aabb {
            min: DVec3::ZERO,
            max: DVec3::ONE,
        };
        let e = edges(&b);
        assert_eq!(e.len(), 12);
        assert!(e.iter().all(|(a, b)| (a.distance(*b) - 1.0).abs() < 1e-12));
    }
}

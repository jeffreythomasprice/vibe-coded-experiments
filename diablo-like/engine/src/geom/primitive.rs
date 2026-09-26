use std::f32::consts::PI;

use glam::{Mat2, Vec2};

use super::aabb::Aabb;

/// Translation + rotation + UNIFORM scale only. Non-uniform scale would
/// break distance-field validity (see the Geometry model section of the
/// design plan): a shrinking non-uniform scale can make the composed field
/// *overestimate* true distance, which is unsafe for sphere-trace collision.
#[derive(Copy, Clone, Debug)]
pub struct Similarity {
    pub translation: Vec2,
    pub rotation: f32,
    pub scale: f32,
}

impl Similarity {
    pub fn identity() -> Self {
        Self {
            translation: Vec2::ZERO,
            rotation: 0.0,
            scale: 1.0,
        }
    }

    pub fn translate(translation: Vec2) -> Self {
        Self {
            translation,
            ..Self::identity()
        }
    }

    /// Maps a point from this transform's local space to its parent space.
    pub fn to_world(self, local_p: Vec2) -> Vec2 {
        self.rotate_dir(local_p) * self.scale + self.translation
    }

    /// Maps a point from parent space into this transform's local space —
    /// the inverse of `to_world`, used to query a child's field at a
    /// parent-space point.
    pub fn to_local(self, world_p: Vec2) -> Vec2 {
        Mat2::from_angle(-self.rotation) * ((world_p - self.translation) / self.scale)
    }

    /// Rotates a direction (no translation, no scale) — for carrying a
    /// child's gradient into parent space, since gradients are unit
    /// directions unaffected by translation or uniform scale.
    pub fn rotate_dir(&self, v: Vec2) -> Vec2 {
        Mat2::from_angle(self.rotation) * v
    }

    pub fn scale_dist(&self, local_d: f32) -> f32 {
        local_d * self.scale
    }
}

/// `d` follows the standard SDF convention: negative inside the shape,
/// positive outside. `grad` is the unit-length gradient of `d`, i.e. the
/// direction of fastest increase — pointing from the nearest boundary point
/// toward `p`. Degenerate at each primitive's own medial axis (arbitrary but
/// deterministic fallback direction), same as any SDF.
#[derive(Copy, Clone, Debug)]
pub struct Sample {
    pub d: f32,
    pub grad: Vec2,
}

#[derive(Copy, Clone, Debug)]
pub struct Circle {
    pub radius: f32,
}

#[derive(Copy, Clone, Debug)]
pub struct RoundRect {
    pub half_extents: Vec2,
    pub radius: f32,
}

/// A capsule: the set of points within `radius` of the segment from
/// `(-half_length, 0)` to `(half_length, 0)` in local space. Combine with a
/// `Similarity` for arbitrary position/orientation.
#[derive(Copy, Clone, Debug)]
pub struct Capsule {
    pub half_length: f32,
    pub radius: f32,
}

/// A convex polygon, vertices wound counter-clockwise. Not validated at
/// construction; callers are responsible for convexity and winding.
#[derive(Clone, Debug)]
pub struct ConvexPoly {
    pub verts: Vec<Vec2>,
}

#[derive(Clone, Debug)]
pub enum Primitive {
    Circle(Circle),
    RoundRect(RoundRect),
    Capsule(Capsule),
    ConvexPoly(ConvexPoly),
}

/// Segments needed to flatten a circular arc of the given radius so the
/// sagitta (max deviation from the true arc) stays within `tol`.
fn arc_segments(radius: f32, tol: f32) -> usize {
    if radius <= tol {
        return 1;
    }
    let cos_half_step = (1.0 - tol / radius).clamp(-1.0, 1.0);
    let half_step = cos_half_step.acos();
    if half_step <= 0.0 {
        return 128;
    }
    let n = (PI / half_step).ceil() as usize;
    n.clamp(8, 128)
}

impl Primitive {
    pub fn sample(&self, p: Vec2) -> Sample {
        match self {
            Primitive::Circle(c) => c.sample(p),
            Primitive::RoundRect(r) => r.sample(p),
            Primitive::Capsule(c) => c.sample(p),
            Primitive::ConvexPoly(c) => c.sample(p),
        }
    }

    pub fn aabb(&self) -> Aabb {
        match self {
            Primitive::Circle(c) => Aabb {
                min: Vec2::splat(-c.radius),
                max: Vec2::splat(c.radius),
            },
            Primitive::RoundRect(r) => Aabb {
                min: -r.half_extents,
                max: r.half_extents,
            },
            Primitive::Capsule(c) => Aabb {
                min: Vec2::new(-c.half_length - c.radius, -c.radius),
                max: Vec2::new(c.half_length + c.radius, c.radius),
            },
            Primitive::ConvexPoly(c) => c
                .verts
                .iter()
                .fold(Aabb::EMPTY, |acc, &v| acc.union(Aabb::point(v))),
        }
    }

    /// Approximates the primitive's boundary as a closed, CCW polygon within
    /// `tol` of the true curve. Used only for contour/crisp-mode export —
    /// the field bake and physics both use the exact analytic `sample`.
    pub fn flatten(&self, tol: f32) -> Vec<Vec2> {
        match self {
            Primitive::Circle(c) => flatten_circle(c.radius, tol),
            Primitive::RoundRect(r) => flatten_round_rect(r, tol),
            Primitive::Capsule(c) => flatten_capsule(c, tol),
            Primitive::ConvexPoly(c) => c.verts.clone(),
        }
    }
}

impl Circle {
    fn sample(&self, p: Vec2) -> Sample {
        let len = p.length();
        let grad = if len > 1e-6 { p / len } else { Vec2::X };
        Sample {
            d: len - self.radius,
            grad,
        }
    }
}

fn flatten_circle(radius: f32, tol: f32) -> Vec<Vec2> {
    let n = arc_segments(radius, tol).max(12);
    (0..n)
        .map(|i| {
            let a = 2.0 * PI * i as f32 / n as f32;
            Vec2::new(a.cos(), a.sin()) * radius
        })
        .collect()
}

impl RoundRect {
    fn sample(&self, p: Vec2) -> Sample {
        let inner = self.half_extents - Vec2::splat(self.radius);
        let q = p.abs() - inner;
        let qx_pos = q.x.max(0.0);
        let qy_pos = q.y.max(0.0);
        let outside_len = Vec2::new(qx_pos, qy_pos).length();
        let d = outside_len + q.x.max(q.y).min(0.0) - self.radius;

        let grad = if qx_pos > 0.0 && qy_pos > 0.0 {
            let dir = Vec2::new(qx_pos, qy_pos).normalize();
            Vec2::new(dir.x * p.x.signum(), dir.y * p.y.signum())
        } else if q.x > q.y {
            Vec2::new(p.x.signum(), 0.0)
        } else {
            Vec2::new(0.0, p.y.signum())
        };
        let grad = if grad == Vec2::ZERO { Vec2::X } else { grad };

        Sample { d, grad }
    }
}

fn flatten_round_rect(r: &RoundRect, tol: f32) -> Vec<Vec2> {
    let inner = r.half_extents - Vec2::splat(r.radius);
    if r.radius <= tol {
        return vec![
            Vec2::new(r.half_extents.x, r.half_extents.y),
            Vec2::new(-r.half_extents.x, r.half_extents.y),
            Vec2::new(-r.half_extents.x, -r.half_extents.y),
            Vec2::new(r.half_extents.x, -r.half_extents.y),
        ];
    }

    let segs = (arc_segments(r.radius, tol) / 4).max(2);
    let mut points = Vec::with_capacity(segs * 4 + 4);
    // Four quarter arcs, one per corner, walked CCW starting at the +X/+Y corner.
    let centers = [
        Vec2::new(inner.x, inner.y),
        Vec2::new(-inner.x, inner.y),
        Vec2::new(-inner.x, -inner.y),
        Vec2::new(inner.x, -inner.y),
    ];
    for (corner, &center) in centers.iter().enumerate() {
        let start_angle = corner as f32 * PI / 2.0;
        for i in 0..=segs {
            let a = start_angle + i as f32 / segs as f32 * PI / 2.0;
            points.push(center + Vec2::new(a.cos(), a.sin()) * r.radius);
        }
    }
    points
}

impl Capsule {
    fn segment(&self) -> (Vec2, Vec2) {
        (
            Vec2::new(-self.half_length, 0.0),
            Vec2::new(self.half_length, 0.0),
        )
    }

    fn sample(&self, p: Vec2) -> Sample {
        let (a, b) = self.segment();
        let ab = b - a;
        let denom = ab.length_squared();
        let t = if denom > 1e-12 {
            ((p - a).dot(ab) / denom).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let closest = a + ab * t;
        let offset = p - closest;
        let len = offset.length();
        let grad = if len > 1e-6 { offset / len } else { Vec2::Y };
        Sample {
            d: len - self.radius,
            grad,
        }
    }
}

fn flatten_capsule(c: &Capsule, tol: f32) -> Vec<Vec2> {
    let segs = (arc_segments(c.radius, tol) / 2).max(4);
    let mut points = Vec::with_capacity(segs * 2 + 2);
    // Right cap (around (+half_length, 0)), sweeping from -90 to +90 degrees.
    for i in 0..=segs {
        let a = -PI / 2.0 + i as f32 / segs as f32 * PI;
        points.push(Vec2::new(c.half_length, 0.0) + Vec2::new(a.cos(), a.sin()) * c.radius);
    }
    // Left cap (around (-half_length, 0)), sweeping from +90 to +270 degrees.
    for i in 0..=segs {
        let a = PI / 2.0 + i as f32 / segs as f32 * PI;
        points.push(Vec2::new(-c.half_length, 0.0) + Vec2::new(a.cos(), a.sin()) * c.radius);
    }
    points
}

fn closest_point_on_segment(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    let denom = ab.length_squared();
    let t = if denom > 1e-12 {
        ((p - a).dot(ab) / denom).clamp(0.0, 1.0)
    } else {
        0.0
    };
    a + ab * t
}

impl ConvexPoly {
    fn sample(&self, p: Vec2) -> Sample {
        let n = self.verts.len();
        debug_assert!(n >= 3, "ConvexPoly needs at least 3 vertices");

        let mut inside = true;
        let mut best_dist_sq = f32::INFINITY;
        let mut best_point = Vec2::ZERO;

        for i in 0..n {
            let a = self.verts[i];
            let b = self.verts[(i + 1) % n];
            let edge = b - a;
            let to_p = p - a;
            let cross = edge.x * to_p.y - edge.y * to_p.x;
            if cross < 0.0 {
                inside = false;
            }

            let cp = closest_point_on_segment(p, a, b);
            let d2 = (p - cp).length_squared();
            if d2 < best_dist_sq {
                best_dist_sq = d2;
                best_point = cp;
            }
        }

        let dist = best_dist_sq.sqrt();
        let sign = if inside { -1.0 } else { 1.0 };
        let offset = p - best_point;
        let grad = if dist > 1e-6 {
            (offset / dist) * sign
        } else {
            Vec2::X
        };

        Sample {
            d: sign * dist,
            grad,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brute_force_distance(boundary: &[Vec2], p: Vec2) -> f32 {
        let n = boundary.len();
        (0..n)
            .map(|i| {
                let a = boundary[i];
                let b = boundary[(i + 1) % n];
                (p - closest_point_on_segment(p, a, b)).length()
            })
            .fold(f32::INFINITY, f32::min)
    }

    fn assert_matches_boundary(sample_fn: impl Fn(Vec2) -> Sample, boundary: &[Vec2], p: Vec2, tol: f32) {
        let s = sample_fn(p);
        let expected_unsigned = brute_force_distance(boundary, p);
        assert!(
            (s.d.abs() - expected_unsigned).abs() < tol,
            "at {p:?}: d={}, expected unsigned dist={expected_unsigned}",
            s.d
        );
        assert!(
            (s.grad.length() - 1.0).abs() < 1e-4,
            "gradient not unit length: {:?}",
            s.grad
        );
    }

    #[test]
    fn circle_matches_brute_force() {
        let c = Circle { radius: 2.0 };
        let boundary = flatten_circle(2.0, 0.001);
        for p in [
            Vec2::new(0.5, 0.0),
            Vec2::new(3.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(-2.5, 0.3),
        ] {
            assert_matches_boundary(|p| c.sample(p), &boundary, p, 0.01);
        }
        // sign check: origin is inside, far point is outside
        assert!(c.sample(Vec2::ZERO).d < 0.0);
        assert!(c.sample(Vec2::new(10.0, 0.0)).d > 0.0);
    }

    #[test]
    fn round_rect_matches_brute_force_sharp() {
        let r = RoundRect {
            half_extents: Vec2::new(2.0, 1.0),
            radius: 0.0,
        };
        let boundary = flatten_round_rect(&r, 0.001);
        for p in [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.9, 0.0),
            Vec2::new(3.0, 0.5),
            Vec2::new(2.5, 1.5),
            Vec2::new(-3.0, -2.0),
        ] {
            assert_matches_boundary(|p| r.sample(p), &boundary, p, 0.01);
        }
        assert!(r.sample(Vec2::ZERO).d < 0.0);
        assert!(r.sample(Vec2::new(10.0, 10.0)).d > 0.0);
    }

    #[test]
    fn round_rect_matches_brute_force_rounded() {
        let r = RoundRect {
            half_extents: Vec2::new(2.0, 1.0),
            radius: 0.4,
        };
        let boundary = flatten_round_rect(&r, 0.001);
        for p in [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.9, 0.9),
            Vec2::new(3.0, 0.5),
            Vec2::new(2.5, 1.5),
        ] {
            assert_matches_boundary(|p| r.sample(p), &boundary, p, 0.02);
        }
    }

    #[test]
    fn capsule_matches_brute_force() {
        let c = Capsule {
            half_length: 1.5,
            radius: 0.5,
        };
        let boundary = flatten_capsule(&c, 0.001);
        for p in [
            Vec2::new(0.0, 0.0),
            Vec2::new(1.5, 0.0),
            Vec2::new(2.5, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(-2.0, 0.3),
        ] {
            assert_matches_boundary(|p| c.sample(p), &boundary, p, 0.01);
        }
        assert!(c.sample(Vec2::ZERO).d < 0.0);
        assert!(c.sample(Vec2::new(5.0, 0.0)).d > 0.0);
    }

    #[test]
    fn convex_poly_matches_brute_force() {
        // A CCW square [-1,1]^2
        let poly = ConvexPoly {
            verts: vec![
                Vec2::new(1.0, -1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(-1.0, 1.0),
                Vec2::new(-1.0, -1.0),
            ],
        };
        let boundary = poly.verts.clone();
        for p in [
            Vec2::new(0.0, 0.0),
            Vec2::new(0.9, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(0.5, 0.9),
        ] {
            assert_matches_boundary(|p| poly.sample(p), &boundary, p, 0.01);
        }
        assert!(poly.sample(Vec2::ZERO).d < 0.0);
        assert!(poly.sample(Vec2::new(5.0, 5.0)).d > 0.0);
    }

    #[test]
    fn similarity_round_trip() {
        let sim = Similarity {
            translation: Vec2::new(3.0, -2.0),
            rotation: 0.7,
            scale: 2.0,
        };
        let p = Vec2::new(1.0, 4.0);
        let world = sim.to_world(p);
        let back = sim.to_local(world);
        assert!((back - p).length() < 1e-4);
    }
}

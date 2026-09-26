use glam::Vec2;

use super::primitive::Similarity;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub const EMPTY: Aabb = Aabb {
        min: Vec2::new(f32::INFINITY, f32::INFINITY),
        max: Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
    };

    pub const INFINITE: Aabb = Aabb {
        min: Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
        max: Vec2::new(f32::INFINITY, f32::INFINITY),
    };

    pub fn point(p: Vec2) -> Aabb {
        Aabb { min: p, max: p }
    }

    pub fn union(self, other: Aabb) -> Aabb {
        Aabb {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn intersect(self, other: Aabb) -> Aabb {
        Aabb {
            min: self.min.max(other.min),
            max: self.max.min(other.max),
        }
    }

    pub fn expand(self, r: f32) -> Aabb {
        Aabb {
            min: self.min - Vec2::splat(r),
            max: self.max + Vec2::splat(r),
        }
    }

    pub fn overlaps(&self, other: &Aabb) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
    }

    /// Distance from `p` to the nearest point of the box; 0 if `p` is inside.
    /// A valid lower bound on distance to anything the box encloses, so it's
    /// safe to prune a `Union` child when this exceeds the current best.
    pub fn dist_to(&self, p: Vec2) -> f32 {
        let clamped = p.clamp(self.min, self.max);
        (p - clamped).length()
    }

    pub fn transform(&self, sim: &Similarity) -> Aabb {
        let corners = [
            Vec2::new(self.min.x, self.min.y),
            Vec2::new(self.max.x, self.min.y),
            Vec2::new(self.min.x, self.max.y),
            Vec2::new(self.max.x, self.max.y),
        ];
        corners
            .into_iter()
            .map(|c| Aabb::point(sim.to_world(c)))
            .fold(Aabb::EMPTY, Aabb::union)
    }
}

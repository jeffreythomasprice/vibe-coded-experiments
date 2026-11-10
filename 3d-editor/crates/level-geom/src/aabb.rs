use glam::DVec3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: DVec3,
    pub max: DVec3,
}

impl Aabb {
    pub fn from_points(points: impl IntoIterator<Item = DVec3>) -> Option<Self> {
        points.into_iter().fold(None, |acc, p| {
            Some(match acc {
                None => Aabb { min: p, max: p },
                Some(b) => Aabb {
                    min: b.min.min(p),
                    max: b.max.max(p),
                },
            })
        })
    }

    pub fn merge(self, other: Aabb) -> Aabb {
        Aabb {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn center(&self) -> DVec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> DVec3 {
        self.max - self.min
    }

    /// Radius of the bounding sphere around `center`.
    pub fn radius(&self) -> f64 {
        self.size().length() * 0.5
    }

    /// Largest absolute coordinate of the box or the origin.
    pub fn reach(&self) -> f64 {
        self.min.abs().max(self.max.abs()).max_element()
    }

    pub fn corners(&self) -> [DVec3; 8] {
        let (a, b) = (self.min, self.max);
        [
            DVec3::new(a.x, a.y, a.z),
            DVec3::new(b.x, a.y, a.z),
            DVec3::new(b.x, b.y, a.z),
            DVec3::new(a.x, b.y, a.z),
            DVec3::new(a.x, a.y, b.z),
            DVec3::new(b.x, a.y, b.z),
            DVec3::new(b.x, b.y, b.z),
            DVec3::new(a.x, b.y, b.z),
        ]
    }
}

use glam::DVec3;
use serde::{Deserialize, Serialize};

/// Points `p` on the plane satisfy `normal . p == dist`; the normal points outward.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Plane {
    pub normal: DVec3,
    pub dist: f64,
}

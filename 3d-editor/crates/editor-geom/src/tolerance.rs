use glam::DVec3;

pub const SNAP: f64 = 1.0 / 1_048_576.0;
pub const SIDE_EPS: f64 = 1e-5;
pub const PLANAR_EPS: f64 = 2e-3;
pub const AREA_EPS: f64 = 1e-9;
pub const WORLD_BOUND: f64 = 1_048_576.0;

pub fn snap(x: f64) -> f64 {
    (x / SNAP).round() * SNAP + 0.0
}

pub fn snap_vec(v: DVec3) -> DVec3 {
    DVec3::new(snap(v.x), snap(v.y), snap(v.z))
}

#[derive(Default)]
pub struct Welder {
    cells: std::collections::HashMap<(i64, i64, i64), Vec<usize>>,
    points: Vec<DVec3>,
}

impl Welder {
    fn cell(p: DVec3) -> (i64, i64, i64) {
        (
            (p.x / WELD_EPS).floor() as i64,
            (p.y / WELD_EPS).floor() as i64,
            (p.z / WELD_EPS).floor() as i64,
        )
    }

    /// Returns the index of the first point within `WELD_EPS` of `p`, adding `p` if there is none.
    pub fn insert(&mut self, p: DVec3) -> (usize, bool) {
        let (cx, cy, cz) = Self::cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = self.cells.get(&(cx + dx, cy + dy, cz + dz)) {
                        for &i in list {
                            if (self.points[i] - p).length() <= WELD_EPS {
                                return (i, false);
                            }
                        }
                    }
                }
            }
        }
        self.points.push(p);
        self.cells
            .entry((cx, cy, cz))
            .or_default()
            .push(self.points.len() - 1);
        (self.points.len() - 1, true)
    }
}

pub const WELD_EPS: f64 = 1e-3;
pub const CSG_EPS: f64 = 1e-4;

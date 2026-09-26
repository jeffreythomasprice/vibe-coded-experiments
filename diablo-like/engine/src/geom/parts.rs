use std::collections::HashMap;

use glam::Vec2;

use super::aabb::Aabb;
use super::csg::{Csg, NodeId};
use super::primitive::Sample;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PartId(u32);

struct Part {
    root: NodeId,
    bounds: Aabb,
}

/// Uniform grid over part bounds, used only to narrow down which parts a
/// query point needs to consider — the actual containment/distance test is
/// always the exact primitive/CSG evaluation.
struct PartGrid {
    cell_size: f32,
    cells: HashMap<(i32, i32), Vec<PartId>>,
}

impl PartGrid {
    fn new(cell_size: f32) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
        }
    }

    fn cell_coord(&self, p: Vec2) -> (i32, i32) {
        (
            (p.x / self.cell_size).floor() as i32,
            (p.y / self.cell_size).floor() as i32,
        )
    }

    fn insert(&mut self, id: PartId, bounds: Aabb) {
        let (min_x, min_y) = self.cell_coord(bounds.min);
        let (max_x, max_y) = self.cell_coord(bounds.max);
        for cy in min_y..=max_y {
            for cx in min_x..=max_x {
                self.cells.entry((cx, cy)).or_default().push(id);
            }
        }
    }

    fn candidates(&self, p: Vec2) -> &[PartId] {
        self.cells
            .get(&self.cell_coord(p))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Every part id registered in a cell the given bounds overlap. May
    /// contain duplicates and parts whose *actual* bounds don't reach as far
    /// as `bounds` (a cell can hold parts that only clip its corner) — a
    /// broad-phase candidate list, not a final answer.
    fn candidates_in_rect(&self, bounds: Aabb) -> Vec<PartId> {
        let (min_x, min_y) = self.cell_coord(bounds.min);
        let (max_x, max_y) = self.cell_coord(bounds.max);
        let mut out = Vec::new();
        for cy in min_y..=max_y {
            for cx in min_x..=max_x {
                if let Some(ids) = self.cells.get(&(cx, cy)) {
                    out.extend_from_slice(ids);
                }
            }
        }
        out
    }
}

/// The CSG normal form this project relies on for efficient queries: the
/// root is implicitly `Union(parts)`, where each part is a small, bounded
/// local tree (never a bare `Complement`). This is always achievable
/// because boolean ops distribute over union, so a global carve-out gets
/// pushed down into just the parts it overlaps. See the design plan's
/// "Normal form" note for why generic pruning over an arbitrary tree
/// doesn't work, but pruning over a spatially-indexed union of small parts
/// does.
pub struct Level {
    pub csg: Csg,
    parts: Vec<Part>,
    grid: PartGrid,
}

impl Level {
    pub fn new(cell_size: f32) -> Self {
        Self {
            csg: Csg::new(),
            parts: Vec::new(),
            grid: PartGrid::new(cell_size),
        }
    }

    /// Registers `root` as a part. Call `self.csg.finalize()` first so the
    /// part's bounds are up to date.
    pub fn add_part(&mut self, root: NodeId) -> PartId {
        let bounds = self.csg.bounds(root);
        let id = PartId(self.parts.len() as u32);
        self.parts.push(Part { root, bounds });
        self.grid.insert(id, bounds);
        id
    }

    pub fn eval(&self, p: Vec2) -> Sample {
        let mut best: Option<Sample> = None;
        for &id in self.grid.candidates(p) {
            let part = &self.parts[id.0 as usize];
            let lb = part.bounds.dist_to(p);
            if let Some(b) = &best
                && lb > 0.0
                && lb >= b.d
            {
                continue;
            }
            let s = self.csg.eval(part.root, p);
            if best.as_ref().is_none_or(|b| s.d < b.d) {
                best = Some(s);
            }
        }
        best.unwrap_or(Sample {
            d: f32::INFINITY,
            grad: Vec2::X,
        })
    }

    /// True if any part's actual bounds overlap `rect`. Used to skip baking
    /// a chunk entirely when nothing is within clamp range of it anywhere —
    /// the shared void layer, not a wasted bake job (see `geom::field`).
    pub fn any_part_overlaps(&self, rect: Aabb) -> bool {
        self.grid
            .candidates_in_rect(rect)
            .into_iter()
            .any(|id| self.parts[id.0 as usize].bounds.overlaps(&rect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::primitive::{Circle, Primitive};

    #[test]
    fn eval_finds_nearest_part_across_cells() {
        let mut level = Level::new(4.0);
        let a = level.csg.leaf(Primitive::Circle(Circle { radius: 1.0 }));
        level.csg.finalize();
        level.add_part(a);

        let b = level.csg.leaf(Primitive::Circle(Circle { radius: 1.0 }));
        let b = level.csg.transform(
            b,
            crate::geom::primitive::Similarity::translate(Vec2::new(10.0, 0.0)),
        );
        level.csg.finalize();
        level.add_part(b);

        assert!(level.eval(Vec2::ZERO).d < 0.0);
        assert!(level.eval(Vec2::new(10.0, 0.0)).d < 0.0);
        assert!(level.eval(Vec2::new(5.0, 0.0)).d > 0.0);

        // Far outside both parts' grid cells: no candidates, falls back to
        // the "nothing nearby" sentinel rather than panicking.
        assert!(level.eval(Vec2::new(1000.0, 1000.0)).d.is_infinite());
    }
}

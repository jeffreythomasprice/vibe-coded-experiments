use std::collections::BTreeSet;

use editor_core::{Brush, Document, FaceAttrs, ObjectId};
use editor_geom::{brush_geometry, clip_brush};
use glam::DVec3;

use editor_cmd::args::Keep;

pub type Segment = (DVec3, DVec3);

/// The clip plane being placed: up to three points, or two points plus the direction the plane runs in.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipState {
    pub points: Vec<DVec3>,
    dir: Option<DVec3>,
    pub keep: Keep,
}

impl Default for ClipState {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            dir: None,
            keep: Keep::Front,
        }
    }
}

impl ClipState {
    /// Three points on the plane; the normal `(c - a) x (b - a)` points to the front.
    pub fn plane(&self) -> Option<[DVec3; 3]> {
        match (self.points.as_slice(), self.dir) {
            ([a, b, c], _) => Some([*a, *b, *c]),
            ([a, b], Some(dir)) => Some([*a, *b, *a + dir]),
            _ => None,
        }
    }

    /// `dir` is the direction the plane contains if this is the second point.
    pub fn add(&mut self, point: DVec3, dir: DVec3) {
        if self.points.len() >= 3 {
            return;
        }
        self.points.push(point);
        if self.points.len() == 2 {
            self.dir = Some(dir);
        }
    }

    pub fn pop(&mut self) {
        self.points.pop();
        if self.points.len() < 2 {
            self.dir = None;
        }
    }

    pub fn clear(&mut self) {
        self.points.clear();
        self.dir = None;
    }

    /// Swaps front and back.
    pub fn reverse(&mut self) {
        match self.points.len() {
            3 => self.points.swap(0, 2),
            2 => self.points.swap(0, 1),
            _ => {}
        }
    }

    pub fn cycle_keep(&mut self) {
        self.keep = match self.keep {
            Keep::Front => Keep::Back,
            Keep::Back => Keep::Both,
            Keep::Both => Keep::Front,
        };
    }
}

/// Wireframes of what a clip would leave on each side of the plane.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preview {
    pub front: Vec<Segment>,
    pub back: Vec<Segment>,
}

pub fn brush_ids(doc: &Document, selection: &BTreeSet<ObjectId>) -> Vec<ObjectId> {
    doc.with_members(selection)
        .into_iter()
        .filter(|id| matches!(id, ObjectId::Brush(_)))
        .collect()
}

pub fn preview(doc: &Document, selection: &BTreeSet<ObjectId>, plane: [DVec3; 3]) -> Preview {
    let mut out = Preview::default();
    for id in brush_ids(doc, selection) {
        let ObjectId::Brush(b) = id else { continue };
        let Some(obj) = doc.brushes.get(&b) else {
            continue;
        };
        if let Ok(r) = clip_brush(&obj.brush, plane, FaceAttrs::default()) {
            out.front.extend(r.front.iter().flat_map(edges));
            out.back.extend(r.back.iter().flat_map(edges));
        }
    }
    out
}

fn edges(brush: &Brush) -> Vec<Segment> {
    let Ok(geo) = brush_geometry(brush) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for face in &geo.faces {
        for (k, &i) in face.loop_.iter().enumerate() {
            let j = face.loop_[(k + 1) % face.loop_.len()];
            if i < j {
                out.push((geo.vertices[i], geo.vertices[j]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_points_use_the_direction() {
        let mut c = ClipState::default();
        assert_eq!(c.plane(), None);
        c.add(DVec3::new(1.0, 0.0, 0.0), DVec3::Z);
        assert_eq!(c.plane(), None);
        c.add(DVec3::new(1.0, 1.0, 0.0), DVec3::Z);
        assert_eq!(
            c.plane(),
            Some([
                DVec3::new(1.0, 0.0, 0.0),
                DVec3::new(1.0, 1.0, 0.0),
                DVec3::new(1.0, 0.0, 1.0)
            ])
        );
    }

    #[test]
    fn a_third_point_replaces_the_direction_and_pop_restores_it() {
        let mut c = ClipState::default();
        c.add(DVec3::ZERO, DVec3::Z);
        c.add(DVec3::X, DVec3::Z);
        c.add(DVec3::Y, DVec3::ZERO);
        assert_eq!(c.plane(), Some([DVec3::ZERO, DVec3::X, DVec3::Y]));
        c.pop();
        assert_eq!(c.plane(), Some([DVec3::ZERO, DVec3::X, DVec3::Z]));
        c.pop();
        c.pop();
        assert_eq!(c.plane(), None);
    }

    #[test]
    fn reverse_flips_the_normal() {
        let normal = |p: [DVec3; 3]| (p[2] - p[0]).cross(p[1] - p[0]);
        for n in [2, 3] {
            let mut c = ClipState::default();
            c.add(DVec3::ZERO, DVec3::Z);
            c.add(DVec3::X, DVec3::Z);
            if n == 3 {
                c.add(DVec3::Y, DVec3::ZERO);
            }
            let before = normal(c.plane().unwrap());
            c.reverse();
            assert!(normal(c.plane().unwrap()).dot(before) < 0.0);
        }
    }

    #[test]
    fn keep_cycles_through_all_three() {
        let mut c = ClipState::default();
        let seen: Vec<Keep> = (0..3)
            .map(|_| {
                c.cycle_keep();
                c.keep
            })
            .collect();
        assert_eq!(seen, [Keep::Back, Keep::Both, Keep::Front]);
    }
}

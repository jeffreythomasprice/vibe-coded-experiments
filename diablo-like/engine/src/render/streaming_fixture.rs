use glam::Vec2;

use crate::geom::csg::NodeId;
use crate::geom::parts::Level;
use crate::geom::primitive::{Capsule, Circle, Primitive, RoundRect, Similarity};

/// M6's verification fixture: a hand-authored (not procedurally generated —
/// that's still deferred) zigzag chain of rooms connected by corridors, with
/// a few holes carved out, spanning enough chunks to genuinely exercise
/// streaming. Fixed/deterministic on purpose: this is a streaming-system
/// test bed, not the real level generator.
const ROOM_CENTERS: &[Vec2] = &[
    Vec2::new(0.0, 0.0),
    Vec2::new(14.0, 3.0),
    Vec2::new(28.0, -2.0),
    Vec2::new(42.0, 4.0),
    Vec2::new(56.0, -3.0),
    Vec2::new(70.0, 2.0),
    Vec2::new(84.0, -2.0),
    Vec2::new(98.0, 3.0),
];
const ROOM_HALF_EXTENTS: Vec2 = Vec2::new(3.0, 2.5);
const ROOM_CORNER_RADIUS: f32 = 0.4;
const CORRIDOR_RADIUS: f32 = 1.2;
const HOLE_RADIUS: f32 = 1.0;
/// Indices (into `ROOM_CENTERS`) of rooms that get a circular hole carved
/// out of their center.
const ROOMS_WITH_HOLES: &[usize] = &[1, 3, 5];

pub const GRID_CELL_SIZE: f32 = 16.0;

/// Returns the populated `Level` (each room/corridor registered as its own
/// part, for the chunked field bake) and one `NodeId` that's the union of
/// every part's root, purely for `contour::extract`'s single-root API —
/// contour/tess stay unchunked (see the design plan): their cost scales
/// with primitive count, not world size, so a whole-fixture extract is fine
/// even though the field itself must be baked per chunk.
pub fn build() -> (Level, NodeId) {
    let mut level = Level::new(GRID_CELL_SIZE);
    let mut roots = Vec::new();

    for (i, &center) in ROOM_CENTERS.iter().enumerate() {
        let room_leaf = level.csg.leaf(Primitive::RoundRect(RoundRect {
            half_extents: ROOM_HALF_EXTENTS,
            radius: ROOM_CORNER_RADIUS,
        }));
        let room = level
            .csg
            .transform(room_leaf, Similarity::translate(center));

        let root = if ROOMS_WITH_HOLES.contains(&i) {
            let hole_leaf = level.csg.leaf(Primitive::Circle(Circle { radius: HOLE_RADIUS }));
            let hole = level
                .csg
                .transform(hole_leaf, Similarity::translate(center));
            level.csg.difference(room, hole)
        } else {
            room
        };

        level.csg.finalize();
        level.add_part(root);
        roots.push(root);
    }

    for pair in ROOM_CENTERS.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let mid = (a + b) * 0.5;
        let half_length = (b - a).length() * 0.5;
        let angle = (b - a).to_angle();

        let corridor_leaf = level.csg.leaf(Primitive::Capsule(Capsule {
            half_length,
            radius: CORRIDOR_RADIUS,
        }));
        let corridor = level.csg.transform(
            corridor_leaf,
            Similarity {
                translation: mid,
                rotation: angle,
                scale: 1.0,
            },
        );

        level.csg.finalize();
        level.add_part(corridor);
        roots.push(corridor);
    }

    let contour_root = level.csg.union(roots);
    level.csg.finalize();
    (level, contour_root)
}

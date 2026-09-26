use glam::Vec2;

use crate::geom::parts::Level;

use super::body::Actor;

const MAX_TRACE_ITERS: u32 = 32;
/// Absolute, not relative — this project's world scale is fixed and known
/// (see the design plan's Physics details note).
const CONTACT_EPS: f32 = 1e-3;
const MIN_ADVANCE: f32 = 1e-3;
/// Shrinks each step so the trace never sits exactly on the isosurface,
/// where the next iteration's clearance could read as slightly negative.
const TRACE_SAFETY: f32 = 0.95;
const MAX_DEPENETRATE_ITERS: u32 = 3;
const MAX_SLIDE_PASSES: u32 = 3;

#[derive(Copy, Clone, Debug)]
pub struct Contact {
    pub point: Vec2,
    pub normal: Vec2,
}

#[derive(Copy, Clone, Debug)]
struct TraceHit {
    t: f32,
    point: Vec2,
    normal: Vec2,
}

struct TraceResult {
    hit: Option<TraceHit>,
    iterations: u32,
}

/// Outward from the wall into free space. This project's sign convention
/// has `d < 0` inside floor/free space (see the design plan), so `grad`
/// (pointing toward increasing `d`, i.e. into the wall) negated gives the
/// direction back into free space.
fn outward_normal(grad: Vec2) -> Vec2 {
    let n = -grad;
    if n.length_squared() > 1e-8 {
        n.normalize()
    } else {
        Vec2::Y
    }
}

/// Sphere-traces a disk of `radius` from `from` along unit vector `dir` up
/// to `dist`, stopping just before it would touch a wall. The step size is
/// simply the field's own clearance value — see the design plan's Physics
/// details note for why that alone is what makes this provably safe
/// against tunneling: the composed field is a conservative underestimate of
/// true distance, so advancing by `clearance` can never step past a wall.
fn sphere_trace(level: &Level, from: Vec2, dir: Vec2, dist: f32, radius: f32) -> TraceResult {
    if dist <= 0.0 {
        return TraceResult {
            hit: None,
            iterations: 0,
        };
    }

    let mut t = 0.0f32;
    for iterations in 1..=MAX_TRACE_ITERS {
        let p = from + dir * t;
        let s = level.eval(p);
        let clearance = -s.d - radius;

        // `s.grad` points toward increasing `d`, i.e. toward the wall
        // (this project's convention has `d < 0` inside floor/free space).
        // A positive dot product means `dir` has a component heading the
        // same way — toward/through the wall, not sliding along or
        // pulling away from it. Without this check, a trace that starts
        // already touching a wall (as every pass after the first slide
        // does) would immediately re-report a zero-progress hit even when
        // moving tangentially, permanently freezing the actor the moment
        // it first touched anything.
        let approaching = dir.dot(s.grad) > 0.0;

        if clearance <= CONTACT_EPS && approaching {
            return TraceResult {
                hit: Some(TraceHit {
                    t,
                    point: p,
                    normal: outward_normal(s.grad),
                }),
                iterations,
            };
        }

        let advance = if clearance <= CONTACT_EPS {
            // Already at the surface but not heading into it: a step sized
            // by `clearance` alone would be ~0 and make no progress along
            // a wall being slid against. Creep by up to one radius instead
            // — still re-checked every iteration, so a corner or a second
            // wall is caught, just possibly a touch later than the exact
            // point it starts.
            radius.max(MIN_ADVANCE)
        } else {
            (clearance * TRACE_SAFETY).max(MIN_ADVANCE)
        };
        if t + advance >= dist {
            return TraceResult {
                hit: None,
                iterations,
            };
        }
        t += advance;
    }

    // Iteration cap reached without resolving: treat as a hit here rather
    // than looping forever. Only reachable near a concave crease where the
    // field's conservative underestimate slows convergence (see the design
    // plan) — never past a wall, since the underestimate can't overshoot.
    let p = from + dir * t;
    let s = level.eval(p);
    TraceResult {
        hit: Some(TraceHit {
            t,
            point: p,
            normal: outward_normal(s.grad),
        }),
        iterations: MAX_TRACE_ITERS,
    }
}

/// Pushes `pos` out of any overlap with a wall. Handles a bad frame (spawned
/// inside geometry, or pushed there by an earlier frame) rather than
/// letting it become a permanent stuck state.
fn depenetrate(level: &Level, pos: Vec2, radius: f32) -> Vec2 {
    let mut p = pos;
    for _ in 0..MAX_DEPENETRATE_ITERS {
        let s = level.eval(p);
        let clearance = -s.d - radius;
        if clearance >= 0.0 {
            break;
        }
        p += outward_normal(s.grad) * (-clearance * 1.01);
    }
    p
}

pub struct MoveStats {
    pub max_trace_iterations: u32,
    pub contacts: Vec<Contact>,
}

/// Moves `actor` by `actor.vel * dt` against `level`, sliding along any wall
/// it meets. This is the whole physics step for a single actor — see
/// `physics::World::step` for how it's driven from the fixed-timestep loop.
pub fn move_actor(level: &Level, actor: &mut Actor, dt: f32) -> MoveStats {
    actor.pos = depenetrate(level, actor.pos, actor.radius);

    let mut remaining = actor.vel * dt;
    let mut max_iterations = 0;
    let mut contacts = Vec::new();

    for _ in 0..MAX_SLIDE_PASSES {
        let dist = remaining.length();
        if dist <= MIN_ADVANCE {
            break;
        }
        let dir = remaining / dist;
        let result = sphere_trace(level, actor.pos, dir, dist, actor.radius);
        max_iterations = max_iterations.max(result.iterations);

        match result.hit {
            None => {
                actor.pos += remaining;
                break;
            }
            Some(hit) => {
                actor.pos += dir * hit.t;
                contacts.push(Contact {
                    point: hit.point,
                    normal: hit.normal,
                });

                let leftover = remaining - dir * hit.t;
                let into_wall = leftover.dot(hit.normal).min(0.0);
                remaining = leftover - hit.normal * into_wall;

                let vel_into_wall = actor.vel.dot(hit.normal).min(0.0);
                actor.vel -= hit.normal * vel_into_wall;
            }
        }
    }

    actor.pos = depenetrate(level, actor.pos, actor.radius);

    MoveStats {
        max_trace_iterations: max_iterations,
        contacts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::primitive::{Circle, Primitive, RoundRect, Similarity};

    fn room_level() -> Level {
        let mut level = Level::new(16.0);
        let room = level.csg.leaf(Primitive::RoundRect(RoundRect {
            half_extents: Vec2::new(4.0, 4.0),
            radius: 0.0,
        }));
        level.csg.finalize();
        level.add_part(room);
        level
    }

    #[test]
    fn trace_hits_a_wall() {
        let level = room_level();
        let radius = 0.3;
        let result = sphere_trace(&level, Vec2::ZERO, Vec2::X, 10.0, radius);
        let hit = result.hit.expect("should hit the +x wall");
        // The wall is at x=4; the disk's edge should stop just short of it.
        assert!((hit.point.x - (4.0 - radius)).abs() < 0.01);
        assert!(hit.normal.dot(Vec2::NEG_X) > 0.99, "normal should point back into the room");
    }

    #[test]
    fn slide_along_wall() {
        let level = room_level();
        let mut actor = Actor::new(Vec2::new(3.0, 0.0), 0.3);
        // Aim mostly into the +x wall but with a +y component: the actor
        // should slide along the wall (keep moving in +y) rather than stop
        // dead.
        actor.vel = Vec2::new(10.0, 5.0);
        let stats = move_actor(&level, &mut actor, 1.0);

        assert!(!stats.contacts.is_empty(), "should have contacted the wall");
        assert!(actor.pos.x < 4.0 - 0.3 + 0.01, "must not penetrate the wall");
        assert!(actor.pos.y > 0.5, "should have slid along +y rather than stopping");
    }

    /// Regression test for a real bug this milestone caught: once an actor
    /// is resting flush against a flat wall, its stored velocity is purely
    /// tangential (the perpendicular component was zeroed by the previous
    /// tick's slide). Every subsequent tick then starts its trace already
    /// touching the wall — without the gradient-vs-direction check in
    /// `sphere_trace`, that immediately re-reported a zero-progress hit
    /// forever, freezing the actor solid the instant it first touched
    /// anything. Uses realistic per-tick sizing (small `dt`, many ticks),
    /// not one oversized `dt`, since that's the shape the bug actually
    /// occurs in during real play.
    #[test]
    fn sliding_along_a_wall_keeps_making_progress_across_many_ticks() {
        let level = room_level();
        let mut actor = Actor::new(Vec2::new(3.0, 0.0), 0.3);
        let dt = 1.0 / 120.0;
        actor.vel = Vec2::new(6.0, 3.0);

        for _ in 0..240 {
            actor.vel = Vec2::new(6.0, 3.0);
            move_actor(&level, &mut actor, dt);
        }

        assert!(
            actor.pos.y > 1.0,
            "actor should have kept sliding along +y, stalled at y={}",
            actor.pos.y
        );
        let clearance = -level.eval(actor.pos).d - actor.radius;
        assert!(clearance > -1e-2, "actor penetrated the wall while sliding");
    }

    #[test]
    fn interior_corner_does_not_escape() {
        let level = room_level();
        let mut actor = Actor::new(Vec2::new(3.0, 3.0), 0.3);
        // Straight at the corner: both the +x and +y walls are in play.
        actor.vel = Vec2::new(10.0, 10.0);
        let mut stats = move_actor(&level, &mut actor, 1.0);
        // A few ticks, in case one tick's slide-pass cap isn't enough to
        // fully resolve a corner in one go.
        for _ in 0..5 {
            stats = move_actor(&level, &mut actor, 1.0);
        }

        let clearance = -level.eval(actor.pos).d - actor.radius;
        assert!(clearance > -1e-2, "actor escaped the room at the corner");
        assert!(actor.pos.x <= 4.0 && actor.pos.y <= 4.0, "actor tunnelled past a wall");
        let _ = stats;
    }

    #[test]
    fn no_tunneling_at_high_speed_against_composed_geometry() {
        // A composed (unioned, overlapping) shape, so the field's
        // conservative underestimate near the concave join is actually
        // exercised, not just a single primitive's exact distance.
        let mut level = Level::new(16.0);
        let a = level.csg.leaf(Primitive::RoundRect(RoundRect {
            half_extents: Vec2::new(4.0, 4.0),
            radius: 0.0,
        }));
        let b_leaf = level.csg.leaf(Primitive::Circle(Circle { radius: 3.0 }));
        let b = level
            .csg
            .transform(b_leaf, Similarity::translate(Vec2::new(6.0, 0.0)));
        let root = level.csg.union(vec![a, b]);
        level.csg.finalize();
        level.add_part(root);

        let mut actor = Actor::new(Vec2::new(-3.5, 0.0), 0.3);
        // A huge single-tick velocity: a naive discrete step-and-check
        // would tunnel straight through the far wall.
        actor.vel = Vec2::new(500.0, 0.0);
        move_actor(&level, &mut actor, 1.0);

        let clearance = -level.eval(actor.pos).d - actor.radius;
        assert!(
            clearance > -1e-2,
            "actor tunnelled through composed geometry at high speed: clearance={clearance}"
        );
    }
}

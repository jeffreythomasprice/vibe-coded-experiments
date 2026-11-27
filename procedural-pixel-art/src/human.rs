//! The human: a big head over a small torso, with floating hands and feet.

use crate::creature::{Anim, Facing};
use crate::palette::{Material, Palette};
use crate::pixbuf::PixBuf;
use crate::rng::Rng;
use crate::stamp::Stamps;

pub const SPRITE_WIDTH: i32 = 32;
pub const SPRITE_HEIGHT: i32 = 40;
/// The point between the feet, in sprite coordinates.
pub const ORIGIN_X: i32 = SPRITE_WIDTH / 2;
pub const GROUND_Y: i32 = 36;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Limb {
    pub forward: i32,
    pub lift: i32,
}

/// Offsets from the rest position. Limbs are ordered [left, right].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pose {
    /// How far the upper body is raised.
    pub bob: i32,
    pub hands: [Limb; 2],
    pub feet: [Limb; 2],
}

const IDLE_FRAME_SECS: f64 = 0.6;
const WALK_FRAME_SECS: f64 = 0.13;

/// The pose `t` seconds into an animation.
pub fn pose(anim: Anim, t: f64) -> Pose {
    let frame = |secs: f64, count: i64| (t / secs).floor() as i64 % count;
    let limb = |forward, lift| Limb { forward, lift };
    match anim {
        Anim::Idle => Pose {
            bob: -(frame(IDLE_FRAME_SECS, 2) as i32),
            ..Pose::default()
        },
        Anim::Walk => {
            // Contact, passing, opposite contact, passing.
            let (stride, lifted) = match frame(WALK_FRAME_SECS, 4) {
                0 => (2, None),
                1 => (0, Some(1)),
                2 => (-2, None),
                _ => (0, Some(0)),
            };
            let mut feet = [limb(stride, 0), limb(-stride, 0)];
            if let Some(i) = lifted {
                feet[i].lift = 2;
            }
            Pose {
                bob: i32::from(lifted.is_some()),
                // Hands swing opposite to the foot on the same side.
                hands: [limb(-stride, 0), limb(stride, 0)],
                feet,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Body {
    pub torso_width: i32,
    pub torso_height: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Human {
    pub palette: Palette,
    pub body: Body,
    pub hat: bool,
}

#[derive(Clone, Copy)]
enum Part {
    Foot(usize),
    Torso,
    Head,
    Hand(usize),
}

const HEAD_WIDTH: i32 = 14;
const HEAD_HEIGHT: i32 = 12;
const HAND_SIZE: i32 = 4;
const FOOT_WIDTH: i32 = 4;
const FOOT_HEIGHT: i32 = 3;

impl Human {
    pub fn random(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            palette: Palette::human(&mut rng),
            // Even widths keep the body symmetrical about the centre line.
            body: Body {
                torso_width: rng.pick(&[8, 10, 10, 12]),
                torso_height: rng.pick(&[7, 8, 9]),
            },
            hat: rng.pick(&[true, false]),
        }
    }

    pub fn render(&self, stamps: &Stamps, facing: Facing, pose: Pose) -> PixBuf {
        // (draw order, part). Depth is doubled so ties can be broken between parts.
        let mut parts = vec![(0, Part::Torso), (1, Part::Head)];
        for (i, side) in [(0, -1), (1, 1)] {
            let (_, foot_depth) = facing.project(side * 2, pose.feet[i].forward);
            // Feet sit under the body, so they always go behind the torso.
            parts.push((-100 + foot_depth, Part::Foot(i)));
            let (_, hand_depth) = facing.project(side * 4, pose.hands[i].forward);
            let order = if hand_depth < 0 {
                2 * hand_depth
            } else {
                2 * hand_depth + 2
            };
            parts.push((order, Part::Hand(i)));
        }
        parts.sort_by_key(|&(order, _)| order);

        let mut sprite = PixBuf::new(SPRITE_WIDTH, SPRITE_HEIGHT);
        for (_, part) in parts {
            sprite.composite(&self.layer(stamps, facing, pose, part));
        }
        sprite
    }

    /// One part, shaded and outlined on its own so overlaps stay readable.
    fn layer(&self, stamps: &Stamps, facing: Facing, pose: Pose, part: Part) -> PixBuf {
        let mut layer = PixBuf::new(SPRITE_WIDTH, SPRITE_HEIGHT);
        let Body {
            torso_width,
            torso_height,
        } = self.body;
        // The gap under the torso is where the floating feet show.
        let torso_top = GROUND_Y - 3 - torso_height - pose.bob;
        let side_of = |i: usize| if i == 0 { -1 } else { 1 };

        match part {
            Part::Foot(i) => {
                let limb = pose.feet[i];
                let (x, depth) = facing.project(side_of(i) * (torso_width / 2 - 2), limb.forward);
                // A longer stride in profile keeps the two feet visibly apart.
                let x = if facing.is_side() { x * 3 / 2 } else { x };
                // Seen head-on, a forward step shows as moving down the screen.
                let step = if facing.is_side() { 0 } else { depth / 2 };
                layer.fill_rounded_rect(
                    ORIGIN_X + x - FOOT_WIDTH / 2,
                    GROUND_Y - FOOT_HEIGHT - limb.lift + step,
                    FOOT_WIDTH,
                    FOOT_HEIGHT,
                    Material::Boots,
                );
                layer.shade_pass();
                layer.outline_pass();
            }
            Part::Torso => {
                let width = if facing.is_side() {
                    // Narrower in profile, but still visible around the near hand.
                    (torso_width - 2).max(8)
                } else {
                    torso_width
                };
                layer.fill_rounded_rect(
                    ORIGIN_X - width / 2,
                    torso_top,
                    width,
                    torso_height,
                    Material::Shirt,
                );
                layer.shade_pass();
                layer.outline_pass();
            }
            Part::Hand(i) => {
                let limb = pose.hands[i];
                let (x, depth) = facing.project(side_of(i) * (torso_width / 2 + 2), limb.forward);
                // In profile the arms hang at the middle of the body, not out to the side.
                let (x, step) = if facing.is_side() {
                    (x, 1)
                } else {
                    (x, depth / 2)
                };
                let center_y = torso_top + torso_height / 2 + 1 - limb.lift + step;
                layer.fill_ellipse(
                    ORIGIN_X + x - HAND_SIZE / 2,
                    center_y - HAND_SIZE / 2,
                    HAND_SIZE,
                    HAND_SIZE,
                    Material::Skin,
                );
                layer.shade_pass();
                layer.outline_pass();
            }
            Part::Head => {
                let center_y = torso_top - HEAD_HEIGHT / 2 + 1;
                layer.fill_ellipse(
                    ORIGIN_X - HEAD_WIDTH / 2,
                    center_y - HEAD_HEIGHT / 2,
                    HEAD_WIDTH,
                    HEAD_HEIGHT,
                    Material::Skin,
                );
                layer.shade_pass();

                let flip = facing == Facing::West;
                let face = match facing {
                    Facing::South => Some(&stamps.face_south),
                    Facing::North => None,
                    Facing::East | Facing::West => Some(&stamps.face_side),
                };
                if let Some(face) = face {
                    layer.blit_stamp(face, ORIGIN_X, center_y, flip);
                }
                layer.blit_stamp(stamps.hair.facing(facing), ORIGIN_X, center_y, flip);
                if self.hat {
                    layer.blit_stamp(stamps.hat.facing(facing), ORIGIN_X, center_y, flip);
                }
                layer.outline_pass();
            }
        }
        layer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_cycles_through_four_frames() {
        let at = |frame: f64| pose(Anim::Walk, (frame + 0.5) * WALK_FRAME_SECS);
        assert_eq!(at(0.0), at(4.0));
        assert_eq!(at(0.0).feet[0].forward, -at(2.0).feet[0].forward);
        assert_eq!(at(1.0).feet[1].lift, 2);
        assert_eq!(at(3.0).feet[0].lift, 2);
        assert_eq!(at(0.0).hands[0].forward, -at(0.0).feet[0].forward);
    }

    #[test]
    fn every_facing_stays_inside_the_sprite() {
        let stamps = Stamps::load().unwrap();
        for seed in 0..50 {
            let mut human = Human::random(seed);
            human.hat = true;
            for facing in [Facing::South, Facing::North, Facing::East, Facing::West] {
                for frame in 0..8 {
                    let t = f64::from(frame) * WALK_FRAME_SECS;
                    let sprite = human.render(&stamps, facing, pose(Anim::Walk, t));
                    for x in 0..SPRITE_WIDTH {
                        assert!(sprite.get(x, 0).is_none());
                        assert!(sprite.get(x, SPRITE_HEIGHT - 1).is_none());
                    }
                    for y in 0..SPRITE_HEIGHT {
                        assert!(sprite.get(0, y).is_none());
                        assert!(sprite.get(SPRITE_WIDTH - 1, y).is_none());
                    }
                }
            }
        }
    }
}

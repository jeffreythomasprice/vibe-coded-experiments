//! The hellhound: a horned, snarling quadruped that smoulders. Its body, legs and
//! tail are drawn by code; its head and flames are stamps.

use crate::creature::{Anim, Facing};
use crate::palette::{Material, Palette};
use crate::pixbuf::{PixBuf, Pixel};
use crate::rng::Rng;
use crate::stamp::{Stamp, Stamps};

pub const SPRITE_WIDTH: i32 = 40;
pub const SPRITE_HEIGHT: i32 = 40;
/// The point between the paws, in sprite coordinates.
pub const ORIGIN_X: i32 = SPRITE_WIDTH / 2;
pub const GROUND_Y: i32 = 34;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Paw {
    pub forward: i32,
    pub lift: i32,
}

/// Offsets from the rest position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pose {
    /// How far the body and head are raised.
    pub bob: i32,
    /// How far the head is thrust forward.
    pub lunge: i32,
    /// Ordered front left, front right, rear left, rear right.
    pub paws: [Paw; 4],
    /// Tail tip offset, -1 to 1.
    pub sway: i32,
    /// Counts up steadily; drives the fire.
    pub flicker: usize,
}

const BREATH_FRAME_SECS: f64 = 0.45;
const GROWL_FRAME_SECS: f64 = 0.1;
const TROT_FRAME_SECS: f64 = 0.11;
const SWAY_FRAME_SECS: f64 = 0.25;
const FLICKER_FRAME_SECS: f64 = 0.1;

/// The pose `t` seconds into an animation.
pub fn pose(anim: Anim, t: f64) -> Pose {
    let frame = |secs: f64, count: i64| (t / secs).floor() as i64 % count;
    let sway = [0, 1, 0, -1][frame(SWAY_FRAME_SECS, 4) as usize];
    let flicker = (t / FLICKER_FRAME_SECS).floor() as usize;
    match anim {
        Anim::Idle => {
            // Every so often the head shakes forward in a growl.
            let growl = frame(GROWL_FRAME_SECS, 24);
            Pose {
                bob: -(frame(BREATH_FRAME_SECS, 2) as i32),
                lunge: i32::from(growl >= 16 && growl % 2 == 0),
                sway,
                flicker,
                ..Pose::default()
            }
        }
        Anim::Walk => {
            // A trot: diagonally opposite paws move together.
            let step = frame(TROT_FRAME_SECS, 4);
            let stride = [2, 0, -2, 0][step as usize];
            let paws = [0, 1, 2, 3].map(|i| {
                let leads = i == 0 || i == 3;
                let lifted = if leads { step == 3 } else { step == 1 };
                Paw {
                    forward: if leads { stride } else { -stride },
                    lift: if lifted { 2 } else { 0 },
                }
            });
            Pose {
                bob: i32::from(step % 2 == 1),
                paws,
                sway,
                flicker,
                ..Pose::default()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hound {
    pub palette: Palette,
    /// Depth of the chest in pixels; even.
    pub girth: i32,
    pub fire: bool,
}

#[derive(Clone, Copy)]
enum Part {
    Leg(usize),
    Chest,
    Haunch,
    /// Both at once, for profile views where they form one outline.
    Trunk,
    Tail,
    Head,
    TailFlame,
    BackFlame(usize),
}

// Positions are (right, forward) from the middle of the body; heights are above
// the ground.
const HEAD_FORWARD: i32 = 8;
const HEAD_HEIGHT: i32 = 17;
const CHEST_FORWARD: i32 = 3;
const HAUNCH_FORWARD: i32 = -5;
const CHEST_HEIGHT: i32 = 10;
const FRONT_LEG_FORWARD: i32 = 4;
const REAR_LEG_FORWARD: i32 = -7;
const LEG_WIDTH: i32 = 4;
const LEG_HEIGHT: i32 = 8;
const BACK_FLAMES: [(i32, i32); 2] = [(-2, -3), (2, -7)];
/// Tail segments from rump to tip, as (right, forward, height).
const TAIL: [(i32, i32, i32); 5] = [
    (0, -9, 10),
    (3, -11, 11),
    (6, -12, 13),
    (8, -13, 15),
    (9, -13, 17),
];

impl Hound {
    pub fn random(seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            palette: Palette::hound(&mut rng),
            girth: rng.pick(&[10, 10, 12]),
            fire: true,
        }
    }

    pub fn render(&self, stamps: &Stamps, facing: Facing, pose: Pose) -> PixBuf {
        let depth = |right, forward| facing.project(right, forward).1;

        // (draw order, part). Depth is scaled to leave room for ties.
        let mut parts = Vec::new();
        for i in 0..4 {
            let (right, forward) = self.leg_position(facing, i, pose);
            parts.push((10 * depth(right, forward), Part::Leg(i)));
        }
        let chest = 10 * depth(0, CHEST_FORWARD);
        let haunch = 10 * depth(0, HAUNCH_FORWARD);
        if facing.is_side() {
            parts.push((0, Part::Trunk));
        } else {
            parts.push((chest, Part::Chest));
            parts.push((haunch, Part::Haunch));
        }
        // In profile the head is level with the body but must still cover it.
        let head = if facing.is_side() {
            100
        } else {
            10 * depth(0, HEAD_FORWARD)
        };
        parts.push((head, Part::Head));
        let tail = 10 * depth(0, TAIL[0].1) - 5;
        parts.push((tail, Part::Tail));
        if self.fire {
            parts.push((tail + 1, Part::TailFlame));
            // Flames stand on top of the back, whichever half is nearer.
            for i in 0..BACK_FLAMES.len() {
                parts.push((chest.max(haunch) + 1, Part::BackFlame(i)));
            }
        }
        parts.sort_by_key(|&(order, _)| order);

        let mut sprite = PixBuf::new(SPRITE_WIDTH, SPRITE_HEIGHT);
        for (_, part) in parts {
            sprite.composite(&self.layer(stamps, facing, pose, part));
        }
        sprite
    }

    /// Where leg `i` meets the ground, as (right, forward).
    fn leg_position(&self, facing: Facing, i: usize, pose: Pose) -> (i32, i32) {
        let side = if i.is_multiple_of(2) { -1 } else { 1 };
        let front = i < 2;
        // The rear legs stand wider, so they show past the front ones head-on.
        let spread = if front {
            self.girth / 2 - 1
        } else {
            self.girth / 2 + 2
        };
        let rest = if front {
            FRONT_LEG_FORWARD
        } else {
            REAR_LEG_FORWARD
        };
        // In profile, nudge the far legs forward so all four show.
        let far = facing.is_side() && facing.project(side, 0).1 < 0;
        let nudge = if far { 3 } else { 0 };
        (side * spread, rest + nudge + pose.paws[i].forward)
    }

    /// One part, shaded and outlined on its own so overlaps stay readable.
    fn layer(&self, stamps: &Stamps, facing: Facing, pose: Pose, part: Part) -> PixBuf {
        let mut layer = PixBuf::new(SPRITE_WIDTH, SPRITE_HEIGHT);
        // Screen position of a point on the body.
        let place = |right: i32, forward: i32, height: i32| {
            let (x, depth) = facing.project(right, forward);
            // Seen head-on, nearer things sit lower on the screen.
            let drop = if facing.is_side() { 0 } else { depth / 3 };
            (ORIGIN_X + x, GROUND_Y - height + drop)
        };
        let flip = facing == Facing::West;

        match part {
            Part::Leg(i) => {
                let (right, forward) = self.leg_position(facing, i, pose);
                let (x, ground) = place(right, forward, 0);
                let bottom = ground - pose.paws[i].lift;
                let top = ground - LEG_HEIGHT - pose.bob;
                let left = x - LEG_WIDTH / 2;
                layer.fill_rounded_rect(left, top, LEG_WIDTH, bottom - top, Material::Fur);
                // The paw sticks out toward wherever the hound is facing.
                let claw = Pixel {
                    material: Material::Horn,
                    shade: 3,
                };
                match facing {
                    Facing::East | Facing::West => {
                        let toe = if flip { left - 1 } else { left + LEG_WIDTH };
                        layer.fill_rounded_rect(toe, bottom - 3, 1, 3, Material::Fur);
                        layer.set(toe, bottom - 2, Pixel::OUTLINE);
                        layer.shade_pass();
                        layer.set(toe, bottom - 1, claw);
                    }
                    Facing::South => {
                        layer.shade_pass();
                        layer.set(left + 1, bottom - 1, claw);
                        layer.set(left + LEG_WIDTH - 1, bottom - 1, claw);
                    }
                    Facing::North => layer.shade_pass(),
                }
                if facing.is_side() && facing.project(right, 0).1 < 0 {
                    layer.darken();
                }
                layer.outline_pass();
            }
            Part::Chest | Part::Haunch | Part::Trunk => {
                let lift = CHEST_HEIGHT + pose.bob;
                if facing.is_side() {
                    let (cx, cy) = place(0, CHEST_FORWARD, lift);
                    let (hx, _) = place(0, HAUNCH_FORWARD, lift);
                    let height = self.girth;
                    layer.fill_ellipse(cx - 6, cy - height / 2, 12, height, Material::Fur);
                    layer.fill_ellipse(hx - 5, cy - height / 2 + 1, 10, height - 1, Material::Fur);
                    // The belly joining the two.
                    let belly = (cx - hx).abs();
                    layer.fill_rounded_rect(
                        cx.min(hx),
                        cy - height / 2 + 2,
                        belly,
                        height - 3,
                        Material::Fur,
                    );
                } else {
                    let (forward, width, height) = if matches!(part, Part::Chest) {
                        (CHEST_FORWARD, self.girth + 4, self.girth)
                    } else {
                        (HAUNCH_FORWARD, self.girth + 6, self.girth)
                    };
                    let (x, y) = place(0, forward, lift);
                    layer.fill_ellipse(x - width / 2, y - height / 2, width, height, Material::Fur);
                }
                layer.shade_pass();
                layer.outline_pass();
            }
            Part::Tail => {
                for i in 0..TAIL.len() {
                    let (x, y) = self.tail_segment(facing, pose, i);
                    // Thinner toward the tip.
                    let size = if i < 2 { 3 } else { 2 };
                    layer.fill_ellipse(x - 1, y - 1, size, size, Material::Fur);
                }
                layer.shade_pass();
                layer.outline_pass();
            }
            Part::Head => {
                let (x, y) = place(0, HEAD_FORWARD + pose.lunge, HEAD_HEIGHT + pose.bob);
                layer.blit_stamp(stamps.hound_head.facing(facing), x, y, flip);
                layer.outline_pass();
            }
            // Flames glow, so they get neither shading nor a dark outline.
            Part::TailFlame => {
                let (x, y) = self.tail_segment(facing, pose, TAIL.len() - 1);
                let stamp = flame(&stamps.flame_large, pose.flicker);
                layer.blit_stamp(stamp, x, y, flip);
                // An ember drifts up and goes out.
                let age = (pose.flicker % 8) as i32;
                if age < 4 {
                    let drift = if pose.flicker % 16 < 8 { -2 } else { 1 };
                    layer.set(
                        x + drift,
                        y - 8 - age,
                        Pixel {
                            material: Material::Fire,
                            shade: 3 - (age / 2) as u8,
                        },
                    );
                }
            }
            Part::BackFlame(i) => {
                let (right, forward) = BACK_FLAMES[i];
                let top = CHEST_HEIGHT + self.girth / 2 - 1 + pose.bob;
                let (x, y) = place(right, forward, top);
                // Out of step with each other and with the tail.
                let stamp = flame(&stamps.flame_small, pose.flicker + i + 1);
                layer.blit_stamp(stamp, x, y, flip);
            }
        }
        layer
    }

    /// Screen position of tail segment `i`. The sway grows toward the tip.
    fn tail_segment(&self, facing: Facing, pose: Pose, i: usize) -> (i32, i32) {
        let (right, forward, height) = TAIL[i];
        let sway = if i < 2 { 0 } else { pose.sway };
        let (x, depth) = facing.project(right + sway, forward + sway);
        let drop = if facing.is_side() { 0 } else { depth / 3 };
        (ORIGIN_X + x, GROUND_Y - height - pose.bob + drop)
    }
}

fn flame(frames: &[Stamp; 3], tick: usize) -> &Stamp {
    &frames[tick % frames.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trot_moves_diagonal_paws_together() {
        let at = |frame: f64| pose(Anim::Walk, (frame + 0.5) * TROT_FRAME_SECS);
        for frame in 0..4 {
            let paws = at(f64::from(frame)).paws;
            assert_eq!(paws[0], paws[3]);
            assert_eq!(paws[1], paws[2]);
        }
        assert_eq!(at(0.0).paws[0].forward, -at(0.0).paws[1].forward);
        assert_eq!(at(1.0).paws[1].lift, 2);
        assert_eq!(at(3.0).paws[0].lift, 2);
    }

    #[test]
    fn fire_can_be_put_out() {
        let stamps = Stamps::load().unwrap();
        let mut hound = Hound::random(1);
        let burning = |hound: &Hound| {
            let sprite = hound.render(&stamps, Facing::East, Pose::default());
            (0..SPRITE_HEIGHT).any(|y| {
                (0..SPRITE_WIDTH).any(|x| {
                    sprite
                        .get(x, y)
                        .is_some_and(|p| p.material == Material::Fire)
                })
            })
        };
        assert!(burning(&hound));
        hound.fire = false;
        assert!(!burning(&hound));
    }
}

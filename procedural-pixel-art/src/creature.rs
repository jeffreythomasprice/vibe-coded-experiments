//! What every kind of creature shares, and dispatch between the kinds.
//!
//! A creature is a few parts hung on anchors in its own 3D space. Facing rotates
//! the anchors, and the projected depth decides what is drawn in front.

use crate::hound::{self, Hound};
use crate::human::{self, Human};
use crate::palette::Palette;
use crate::pixbuf::PixBuf;
use crate::stamp::Stamps;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    South,
    North,
    East,
    West,
}

impl Facing {
    /// Projects a body-relative position into (screen x, depth toward the viewer).
    pub fn project(self, right: i32, forward: i32) -> (i32, i32) {
        match self {
            Facing::South => (-right, forward),
            Facing::North => (right, -forward),
            Facing::East => (forward, right),
            Facing::West => (-forward, -right),
        }
    }

    pub fn is_side(self) -> bool {
        matches!(self, Facing::East | Facing::West)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anim {
    Idle,
    Walk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Human,
    Hound,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Human, Kind::Hound];
}

/// How far a creature extends from the point between its feet.
#[derive(Clone, Copy, Debug)]
pub struct Reach {
    pub side: i32,
    pub above: i32,
    pub below: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Creature {
    Human(Human),
    Hound(Hound),
}

impl Creature {
    pub fn random(kind: Kind, seed: u64) -> Self {
        match kind {
            Kind::Human => Creature::Human(Human::random(seed)),
            Kind::Hound => Creature::Hound(Hound::random(seed)),
        }
    }

    /// The sprite `t` seconds into an animation.
    pub fn render(&self, stamps: &Stamps, facing: Facing, anim: Anim, t: f64) -> PixBuf {
        match self {
            Creature::Human(human) => human.render(stamps, facing, human::pose(anim, t)),
            Creature::Hound(hound) => hound.render(stamps, facing, hound::pose(anim, t)),
        }
    }

    pub fn palette(&self) -> &Palette {
        match self {
            Creature::Human(human) => &human.palette,
            Creature::Hound(hound) => &hound.palette,
        }
    }

    /// Switches the optional flourish: the human's hat or the hound's fire.
    pub fn toggle_extra(&mut self) {
        match self {
            Creature::Human(human) => human.hat = !human.hat,
            Creature::Hound(hound) => hound.fire = !hound.fire,
        }
    }

    /// The point between the feet, in sprite coordinates.
    pub fn origin(&self) -> (i32, i32) {
        match self {
            Creature::Human(_) => (human::ORIGIN_X, human::GROUND_Y),
            Creature::Hound(_) => (hound::ORIGIN_X, hound::GROUND_Y),
        }
    }

    pub fn reach(&self) -> Reach {
        let (side, above, below) = match self {
            Creature::Human(_) => (14, 32, 5),
            Creature::Hound(_) => (19, 33, 6),
        };
        Reach { side, above, below }
    }

    /// The ground shadow as (row offset, half width).
    pub fn shadow(&self, facing: Facing) -> &'static [(i32, i32)] {
        match self {
            Creature::Human(_) => &[(-2, 5), (-1, 7), (0, 7), (1, 5)],
            Creature::Hound(_) if facing.is_side() => &[(-2, 10), (-1, 13), (0, 13), (1, 10)],
            Creature::Hound(_) => &[(-3, 6), (-2, 8), (-1, 9), (0, 9), (1, 8), (2, 6)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_stays_inside_its_sprite_and_reach() {
        let stamps = Stamps::load().unwrap();
        for kind in Kind::ALL {
            for seed in 0..20 {
                let creature = Creature::random(kind, seed);
                let (origin_x, ground_y) = creature.origin();
                let reach = creature.reach();
                for facing in [Facing::South, Facing::North, Facing::East, Facing::West] {
                    for anim in [Anim::Idle, Anim::Walk] {
                        // Long enough to cover every walk, growl and flame frame.
                        for tick in 0..50 {
                            let t = f64::from(tick) * 0.05;
                            let sprite = creature.render(&stamps, facing, anim, t);
                            for y in 0..sprite.height {
                                for x in 0..sprite.width {
                                    if sprite.get(x, y).is_none() {
                                        continue;
                                    }
                                    let at = format!("{kind:?} {seed} {facing:?} {t}: {x},{y}");
                                    assert!(x > 0 && x < sprite.width - 1, "{at}");
                                    assert!(y > 0 && y < sprite.height - 1, "{at}");
                                    assert!((x - origin_x).abs() <= reach.side, "{at}");
                                    assert!(ground_y - y <= reach.above, "{at}");
                                    assert!(y - ground_y < reach.below, "{at}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

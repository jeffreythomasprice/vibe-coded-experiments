use serde::{Deserialize, Serialize};

use crate::character::{Character, HealthDamage};
use crate::character::traits::{AbilityKind, AttributeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthLevelKind {
    Wound,
    Incap,
    Dying,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthLevel {
    pub penalty: i8,
    pub label: &'static str,
    pub kind: HealthLevelKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OxBodyPattern {
    OneZero,
    TwoMinusOne,
    OneMinusOneTwoMinusTwo,
}

impl Default for OxBodyPattern {
    fn default() -> Self {
        OxBodyPattern::OneMinusOneTwoMinusTwo
    }
}

const BASE_TRACK: &[HealthLevel] = &[
    HealthLevel { penalty: 0, label: "-0", kind: HealthLevelKind::Wound },
    HealthLevel { penalty: -1, label: "-1", kind: HealthLevelKind::Wound },
    HealthLevel { penalty: -1, label: "-1", kind: HealthLevelKind::Wound },
    HealthLevel { penalty: -2, label: "-2", kind: HealthLevelKind::Wound },
    HealthLevel { penalty: -2, label: "-2", kind: HealthLevelKind::Wound },
    HealthLevel { penalty: -4, label: "-4", kind: HealthLevelKind::Wound },
    HealthLevel { penalty: 0, label: "Incap", kind: HealthLevelKind::Incap },
];

/// Walk Ox-Body Technique purchases in order, returning each purchase's
/// chosen pattern (defaulting when unspecified). Capped at the character's
/// Resistance rating per the rules.
fn ox_body_patterns(character: &Character) -> Vec<OxBodyPattern> {
    let resistance = character.ability(AbilityKind::Resistance) as usize;
    character
        .charms
        .iter()
        .filter(|c| c.name.eq_ignore_ascii_case("Ox-Body Technique"))
        .map(|c| c.ox_body_pattern.unwrap_or_default())
        .take(resistance)
        .collect()
}

/// Compute the character's full health track: base 7 wound levels, plus
/// Ox-Body levels per purchase, the Incap row, and Stamina dying rows
/// after Incap.
pub fn health_track(character: &Character) -> Vec<HealthLevel> {
    let mut track: Vec<HealthLevel> = BASE_TRACK.to_vec();
    for pattern in ox_body_patterns(character) {
        let to_add: &[HealthLevel] = match pattern {
            OxBodyPattern::OneZero => &[HealthLevel {
                penalty: 0, label: "-0", kind: HealthLevelKind::Wound,
            }],
            OxBodyPattern::TwoMinusOne => &[
                HealthLevel { penalty: -1, label: "-1", kind: HealthLevelKind::Wound },
                HealthLevel { penalty: -1, label: "-1", kind: HealthLevelKind::Wound },
            ],
            OxBodyPattern::OneMinusOneTwoMinusTwo => &[
                HealthLevel { penalty: -1, label: "-1", kind: HealthLevelKind::Wound },
                HealthLevel { penalty: -2, label: "-2", kind: HealthLevelKind::Wound },
                HealthLevel { penalty: -2, label: "-2", kind: HealthLevelKind::Wound },
            ],
        };
        for level in to_add {
            let pos = wound_insert_position(&track, level.penalty);
            track.insert(pos, *level);
        }
    }
    let dying = character.attribute(AttributeKind::Stamina) as usize;
    for _ in 0..dying {
        track.push(HealthLevel {
            penalty: 0, label: "Dying", kind: HealthLevelKind::Dying,
        });
    }
    track
}

/// Find the index at which a new wound level of `penalty` should be
/// inserted: right after the last wound level whose penalty is at least
/// as light (i.e. greater than or equal as a signed value: 0 > -1 > -2 >
/// -4) and before any heavier wound level, Incap, or Dying row.
fn wound_insert_position(track: &[HealthLevel], penalty: i8) -> usize {
    track
        .iter()
        .position(|l| l.kind != HealthLevelKind::Wound || l.penalty < penalty)
        .unwrap_or(track.len())
}

/// Wound penalty: the deepest filled `Wound`-kind level. Incap and Dying
/// rows don't contribute a wound penalty (Incap means "Inactive only",
/// Dying starts the death countdown).
pub fn wound_penalty(character: &Character) -> i8 {
    let total = total_damage(&character.pool_state.health_damage) as usize;
    if total == 0 {
        return 0;
    }
    let track = health_track(character);
    let idx = (total - 1).min(track.len() - 1);
    for slot in track[..=idx].iter().rev() {
        if slot.kind == HealthLevelKind::Wound {
            return slot.penalty;
        }
    }
    0
}

/// Index of the Incap row in the character's health track.
pub fn incap_index(character: &Character) -> usize {
    health_track(character)
        .iter()
        .position(|l| l.kind == HealthLevelKind::Incap)
        .expect("health track always contains an Incap row")
}

/// True iff the character has taken at least enough damage to fill the
/// Incap row.
pub fn is_incapacitated(character: &Character) -> bool {
    let total = total_damage(&character.pool_state.health_damage) as usize;
    total > incap_index(character)
}

fn total_damage(d: &HealthDamage) -> u8 {
    d.bashing + d.lethal + d.aggravated
}

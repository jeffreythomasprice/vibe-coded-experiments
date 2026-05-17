use serde::{Deserialize, Serialize};

use crate::character::{Character, HealthDamage};
use crate::character::traits::AbilityKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthLevel {
    pub penalty: i8,
    pub label: &'static str,
}

const BASE_TRACK: &[HealthLevel] = &[
    HealthLevel { penalty: 0, label: "-0" },
    HealthLevel { penalty: -1, label: "-1" },
    HealthLevel { penalty: -1, label: "-1" },
    HealthLevel { penalty: -2, label: "-2" },
    HealthLevel { penalty: -2, label: "-2" },
    HealthLevel { penalty: -4, label: "-4" },
    HealthLevel { penalty: -4, label: "Incap" },
];

/// Count of Ox-Body Technique purchases held by the character (matched by
/// charm name).
fn ox_body_count(character: &Character) -> usize {
    character
        .charms
        .iter()
        .filter(|c| c.name.eq_ignore_ascii_case("Ox-Body Technique"))
        .count()
}

/// Compute the character's full health track. Base 7 levels plus one
/// `-0` and two `-2` per Ox-Body purchase (the conservative pattern; the
/// rulebook lets the player pick among three patterns). Cap at the
/// character's Resistance rating per the rules.
pub fn health_track(character: &Character) -> Vec<HealthLevel> {
    let mut track: Vec<HealthLevel> = BASE_TRACK.to_vec();
    let resistance = character.ability(AbilityKind::Resistance) as usize;
    let purchases = ox_body_count(character).min(resistance);
    for _ in 0..purchases {
        // Insert one -0 just before the existing -1 levels (i.e. at index 1)
        // and two -2 levels before Incap.
        track.insert(1, HealthLevel { penalty: 0, label: "-0" });
        let incap_idx = track.len() - 1;
        track.insert(
            incap_idx,
            HealthLevel { penalty: -2, label: "-2" },
        );
        track.insert(
            incap_idx,
            HealthLevel { penalty: -2, label: "-2" },
        );
    }
    track
}

/// Wound penalty: the tier of the deepest filled health level. Returns 0
/// for no damage, or a negative number for the penalty (e.g. -2).
pub fn wound_penalty(character: &Character) -> i8 {
    let total = total_damage(&character.pool_state.health_damage) as usize;
    if total == 0 {
        return 0;
    }
    let track = health_track(character);
    let idx = (total - 1).min(track.len() - 1);
    track[idx].penalty
}

fn total_damage(d: &HealthDamage) -> u8 {
    d.bashing + d.lethal + d.aggravated
}

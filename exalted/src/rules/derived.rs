//! Pure derivations from a character's existing traits — no new state.
//!
//! Covers the chunks of the Voidstate sheet that aren't yet modelled:
//! movement (Move/Dash/Jump), the knockdown/stunning thresholds, and the
//! Exalted healing-rate table. Sources cited inline; see `game_rules.md`
//! §4-§6.

use serde::{Deserialize, Serialize};

use crate::character::Character;
use crate::character::traits::{AbilityKind, AttributeKind};
use crate::rules::health::wound_penalty;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Movement {
    /// Reflexive movement rate, yards per tick.
    pub move_: u8,
    /// Dash rate, yards per tick. Speed 3 / -2 DV / cannot parry.
    pub dash: u8,
    /// Jump distance straight up, yards.
    pub jump_vertical: u8,
    /// Horizontal jump distance, yards. Always 2× vertical.
    pub jump_horizontal: u8,
}

fn armor_mobility_penalty(c: &Character) -> u8 {
    c.equipment.armor.as_ref().map(|a| a.mobility_penalty).unwrap_or(0)
}

/// Per-tick Move/Dash and per-jump vertical/horizontal yardage
/// (`game_rules.md` §5, pp.127-128, 144-145).
pub fn movement(c: &Character) -> Movement {
    let dex = c.attribute(AttributeKind::Dexterity);
    let str_ = c.attribute(AttributeKind::Strength);
    let ath = c.ability(AbilityKind::Athletics);
    let wound = wound_penalty(c).unsigned_abs();
    let mob = armor_mobility_penalty(c);

    // Move = max(Dex - wound - mob, 1).
    let move_ = (dex as i16 - wound as i16 - mob as i16).max(1) as u8;
    // Dash = max(Dex + 6 - wound - mob, 2).
    let dash = (dex as i16 + 6 - wound as i16 - mob as i16).max(2) as u8;
    // Jump vertical = max(Str + Athletics - wound - mob, 0).
    let jump_vertical =
        (str_ as i16 + ath as i16 - wound as i16 - mob as i16).max(0) as u8;
    let jump_horizontal = jump_vertical.saturating_mul(2);

    Movement {
        move_,
        dash,
        jump_vertical,
        jump_horizontal,
    }
}

/// Feats-of-strength lift table from `game_rules.md` §5.1 (p.128).
/// Indexed by `(Strength + Athletics)`; entries are max lift in pounds.
/// Index 0 is "no exertion possible"; the chart caps at index 20.
const LIFT_TABLE_LBS: [u32; 21] = [
    0, 80, 160, 250, 350, 450, 550, 650, 800, 1000, 1200, 1400, 1600, 1800, 2000, 2200, 2500, 3000,
    3500, 4000, 4500,
];

/// Base maximum weight a character can lift as a feat of strength, in pounds
/// (`game_rules.md` §5.1, p.128). Looked up by `(Strength + Athletics)`
/// against the published chart; values above 20 clamp to the chart's top.
///
/// Specialties, channeled Virtues, stunt dice, Willpower successes, and
/// cooperation all add directly to the `(Str + Ath)` total at use time —
/// they are not folded into the sheet value.
pub fn lift_lbs(c: &Character) -> u32 {
    let str_ = c.attribute(AttributeKind::Strength) as usize;
    let ath = c.ability(AbilityKind::Athletics) as usize;
    let idx = (str_ + ath).min(LIFT_TABLE_LBS.len() - 1);
    LIFT_TABLE_LBS[idx]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knockdown {
    /// Raw damage strictly greater than this triggers a knockdown check
    /// (`game_rules.md` §4.1, p.151).
    pub threshold: u8,
    /// Dice pool for the resisting roll (difficulty 2). The rules let the
    /// player pick the higher of `Dex + Athletics` or `Sta + Resistance`;
    /// we return the better of the two.
    pub resist_pool: u8,
}

pub fn knockdown(c: &Character) -> Knockdown {
    let sta = c.attribute(AttributeKind::Stamina);
    let dex = c.attribute(AttributeKind::Dexterity);
    let res = c.ability(AbilityKind::Resistance);
    let ath = c.ability(AbilityKind::Athletics);
    Knockdown {
        threshold: sta.saturating_add(res),
        resist_pool: dex.saturating_add(ath).max(sta.saturating_add(res)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stunning {
    /// HLs from a single blow strictly greater than this triggers a
    /// stunning check (`game_rules.md` §4.2, p.151).
    pub threshold: u8,
    /// Dice pool for the resisting roll. Difficulty = (HLs of damage − Sta).
    pub resist_pool: u8,
}

pub fn stunning(c: &Character) -> Stunning {
    let sta = c.attribute(AttributeKind::Stamina);
    let res = c.ability(AbilityKind::Resistance);
    Stunning {
        threshold: sta,
        resist_pool: sta.saturating_add(res),
    }
}

/// Healing-rate descriptors for an Exalted character at the given health
/// tier (`game_rules.md` §6.1, pp.150-151). Strings are fixed table entries;
/// returning `&'static str` keeps things zero-cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealingRow {
    pub tier: &'static str,
    pub bashing: &'static str,
    pub lethal_rest: &'static str,
    pub lethal_active: &'static str,
}

/// Static table of Exalt healing rates. Aggravated heals at the lethal rate
/// (p.151), so we omit a separate row.
pub const EXALT_HEALING_TABLE: &[HealingRow] = &[
    HealingRow {
        tier: "-0",
        bashing: "1 HL / 3 hours",
        lethal_rest: "6 hours",
        lethal_active: "12 hours",
    },
    HealingRow {
        tier: "-1",
        bashing: "1 HL / 3 hours",
        lethal_rest: "2 days",
        lethal_active: "4 days",
    },
    HealingRow {
        tier: "-2",
        bashing: "1 HL / 3 hours",
        lethal_rest: "4 days",
        lethal_active: "8 days",
    },
    HealingRow {
        tier: "-4 / Incap",
        bashing: "1 HL / 3 hours",
        lethal_rest: "1 week",
        lethal_active: "2 weeks",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{Caste, Character};

    fn unencumbered_dex3() -> Character {
        let mut c = Character::new_blank_solar("Movement Test", Caste::Dawn);
        // Dex starts at 1; bump to 3 via chargen for the formulas below.
        let dex = c.attributes.get_mut(&AttributeKind::Dexterity).unwrap();
        dex.add_chargen().add_chargen();
        c
    }

    #[test]
    fn movement_dex_three_unencumbered() {
        let c = unencumbered_dex3();
        let m = movement(&c);
        assert_eq!(m.move_, 3);
        assert_eq!(m.dash, 9);
    }

    #[test]
    fn movement_floor_holds_under_heavy_armor() {
        let mut c = unencumbered_dex3();
        c.equipment.armor = Some(crate::character::Armor {
            name: "Test".into(),
            soak_bashing: 0,
            soak_lethal: 0,
            soak_aggravated: 0,
            hardness_bashing: 0,
            hardness_lethal: 0,
            mobility_penalty: 10,
            fatigue: 0,
            attunement_motes: None,
            artifact_name: None,
        });
        let m = movement(&c);
        assert_eq!(m.move_, 1, "Move floors at 1");
        assert_eq!(m.dash, 2, "Dash floors at 2");
    }

    #[test]
    fn lift_table_chargen_solar() {
        // Blank solar: Str 1, Athletics 0 → index 1 → 80 lbs.
        let c = Character::new_blank_solar("Lift Test", Caste::Dawn);
        assert_eq!(lift_lbs(&c), 80);
    }

    #[test]
    fn lift_table_mid_chart() {
        // Str 3 + Athletics 2 = 5 → 450 lbs.
        let mut c = Character::new_blank_solar("Lift Test", Caste::Dawn);
        let str_ = c.attributes.get_mut(&AttributeKind::Strength).unwrap();
        str_.add_chargen().add_chargen(); // Str 3
        let ath = c.abilities.get_mut(&AbilityKind::Athletics).unwrap();
        ath.add_chargen().add_chargen(); // Athletics 2
        assert_eq!(lift_lbs(&c), 450);
    }

    #[test]
    fn lift_table_clamps_above_chart() {
        // Force (Str + Ath) well above 20 — should clamp to the top entry.
        let mut c = Character::new_blank_solar("Lift Test", Caste::Dawn);
        let str_ = c.attributes.get_mut(&AttributeKind::Strength).unwrap();
        for _ in 0..20 {
            str_.add_chargen();
        }
        let ath = c.abilities.get_mut(&AbilityKind::Athletics).unwrap();
        for _ in 0..10 {
            ath.add_chargen();
        }
        assert_eq!(lift_lbs(&c), 4500);
    }

    #[test]
    fn knockdown_threshold_is_sta_plus_resistance() {
        let mut c = unencumbered_dex3();
        let sta = c.attributes.get_mut(&AttributeKind::Stamina).unwrap();
        sta.add_chargen(); // Sta 2
        let res = c.abilities.get_mut(&AbilityKind::Resistance).unwrap();
        res.add_chargen().add_chargen(); // Resistance 2
        let k = knockdown(&c);
        assert_eq!(k.threshold, 4);
    }

    #[test]
    fn stunning_threshold_is_sta() {
        let mut c = unencumbered_dex3();
        let sta = c.attributes.get_mut(&AttributeKind::Stamina).unwrap();
        sta.add_chargen().add_chargen(); // Sta 3
        let s = stunning(&c);
        assert_eq!(s.threshold, 3);
    }
}

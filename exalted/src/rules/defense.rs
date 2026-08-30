use crate::character::{AbilityKind, AttributeKind, Character, VirtueKind, Weapon};
use crate::rules::dice::specialty_bonus;

/// Armor's mobility penalty, applied against Dodge DV only (not Parry DV;
/// `game_rules.md` §2.2).
pub(crate) fn mobility_penalty(character: &Character) -> u8 {
    character
        .equipment
        .armor
        .as_ref()
        .map(|a| a.mobility_penalty)
        .unwrap_or(0)
}

/// Dodge DV: ⌊(Dex + Dodge + Essence) / 2⌋, minus armor mobility penalty.
///
/// The rulebook has Exalted round up and mortals round down here; this
/// codebase uses floor uniformly as a house rule (see `game_rules.md` §2.2).
pub fn dodge_dv(character: &Character) -> u8 {
    let raw = character.attribute(AttributeKind::Dexterity) as u16
        + character.ability(AbilityKind::Dodge) as u16
        + character.essence_dots() as u16;
    let rounded = raw / 2;
    rounded.saturating_sub(mobility_penalty(character) as u16) as u8
}

/// Parry DV with a given weapon: ⌊(Dex + weapon's Ability + weapon Defense) / 2⌋.
/// Uses the weapon's `ability` field (Melee, Martial Arts, etc.). Weapon
/// `defense` can be negative (unwieldy weapons), so the halving must floor
/// toward negative infinity, not truncate toward zero — hence `div_euclid`.
/// Clamped to 0 only at the end; not subject to the armor mobility penalty.
pub fn parry_dv(character: &Character, weapon: &Weapon) -> u8 {
    let dex = character.attribute(AttributeKind::Dexterity) as i16;
    let abil = character.ability(weapon.ability) as i16;
    let raw = dex + abil + weapon.defense as i16;
    raw.div_euclid(2).max(0) as u8
}

/// Picks the character's best parry weapon: the highest Parry DV among
/// weapons whose Ability is Melee or Martial Arts (per the rulebook, "almost
/// invariably Martial Arts or Melee" — p.147). Excludes ranged weapons
/// (Archery/Thrown), which cannot be used to parry. Returns `None` if the
/// character carries no such weapon; natural attacks (fists, kicks) aren't
/// modelled.
pub fn best_parry_weapon(character: &Character) -> Option<&Weapon> {
    character
        .equipment
        .weapons
        .iter()
        .filter(|w| matches!(w.ability, AbilityKind::Melee | AbilityKind::MartialArts))
        .max_by_key(|w| parry_dv(character, w))
}

/// Mental Dodge DV: ⌊(Willpower + Integrity + pertinent specialty + Essence) / 2⌋.
pub fn mdv_dodge(character: &Character, specialty: Option<&str>) -> u8 {
    let spec = specialty_bonus(character, AbilityKind::Integrity, specialty);
    let raw = character.willpower_dots() as u16
        + character.ability(AbilityKind::Integrity) as u16
        + spec as u16
        + character.essence_dots() as u16;
    (raw / 2) as u8
}

/// Mental Parry DV: ⌊((Cha or Manip) + Ability + pertinent specialty) / 2⌋.
/// Caller chooses the attribute and the social ability used
/// (Performance/Presence/Investigation).
pub fn mdv_parry(
    character: &Character,
    attribute: AttributeKind,
    ability: AbilityKind,
    specialty: Option<&str>,
) -> u8 {
    let spec = specialty_bonus(character, ability, specialty);
    let raw =
        character.attribute(attribute) as u16 + character.ability(ability) as u16 + spec as u16;
    (raw / 2) as u8
}

/// Bashing soak: Stamina + armor's bashing soak.
pub fn soak_bashing(character: &Character) -> u8 {
    let natural = character.attribute(AttributeKind::Stamina);
    let armor = character
        .equipment
        .armor
        .as_ref()
        .map(|a| a.soak_bashing)
        .unwrap_or(0);
    natural.saturating_add(armor)
}

/// Lethal soak: ⌊Stamina/2⌋ (Exalts) + armor's lethal soak.
pub fn soak_lethal(character: &Character) -> u8 {
    let natural = character.attribute(AttributeKind::Stamina) / 2;
    let armor = character
        .equipment
        .armor
        .as_ref()
        .map(|a| a.soak_lethal)
        .unwrap_or(0);
    natural.saturating_add(armor)
}

/// Aggravated soak: 0 natural + armor's aggravated soak.
pub fn soak_aggravated(character: &Character) -> u8 {
    character
        .equipment
        .armor
        .as_ref()
        .map(|a| a.soak_aggravated)
        .unwrap_or(0)
}

/// Join Battle: Wits + Awareness.
pub fn join_battle(character: &Character) -> u8 {
    character.attribute(AttributeKind::Wits) + character.ability(AbilityKind::Awareness)
}

/// Compute Willpower base (sum of two highest Virtues) — does not include
/// bonus-point or XP purchases.
pub fn willpower_from_virtues(character: &Character) -> u8 {
    let mut virtues: Vec<u8> = VirtueKind::ALL
        .iter()
        .map(|v| character.virtue(*v))
        .collect();
    virtues.sort_unstable_by(|a, b| b.cmp(a));
    virtues.iter().take(2).sum()
}

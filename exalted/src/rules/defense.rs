use crate::character::{AbilityKind, AttributeKind, Character, VirtueKind, Weapon};

fn mobility_penalty(character: &Character) -> u8 {
    character
        .equipment
        .armor
        .as_ref()
        .map(|a| a.mobility_penalty)
        .unwrap_or(0)
}

/// Solar Dodge DV: ⌈(Dex + Dodge + Essence) / 2⌉ + 2, minus mobility penalty.
pub fn dodge_dv(character: &Character) -> u8 {
    let raw = character.attribute(AttributeKind::Dexterity) as u16
        + character.ability(AbilityKind::Dodge) as u16
        + character.essence_dots() as u16;
    let rounded = (raw + 1) / 2;
    let with_bonus = rounded.saturating_add(2);
    with_bonus.saturating_sub(mobility_penalty(character) as u16) as u8
}

/// Parry DV with a given weapon. Uses the weapon's `ability` field (Melee,
/// Martial Arts, etc.). Solar rounding (up).
pub fn parry_dv(character: &Character, weapon: &Weapon) -> u8 {
    let dex = character.attribute(AttributeKind::Dexterity) as i16;
    let abil = character.ability(weapon.ability) as i16;
    let raw = dex + abil + weapon.defense as i16;
    let rounded = ((raw + 1) / 2).max(0) as u16;
    let with_bonus = rounded.saturating_add(2);
    with_bonus.saturating_sub(mobility_penalty(character) as u16) as u8
}

/// Mental Dodge DV: ⌊(Willpower + Integrity + Essence) / 2⌋. No +2.
pub fn mdv_dodge(character: &Character) -> u8 {
    let raw = character.willpower_dots() as u16
        + character.ability(AbilityKind::Integrity) as u16
        + character.essence_dots() as u16;
    (raw / 2) as u8
}

/// Mental Parry DV: ⌊((Cha or Manip) + Ability) / 2⌋ + 2. Caller chooses
/// the attribute and the social ability used (Performance/Presence/Investigation).
pub fn mdv_parry(character: &Character, attribute: AttributeKind, ability: AbilityKind) -> u8 {
    let raw = character.attribute(attribute) as u16 + character.ability(ability) as u16;
    let rounded = raw / 2;
    rounded.saturating_add(2) as u8
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

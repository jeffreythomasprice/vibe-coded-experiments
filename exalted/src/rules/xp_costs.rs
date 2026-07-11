//! Post-chargen experience-point costs. Source: Exalted 2E core rulebook,
//! pp. 275–276, "Experience Costs" table.
//!
//! Two cells in the table were OCR-corrupted in the copy used for reference:
//! - Specialty cost printed as "je". The conventional 2E value is **3 XP**;
//!   encoded here as `xp_cost_specialty`.
//! - Non-Solar Charm cost (Eclipse only) printed as "Ai pectit". Encoded
//!   here as `NON_SOLAR_CHARM_XP_COST` = 20 (double the out-of-Caste Solar
//!   charm cost); revisit against a clean PDF.

/// Cost in XP to raise an Attribute to `new_rating`. Rulebook: rating × 4.
pub fn xp_cost_attribute_increase(new_rating: u8) -> u32 {
    new_rating as u32 * 4
}

/// Cost in XP to raise an Ability to `new_rating`. Favored/Caste: (r × 2) − 1.
/// Out-of-Caste: r × 2.
pub fn xp_cost_ability_increase(new_rating: u8, favored_or_caste: bool) -> u32 {
    if favored_or_caste {
        new_rating as u32 * 2 - 1
    } else {
        new_rating as u32 * 2
    }
}

/// Cost to buy a brand-new Ability at rating 1 (from 0). Flat 3 XP.
pub fn xp_cost_new_ability() -> u32 {
    3
}

/// Cost to buy a new specialty. Rulebook OCR was "je" — conventional 2E
/// value is 3 XP.
pub fn xp_cost_specialty() -> u32 {
    3
}

/// Cost to raise Virtue. Rating × 3. Does not retroactively raise Willpower.
pub fn xp_cost_virtue_increase(new_rating: u8) -> u32 {
    new_rating as u32 * 3
}

/// Cost to raise Willpower. Rating × 2.
pub fn xp_cost_willpower_increase(new_rating: u8) -> u32 {
    new_rating as u32 * 2
}

/// Cost to raise Essence (both to 3 and above). Rating × 8.
pub fn xp_cost_essence_increase(new_rating: u8) -> u32 {
    new_rating as u32 * 8
}

/// Cost to buy a Charm. Favored/Caste = 8, Out-of-Caste = 10.
pub fn xp_cost_charm(favored_or_caste: bool) -> u32 {
    if favored_or_caste { 8 } else { 10 }
}

/// Eclipse-only non-Solar Charm (spirit charms, alien charms). Rulebook OCR
/// was corrupted ("Ai pectit") on p.275; both our document-search query
/// (2026-05-17) and `character_creation.md` §6.4 still show the corrupted
/// cell. The working estimate is "roughly double the out-of-Caste Solar
/// cost" → 20 XP. TODO(web-search): confirm against a clean PDF or the
/// White Wolf errata.
pub const NON_SOLAR_CHARM_XP_COST: u32 = 20;

/// Cost to learn a spell. Occult favored/caste = 8, otherwise = 10.
pub fn xp_cost_spell(occult_favored_or_caste: bool) -> u32 {
    if occult_favored_or_caste { 8 } else { 10 }
}

/// Cost to learn a Combo in play: sum of the minimum Ability ratings of the
/// member Charms (p.246).
pub fn xp_cost_combo(min_ability_sum: u32) -> u32 {
    min_ability_sum
}

/// Cost to buy one Degree of a Thaumaturgy Art. Occult caste/favored = 8,
/// otherwise = 10 (core p.140).
pub fn xp_cost_art_degree(occult_favored_or_caste: bool) -> u32 {
    if occult_favored_or_caste { 8 } else { 10 }
}

/// Cost to learn one thaumaturgic Procedure: a flat 1 XP each (core p.140).
pub fn xp_cost_procedure() -> u32 {
    1
}

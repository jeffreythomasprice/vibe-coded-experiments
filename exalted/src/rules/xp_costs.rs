//! Post-chargen experience-point costs. Source: Exalted 2E core rulebook,
//! pp. 275–276, "Experience Costs" table.
//!
//! **Rating-based costs price off the CURRENT rating, not the new one.** Per
//! p.276: "Increasing a trait costs banked experience equal to a multiple of
//! its current rating. This is the value of the trait before it is raised."
//! Worked example: raising a Caste Ability (Martial Arts) from 4 to 5 costs
//! (4 × 2) − 1 = 7 XP — the formula is evaluated at the *pre-increase* rating
//! of 4, not the post-increase rating of 5.
//!
//! Two cells in the table were OCR-corrupted in the copy used for reference:
//! - Specialty cost printed as "je". The conventional 2E value is **3 XP**;
//!   encoded here as `xp_cost_specialty`.
//! - Non-Solar Charm cost (Eclipse only) printed as "Ai pectit". Encoded
//!   here as `NON_SOLAR_CHARM_XP_COST` = 20 (double the out-of-Caste Solar
//!   charm cost); revisit against a clean PDF.

/// Cost in XP to raise an Attribute from `current_rating`. Rulebook: rating × 4,
/// priced at the rating *before* the increase (p.276).
pub fn xp_cost_attribute_increase(current_rating: u8) -> u32 {
    current_rating as u32 * 4
}

/// Cost in XP to raise an Ability from `current_rating`. Favored/Caste: (r × 2) − 1.
/// Out-of-Caste: r × 2. Priced at the rating *before* the increase (p.276); e.g.
/// raising a Caste Ability from 4 to 5 costs (4 × 2) − 1 = 7 XP.
pub fn xp_cost_ability_increase(current_rating: u8, favored_or_caste: bool) -> u32 {
    if favored_or_caste {
        (current_rating as u32 * 2).saturating_sub(1)
    } else {
        current_rating as u32 * 2
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

/// Cost to raise Virtue from `current_rating`. Rating × 3, priced at the rating
/// *before* the increase (p.276). Does not retroactively raise Willpower.
pub fn xp_cost_virtue_increase(current_rating: u8) -> u32 {
    current_rating as u32 * 3
}

/// Cost to raise Willpower from `current_rating`. Rating × 2, priced at the
/// rating *before* the increase (p.276).
pub fn xp_cost_willpower_increase(current_rating: u8) -> u32 {
    current_rating as u32 * 2
}

/// Cost to raise Essence from `current_rating` (both to 3 and above). Rating × 8,
/// priced at the rating *before* the increase (p.276).
pub fn xp_cost_essence_increase(current_rating: u8) -> u32 {
    current_rating as u32 * 8
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Core p.276 worked example: Marcus raises Jin's Martial Arts (a Caste
    /// Ability) from 4 to 5. Cost = (4 × 2) − 1 = 7, priced at the rating
    /// *before* the raise (4), not after (5).
    #[test]
    fn book_example_caste_ability_four_to_five() {
        assert_eq!(xp_cost_ability_increase(4, true), 7);
    }

    #[test]
    fn ability_increase_uses_current_rating() {
        // Out-of-caste Lore 3 -> 4: priced at current rating 3, so 3 * 2 = 6.
        assert_eq!(xp_cost_ability_increase(3, false), 6);
    }

    #[test]
    fn attribute_increase_uses_current_rating() {
        // Strength 4 -> 5: priced at current rating 4, so 4 * 4 = 16.
        assert_eq!(xp_cost_attribute_increase(4), 16);
    }

    #[test]
    fn virtue_increase_uses_current_rating() {
        // Virtue 2 -> 3: priced at current rating 2, so 2 * 3 = 6.
        assert_eq!(xp_cost_virtue_increase(2), 6);
    }

    #[test]
    fn willpower_increase_uses_current_rating() {
        // Willpower 5 -> 6: priced at current rating 5, so 5 * 2 = 10.
        assert_eq!(xp_cost_willpower_increase(5), 10);
    }

    #[test]
    fn essence_increase_uses_current_rating() {
        // Essence 2 -> 3: priced at current rating 2, so 2 * 8 = 16.
        assert_eq!(xp_cost_essence_increase(2), 16);
    }
}

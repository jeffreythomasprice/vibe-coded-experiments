//! Character-side helpers for reasoning about XP totals. The canonical XP
//! cost table lives in `crate::rules::xp_costs`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::character::{Character, DotSource};
use crate::rules::chargen::{procedure_bp_cost, specialty_bp_cost_for_ability};

/// A single XP award entered by the user — a timestamped, free-form note
/// alongside the XP amount earned. `xp_earned` on the `Character` is the
/// authoritative running total; when `xp_awards` is non-empty, validation
/// requires it to match the sum of these amounts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct XpAward {
    pub amount: u32,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl XpAward {
    pub fn new(amount: u32, body: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            amount,
            body: body.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn edit(&mut self, amount: u32, body: impl Into<String>) {
        self.amount = amount;
        self.body = body.into();
        self.updated_at = Utc::now();
    }
}

/// Sum of every `XpAward.amount` on the character.
pub fn total_xp_awarded(c: &Character) -> u32 {
    c.xp_awards.iter().map(|a| a.amount).sum()
}

/// Total XP spent across every rated trait, charm, and specialty.
pub fn total_xp_spent(c: &Character) -> u32 {
    let mut total: u32 = 0;
    for t in c.attributes.values() {
        total += t.xp_spent_on_dots();
        total += t.xp_spent_on_specialties();
    }
    for t in c.abilities.values() {
        total += t.xp_spent_on_dots();
        total += t.xp_spent_on_specialties();
    }
    for craft in &c.crafts {
        total += craft.rating.xp_spent_on_dots();
        total += craft.rating.xp_spent_on_specialties();
    }
    for t in c.virtues.values() {
        total += t.xp_spent_on_dots();
    }
    total += c.willpower.xp_spent_on_dots();
    total += c.essence.xp_spent_on_dots();
    for charm in &c.charms {
        total += charm.source().xp_spent();
    }
    for combo in &c.combos {
        total += combo.source.xp_spent();
    }
    for spell in &c.spells {
        total += spell.source().xp_spent();
    }
    for intimacy in &c.intimacies {
        total += intimacy.source.xp_spent();
    }
    for bg in &c.backgrounds {
        total += bg.trait_().xp_spent_on_dots();
    }
    // Thaumaturgy: Degrees use the standard dot ledger; Procedures are 1 XP each.
    for art in &c.occult_arts {
        total += art.rating.xp_spent_on_dots();
        for proc in &art.procedures {
            total += proc.source.xp_spent();
        }
    }
    total
}

/// Total bonus points spent across every chargen-spendable category.
pub fn total_bp_spent(c: &Character) -> u32 {
    let mut total: u32 = 0;
    for t in c.attributes.values() {
        total += t.bp_spent_on_dots();
    }
    for (ab, t) in &c.abilities {
        total += t.bp_spent_on_dots();
        let cf = c.is_caste_or_favored_ability(*ab);
        let (n_cf, n_oc) = t
            .specialties
            .iter()
            .filter(|s| matches!(s.source, DotSource::BonusPoints { .. }))
            .fold((0usize, 0usize), |(cf_count, oc_count), _| {
                if cf {
                    (cf_count + 1, oc_count)
                } else {
                    (cf_count, oc_count + 1)
                }
            });
        total += specialty_bp_cost_for_ability(n_cf, n_oc);
    }
    // Crafts are separate abilities; Craft's Caste/Favored status applies to
    // each focus, and the "1 BP per 2 specialties" discount aggregates per
    // craft focus (matching the per-ability loop above).
    let craft_cf = c.is_caste_or_favored_ability(crate::character::AbilityKind::Craft);
    for craft in &c.crafts {
        total += craft.rating.bp_spent_on_dots();
        let (n_cf, n_oc) = craft
            .rating
            .specialties
            .iter()
            .filter(|s| matches!(s.source, DotSource::BonusPoints { .. }))
            .fold((0usize, 0usize), |(cf_count, oc_count), _| {
                if craft_cf {
                    (cf_count + 1, oc_count)
                } else {
                    (cf_count, oc_count + 1)
                }
            });
        total += specialty_bp_cost_for_ability(n_cf, n_oc);
    }
    for t in c.virtues.values() {
        total += t.bp_spent_on_dots();
    }
    total += c.willpower.bp_spent_on_dots();
    total += c.essence.bp_spent_on_dots();
    for charm in &c.charms {
        total += charm.source().bp_spent();
    }
    for combo in &c.combos {
        total += combo.source.bp_spent();
    }
    for spell in &c.spells {
        total += spell.source().bp_spent();
    }
    for intimacy in &c.intimacies {
        total += intimacy.source.bp_spent();
    }
    for bg in &c.backgrounds {
        total += bg.trait_().bp_spent_on_dots();
    }
    // Thaumaturgy: Degrees use the standard dot ledger; Procedures aggregate at
    // 3-per-BP within each Art (ceil), matching `validate_bp`.
    for art in &c.occult_arts {
        total += art.rating.bp_spent_on_dots();
        let n_bp_procs = art
            .procedures
            .iter()
            .filter(|p| matches!(p.source, DotSource::BonusPoints { .. }))
            .count();
        total += procedure_bp_cost(n_bp_procs);
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xp_award_new_sets_both_timestamps_equal() {
        let a = XpAward::new(4, "session 1");
        assert_eq!(a.amount, 4);
        assert_eq!(a.body, "session 1");
        assert_eq!(a.created_at, a.updated_at);
    }

    #[test]
    fn xp_award_edit_preserves_created_and_advances_updated() {
        let mut a = XpAward::new(2, "first");
        let original_created = a.created_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        a.edit(3, "second");
        assert_eq!(a.amount, 3);
        assert_eq!(a.body, "second");
        assert_eq!(a.created_at, original_created);
        assert!(a.updated_at > original_created);
    }
}

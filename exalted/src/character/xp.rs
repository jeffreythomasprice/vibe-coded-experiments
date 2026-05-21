//! Character-side helpers for reasoning about XP totals. The canonical XP
//! cost table lives in `crate::rules::xp_costs`.

use crate::character::{Character, DotSource};
use crate::rules::chargen::specialty_bp_cost_for_ability;

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
    for t in c.virtues.values() {
        total += t.xp_spent_on_dots();
    }
    total += c.willpower.xp_spent_on_dots();
    total += c.essence.xp_spent_on_dots();
    for charm in &c.charms {
        total += charm.source().xp_spent();
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
    for t in c.virtues.values() {
        total += t.bp_spent_on_dots();
    }
    total += c.willpower.bp_spent_on_dots();
    total += c.essence.bp_spent_on_dots();
    for charm in &c.charms {
        total += charm.source().bp_spent();
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
    total
}

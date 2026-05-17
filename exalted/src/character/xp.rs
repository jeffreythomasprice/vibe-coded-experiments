//! Character-side helpers for reasoning about XP totals. The canonical XP
//! cost table lives in `crate::rules::xp_costs`.

use crate::character::Character;

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
        total += charm.source.xp_spent();
    }
    for intimacy in &c.intimacies {
        total += intimacy.source.xp_spent();
    }
    for bg in c.backgrounds.values() {
        total += bg.xp_spent_on_dots();
    }
    total
}

/// Total bonus points spent across every chargen-spendable category.
pub fn total_bp_spent(c: &Character) -> u32 {
    let mut total: u32 = 0;
    for t in c.attributes.values() {
        total += t.bp_spent_on_dots();
        total += t.bp_spent_on_specialties();
    }
    for t in c.abilities.values() {
        total += t.bp_spent_on_dots();
        total += t.bp_spent_on_specialties();
    }
    for t in c.virtues.values() {
        total += t.bp_spent_on_dots();
    }
    total += c.willpower.bp_spent_on_dots();
    total += c.essence.bp_spent_on_dots();
    for charm in &c.charms {
        total += charm.source.bp_spent();
    }
    for intimacy in &c.intimacies {
        total += intimacy.source.bp_spent();
    }
    for bg in c.backgrounds.values() {
        total += bg.bp_spent_on_dots();
    }
    total
}

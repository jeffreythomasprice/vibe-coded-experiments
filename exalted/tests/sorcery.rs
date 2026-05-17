mod common;

use common::valid_dawn;
use exalted::character::{DotSource, Spell, SpellCircle};
use exalted::error::ValidationError;

#[test]
fn solar_circle_spell_rejected_at_chargen() {
    let mut c = valid_dawn();
    // Replace one chargen-priority charm with a chargen-priority Solar Circle
    // spell, keeping the 10-pick budget balanced.
    c.charms.retain(|ch| ch.name != "First Awareness Excellency");
    c.spells.push(Spell::new(
        "Total Annihilation",
        SpellCircle::Solar,
        DotSource::ChargenPriority,
    ));
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::SolarCircleAtChargen { .. })),
        "expected SolarCircleAtChargen, got {:#?}",
        report.errors
    );
}

#[test]
fn terrestrial_circle_spell_counts_toward_ten_picks() {
    let mut c = valid_dawn();
    // Swap a Charm for a Terrestrial spell — the count should still be 10.
    c.charms.retain(|ch| ch.name != "First Awareness Excellency");
    c.spells.push(Spell::new(
        "Death of Obsidian Butterflies",
        SpellCircle::Terrestrial,
        DotSource::ChargenPriority,
    ));
    let report = c.validate_chargen();
    assert!(
        !report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::CharmCountWrong { .. })),
        "10 picks (9 charms + 1 spell) should validate; errors: {:#?}",
        report.errors
    );
}

#[test]
fn spell_without_charm_swap_breaks_pick_budget() {
    let mut c = valid_dawn();
    // Add a Terrestrial spell at ChargenPriority WITHOUT removing a charm —
    // should now report 11 picks.
    c.spells.push(Spell::new(
        "Death of Obsidian Butterflies",
        SpellCircle::Terrestrial,
        DotSource::ChargenPriority,
    ));
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::CharmCountWrong { got: 11 })),
        "expected CharmCountWrong{{got: 11}}; got {:#?}",
        report.errors
    );
}

#[test]
fn bp_spell_cost_validated_against_occult_caste_favored() {
    // valid_dawn() does not have Occult as Caste or Favored (Dawn caste is
    // combat). So a BP spell should cost 5 BP. Paying 4 BP should error.
    let mut c = valid_dawn();
    c.spells.push(Spell::new(
        "Cirrus Skiff",
        SpellCircle::Terrestrial,
        DotSource::BonusPoints { spent: 4 },
    ));
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::BpCostWrong { trait_name, paid: 4, expected: 5 } if trait_name.starts_with("Spell::"))),
        "expected BpCostWrong (4 paid, 5 expected) for non-Occult-CF spell; got {:#?}",
        report.errors
    );
}

#[test]
fn xp_spell_cost_validated() {
    let mut c = valid_dawn();
    // XP-bought spell, wrong cost. Out-of-Occult-CF: should be 10 XP.
    c.xp_earned = 100;
    c.xp_banked = 92;
    c.spells.push(Spell::new(
        "Cirrus Skiff",
        SpellCircle::Terrestrial,
        DotSource::Xp { spent: 8 }, // Wrong: should be 10
    ));
    let report = c.validate_xp();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::XpCostWrong { trait_name, paid: 8, expected: 10 } if trait_name.starts_with("Spell::"))),
        "expected XpCostWrong (8 paid, 10 expected) for spell; got {:#?}",
        report.errors
    );
}

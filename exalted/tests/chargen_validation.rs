mod common;

use common::valid_dawn;
use exalted::character::{AbilityKind, BackgroundKind, DotPurchase, DotSource};
use exalted::error::ValidationError;

fn err_kinds(report: &exalted::error::ValidationReport) -> Vec<&ValidationError> {
    report.errors.iter().collect()
}

#[test]
fn baseline_character_validates() {
    let c = valid_dawn();
    let report = c.validate_chargen();
    assert!(
        report.is_ok(),
        "baseline should validate cleanly: {:#?}",
        report.errors
    );
}

#[test]
fn favored_overlapping_with_caste_is_caught() {
    let mut c = valid_dawn();
    c.favored_abilities[0] = AbilityKind::Melee; // Melee is Dawn caste
    let report = c.validate_chargen();
    assert!(matches!(
        report.errors.first(),
        Some(ValidationError::CasteFavoredOverlap { .. })
    ));
}

#[test]
fn missing_favored_ability_dot_is_caught() {
    let mut c = valid_dawn();
    // Stealth is favored and currently has 3 chargen-priority dots — drop to 0.
    let t = c.abilities.get_mut(&AbilityKind::Stealth).unwrap();
    t.purchases.clear();
    // Move those 3 dots somewhere else so the total of 28 still holds.
    let lore = c.abilities.get_mut(&AbilityKind::Lore).unwrap();
    // Lore already has 3; pushing 3 more would make 6 chargen-priority dots
    // — but the per-ability cap is 3 at chargen. Use a different ability:
    lore.purchases.pop();
    let medicine = c.abilities.get_mut(&AbilityKind::Medicine).unwrap();
    medicine.add_chargen();
    medicine.add_chargen();
    medicine.add_chargen();

    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::FavoredAbilityZeroDots { .. })),
        "expected FavoredAbilityZeroDots, got {:?}",
        report.errors
    );
}

#[test]
fn ability_chargen_pool_mismatch_caught() {
    let mut c = valid_dawn();
    // Drop one chargen-priority dot off any ability.
    c.abilities
        .get_mut(&AbilityKind::Athletics)
        .unwrap()
        .purchases
        .pop();
    let report = c.validate_chargen();
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::AbilityChargenDotsWrong { got: 27 }
    )));
}

#[test]
fn virtue_below_three_breaks_primary() {
    let mut c = valid_dawn();
    // Primary virtue Compassion is currently 3 (1 base + 2 chargen). Strip
    // both chargen dots and move them to Conviction (already at 3 — would
    // make 5 which violates virtue chargen-over-4 rule, so add elsewhere).
    {
        let comp = c.virtues.get_mut(&exalted::character::VirtueKind::Compassion).unwrap();
        comp.purchases.clear();
    }
    {
        let temp = c.virtues.get_mut(&exalted::character::VirtueKind::Temperance).unwrap();
        temp.add_chargen();
        temp.add_chargen();
    }
    let report = c.validate_chargen();
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::PrimaryVirtueTooLow { .. }
    )));
}

#[test]
fn bonus_point_total_mismatch_caught() {
    let mut c = valid_dawn();
    // Add an extra BP-purchased dot of Resources (worth 2 BP since it's the
    // 5th dot, above the ≤3 threshold). Pushes total to 17.
    c.backgrounds
        .get_mut(&BackgroundKind::Resources)
        .unwrap()
        .purchases
        .push(DotPurchase::new(DotSource::BonusPoints { spent: 2 }));
    let report = c.validate_chargen();
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::BonusPointsWrong { got: 17 }
    )));
}

#[test]
fn charm_count_wrong_caught() {
    let mut c = valid_dawn();
    // Remove the second-to-last chargen charm; leaves 9 ChargenPriority charms.
    c.charms.retain(|ch| ch.name != "First Athletics Excellency");
    let report = c.validate_chargen();
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::CharmCountWrong { got: 9 }
    )));
}

#[test]
fn caste_favored_charm_minimum_caught() {
    let mut c = valid_dawn();
    // Replace 6 of the 10 chargen-priority C/F charms with charms outside
    // caste/favored. Lore is non-caste, non-favored.
    let replacements = [
        "First Lore Excellency",
        "First Medicine Excellency",
        "First Investigation Excellency",
        "First Occult Excellency",
        "First Craft Excellency",
        "First Performance Excellency",
    ];
    let mut idx = 0;
    for charm in c.charms.iter_mut() {
        if !charm.source.is_chargen_priority() {
            continue;
        }
        if idx >= replacements.len() {
            break;
        }
        // Don't replace charms that are already non-C/F.
        charm.name = replacements[idx].to_string();
        idx += 1;
    }
    let report = c.validate_chargen();
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::CasteFavoredCharmsTooFew { .. }
    )));
}

#[test]
fn essence_over_chargen_max_caught() {
    let mut c = valid_dawn();
    // Push essence above 5 via BP. (7 BP per dot; the validator only cares
    // about the dot total, not whether the BP total still equals 15.)
    for _ in 0..4 {
        c.essence
            .purchases
            .push(DotPurchase::new(DotSource::BonusPoints { spent: 7 }));
    }
    let report = c.validate_chargen();
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::EssenceOverMaxAtChargen { .. }
    )));
}

#[test]
fn report_notes_unknown_charm_softly() {
    let mut c = valid_dawn();
    if let Some(ch) = c.charms.iter_mut().find(|ch| ch.name == "First Lore Excellency") {
        ch.name = "Some Made Up Charm".to_string();
    } else {
        // valid_dawn doesn't put a Lore Excellency at chargen; pick the last
        // chargen charm and rename it.
        let last = c
            .charms
            .iter_mut()
            .rev()
            .find(|ch| ch.source.is_chargen_priority())
            .unwrap();
        last.name = "Some Made Up Charm".to_string();
    }
    let report = c.validate_chargen();
    // The unknown charm shows up as a *note*, not an error. We don't care
    // about the rest of the report — just that this note exists.
    assert!(
        report
            .notes
            .iter()
            .any(|n| matches!(n, ValidationError::UnknownCharm { charm } if charm == "Some Made Up Charm")),
        "expected UnknownCharm note, got notes {:?}",
        report.notes
    );
    let _ = err_kinds(&report);
}

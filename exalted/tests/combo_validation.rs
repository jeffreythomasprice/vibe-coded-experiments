//! Combo validation: each rule in `check_combos` should surface as a
//! distinct, identifiable `ValidationError` variant, and the BP/XP cost
//! checks should fire when the user mis-pays a Combo.

mod common;

use common::valid_dawn;
use exalted::character::{Combo, DotSource};
use exalted::error::ValidationError;

/// Sum of `mins_ability` for each member Charm — the canonical XP cost of a
/// Combo bought after chargen (1 XP per Excellency-tier dot of prereq).
fn xp_for(charm_ids: &[&str]) -> u32 {
    let db = exalted::rules::database::database();
    charm_ids
        .iter()
        .filter_map(|id| db.charm(id))
        .map(|e| e.mins_ability as u32)
        .sum()
}

fn combo_xp(name: &str, ids: &[&str]) -> Combo {
    Combo {
        name: name.to_string(),
        charm_ids: ids.iter().map(|s| s.to_string()).collect(),
        source: DotSource::Xp {
            spent: xp_for(ids),
        },
        notes: Vec::new(),
    }
}

#[test]
fn baseline_with_clean_combo_has_no_combo_errors() {
    let mut c = valid_dawn();
    c.combos = vec![combo_xp(
        "Twin Excellence",
        &["first-awareness-excellency", "first-dodge-excellency"],
    )];
    c.xp_earned = xp_for(&["first-awareness-excellency", "first-dodge-excellency"]);
    let report = c.validate_chargen();
    let bad = report
        .errors
        .iter()
        .filter(|e| {
            matches!(
                e,
                ValidationError::ComboCharmNotOwned { .. }
                    | ValidationError::ComboCharmNotComboable { .. }
                    | ValidationError::ComboBasicWithNonReflexive { .. }
                    | ValidationError::ComboMultipleSimple { .. }
                    | ValidationError::ComboDuplicateCharm { .. }
                    | ValidationError::ComboInvalidSource { .. }
            )
        })
        .collect::<Vec<_>>();
    assert!(bad.is_empty(), "expected no combo errors; got {:#?}", bad);
}

#[test]
fn member_charm_not_owned_is_hard_error() {
    let mut c = valid_dawn();
    c.combos = vec![combo_xp(
        "Ghost Combo",
        &["first-awareness-excellency", "first-occult-excellency"],
    )];
    c.xp_earned = 100;
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::ComboCharmNotOwned { charm, .. }
                if charm == "first-occult-excellency"
        )),
        "expected ComboCharmNotOwned for first-occult-excellency; got {:#?}",
        report.errors,
    );
}

#[test]
fn duplicate_member_charm_is_hard_error() {
    let mut c = valid_dawn();
    c.combos = vec![combo_xp(
        "Echoing Excellence",
        &["first-awareness-excellency", "first-awareness-excellency"],
    )];
    c.xp_earned = 100;
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::ComboDuplicateCharm { charm, .. }
                if charm == "first-awareness-excellency"
        )),
        "expected ComboDuplicateCharm; got {:#?}",
        report.errors,
    );
}

#[test]
fn non_comboable_member_is_hard_error() {
    // Add Ox-Body Technique (Permanent, no Combo-OK keyword) to the
    // character, then try to Combo it. Should flag both the Permanent type
    // and the missing Combo-OK keyword.
    use exalted::character::{AbilityKind, CharmRef};
    let mut c = valid_dawn();
    // Resistance 1 is enough for one Ox-Body pick.
    c.abilities
        .get_mut(&AbilityKind::Resistance)
        .unwrap()
        .add_bonus(1);
    // Swap the BP charm to Ox-Body so we still have 11 charms total and a
    // non-Combo-OK target. We'll replace the second-martial-arts BP charm.
    let last = c.charms.len() - 1;
    c.charms[last] = CharmRef::Lookup {
        id: "ox-body-technique".to_string(),
        source: DotSource::BonusPoints { spent: 4 },
        non_solar: false,
        notes: Vec::new(),
        ox_body_pattern: Some(exalted::rules::health::OxBodyPattern::OneZero),
    };
    c.combos = vec![combo_xp(
        "Iron Hide",
        &["first-awareness-excellency", "ox-body-technique"],
    )];
    c.xp_earned = 100;
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::ComboCharmNotComboable { charm, .. }
                if charm.contains("Ox-Body")
        )),
        "expected ComboCharmNotComboable for Ox-Body Technique; got {:#?}",
        report.errors,
    );
}

#[test]
fn invalid_source_chargen_priority_is_hard_error() {
    let mut c = valid_dawn();
    c.combos = vec![Combo {
        name: "Free Combo".to_string(),
        charm_ids: vec!["first-awareness-excellency".to_string()],
        source: DotSource::ChargenPriority,
        notes: Vec::new(),
    }];
    c.xp_earned = 0;
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::ComboInvalidSource { .. })),
        "expected ComboInvalidSource; got {:#?}",
        report.errors,
    );
}

#[test]
fn xp_cost_mismatch_is_hard_error() {
    let mut c = valid_dawn();
    // Combo of 2 Excellencies should cost 2 XP. Underpay by 1.
    c.combos = vec![Combo {
        name: "Cheap Combo".to_string(),
        charm_ids: vec![
            "first-awareness-excellency".to_string(),
            "first-dodge-excellency".to_string(),
        ],
        source: DotSource::Xp { spent: 1 },
        notes: Vec::new(),
    }];
    c.xp_earned = 1;
    let report = c.validate_xp();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::XpCostWrong { trait_name, expected, .. }
                if trait_name.starts_with("Combo::") && *expected == 2
        )),
        "expected XpCostWrong for Combo::Cheap Combo; got {:#?}",
        report.errors,
    );
}

#[test]
fn bp_cost_combo_is_one_per_member() {
    use exalted::rules::chargen::combo_bp_cost;
    assert_eq!(combo_bp_cost(0), 0);
    assert_eq!(combo_bp_cost(1), 1);
    assert_eq!(combo_bp_cost(5), 5);
}

#[test]
fn markdown_render_includes_combo_section() {
    let mut c = valid_dawn();
    c.combos = vec![combo_xp(
        "Twin Excellence",
        &["first-awareness-excellency", "first-dodge-excellency"],
    )];
    let md = exalted::render::character_to_markdown(&c);
    assert!(md.contains("## Combos (1)"), "missing combo header in: {}", md);
    assert!(md.contains("Twin Excellence"), "missing combo name in: {}", md);
    assert!(md.contains("Activation: +1 WP"), "missing activation reminder in: {}", md);
}

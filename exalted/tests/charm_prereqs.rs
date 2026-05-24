//! Hard-error prerequisite validation: charm prereqs (plain id and wildcard),
//! attribute minimums, and spell→sorcery-charm gating.

mod common;

use std::collections::BTreeMap;

use common::valid_dawn;
use exalted::character::{
    AbilityKind, AttributeKind, CharmRef, DotSource, SpellRef,
};
use exalted::error::ValidationError;
use exalted::rules::database::{CharmEntry, CharmType};

#[test]
fn missing_named_prereq_is_hard_error() {
    // accuracy-without-distance requires there-is-no-wind. Add it without the
    // prereq; expect a hard CharmPrereqMissing error (not a soft note).
    let mut c = valid_dawn();
    // Bump Archery to 5 and Essence to 3 so the dot mins don't error first.
    c.abilities
        .get_mut(&AbilityKind::Archery)
        .unwrap()
        .add_bonus(2);
    c.abilities
        .get_mut(&AbilityKind::Archery)
        .unwrap()
        .add_bonus(2);
    c.essence.add_bonus(1);
    c.charms.push(CharmRef::lookup(
        "accuracy-without-distance",
        DotSource::BonusPoints { spent: 4 },
    ));
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(
                e,
                ValidationError::CharmPrereqMissing { missing, .. }
                    if missing == "there-is-no-wind"
            )),
        "expected hard CharmPrereqMissing for there-is-no-wind; got {:#?}",
        report.errors,
    );
}

#[test]
fn any_excellency_wildcard_is_satisfied_by_any_of_five() {
    // there-is-no-wind requires any-archery-excellency. valid_dawn already
    // has first-archery-excellency, so adding there-is-no-wind shouldn't
    // raise a prereq error.
    let mut c = valid_dawn();
    c.abilities
        .get_mut(&AbilityKind::Archery)
        .unwrap()
        .add_bonus(2);
    c.abilities
        .get_mut(&AbilityKind::Archery)
        .unwrap()
        .add_bonus(2);
    c.charms.push(CharmRef::lookup(
        "there-is-no-wind",
        DotSource::BonusPoints { spent: 4 },
    ));
    let report = c.validate_chargen();
    assert!(
        !report.errors.iter().any(|e| matches!(
            e,
            ValidationError::CharmPrereqAnyExcellencyMissing { .. }
                | ValidationError::CharmPrereqMissing { .. }
        )),
        "should have no missing prereq for there-is-no-wind: {:#?}",
        report.errors,
    );
}

#[test]
fn any_n_excellencies_wildcard_counted() {
    // Custom charm with prereq `any-two-archery-excellencies`. Character has
    // first-archery-excellency (1) → expect NExcellencies error reporting 1/2.
    let custom = CharmEntry {
        id: "test-two-archery-prereq".to_string(),
        name: "Test Two Archery Prereq".to_string(),
        exalt_type: "solar".to_string(),
        ability: "archery".to_string(),
        cost: "—".to_string(),
        mins_ability: 1,
        mins_essence: 1,
        charm_type: CharmType::Permanent,
        type_detail: "".to_string(),
        keywords: vec![],
        duration: "Permanent".to_string(),
        prerequisites: vec!["any-two-archery-excellencies".to_string()],
        mins_attribute: BTreeMap::new(),
        source: "test".to_string(),
        pages: "1".to_string(),
        effect: "".to_string(),
        description: "test".to_string(),
    };
    let mut c = valid_dawn();
    c.charms.push(CharmRef::Custom {
        entry: custom,
        source: DotSource::BonusPoints { spent: 4 },
        non_solar: false,
        notes: Vec::new(),
        ox_body_pattern: None,
    });
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::CharmPrereqNExcellenciesMissing {
                ability, required: 2, got: 1, ..
            } if ability == "Archery"
        )),
        "expected CharmPrereqNExcellenciesMissing(Archery, 2, got 1); got {:#?}",
        report.errors,
    );
}

#[test]
fn attribute_minimum_enforced() {
    // valid_dawn has Strength 4. Custom charm requiring Strength 5 should
    // trigger CharmAttributeBelowMin.
    let mut mins = BTreeMap::new();
    mins.insert(AttributeKind::Strength, 5);
    let custom = CharmEntry {
        id: "test-strength-prereq".to_string(),
        name: "Test Strength Prereq".to_string(),
        exalt_type: "solar".to_string(),
        ability: "athletics".to_string(),
        cost: "—".to_string(),
        mins_ability: 1,
        mins_essence: 1,
        charm_type: CharmType::Permanent,
        type_detail: "".to_string(),
        keywords: vec![],
        duration: "Permanent".to_string(),
        prerequisites: vec![],
        mins_attribute: mins,
        source: "test".to_string(),
        pages: "1".to_string(),
        effect: "".to_string(),
        description: "test".to_string(),
    };
    let mut c = valid_dawn();
    c.charms.push(CharmRef::Custom {
        entry: custom,
        source: DotSource::BonusPoints { spent: 4 },
        non_solar: false,
        notes: Vec::new(),
        ox_body_pattern: None,
    });
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::CharmAttributeBelowMin {
                attribute, required: 5, got: 4, ..
            } if attribute == "Strength"
        )),
        "expected CharmAttributeBelowMin(Strength, 5, got 4); got {:#?}",
        report.errors,
    );
}

#[test]
fn terrestrial_spell_requires_terrestrial_circle_sorcery() {
    // Add a Terrestrial spell via the chargen sorcery swap, drop one Charm
    // to keep the pick count at 10, but DON'T add terrestrial-circle-sorcery.
    let mut c = valid_dawn();
    c.charms.retain(|ch| !ch.is_id("first-awareness-excellency"));
    c.spells.push(SpellRef::lookup(
        "cirrus-skiff",
        DotSource::ChargenPriority,
    ));
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::SpellRequiresSorceryCharm { charm, .. }
                if charm == "terrestrial-circle-sorcery"
        )),
        "expected SpellRequiresSorceryCharm(terrestrial-circle-sorcery); got {:#?}",
        report.errors,
    );
}

#[test]
fn terrestrial_spell_with_sorcery_charm_passes_gate() {
    // Same setup, but include terrestrial-circle-sorcery as one of the 10
    // chargen picks. The gating check should not fire.
    let mut c = valid_dawn();
    // Drop two charms; replace with terrestrial-circle-sorcery + the spell
    // (so 9 charms + 1 spell = 10 picks).
    c.charms.retain(|ch| {
        !ch.is_id("first-awareness-excellency") && !ch.is_id("first-dodge-excellency")
    });
    c.charms.insert(
        0,
        CharmRef::lookup("terrestrial-circle-sorcery", DotSource::ChargenPriority),
    );
    c.spells.push(SpellRef::lookup(
        "cirrus-skiff",
        DotSource::ChargenPriority,
    ));
    // Occult is 3, so terrestrial-circle-sorcery's mins are met (Occult 3,
    // Essence 1+). Bump Essence isn't needed.
    let report = c.validate_chargen();
    assert!(
        !report.errors.iter().any(|e| matches!(
            e,
            ValidationError::SpellRequiresSorceryCharm { .. }
        )),
        "should not fire sorcery-gate when terrestrial-circle-sorcery is present: {:#?}",
        report.errors,
    );
}

#[test]
fn excellency_template_substitutes_prereqs() {
    // After the template-expansion fix, derived charms like
    // infinite-archery-mastery should have their prereqs substituted to
    // reference the same ability.
    let db = exalted::rules::database::database();
    let entry = db
        .charm("infinite-archery-mastery")
        .expect("derived Excellency should exist");
    assert_eq!(entry.prerequisites, vec!["any-archery-excellency"]);

    let flow = db
        .charm("archery-essence-flow")
        .expect("derived Excellency should exist");
    assert_eq!(flow.prerequisites, vec!["any-archery-excellency"]);
}

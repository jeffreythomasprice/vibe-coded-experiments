//! Multiple Craft abilities: each craft is a separately-rated ability sharing
//! one Caste/Favored slot, kept in `Character::crafts` rather than the ability
//! map. Covers accessors, legacy migration, TOML roundtrip, and the
//! chargen/XP/charm-prereq accounting that folds crafts in.

mod common;

use common::valid_dawn;
use exalted::character::xp::total_xp_spent;
use exalted::character::{AbilityKind, CharmRef, Craft, DotSource, RatedTrait};
use exalted::error::ValidationError;
use exalted::rules::database::init_database;
use exalted::rules::xp_costs::xp_cost_new_ability;

fn rating_chargen(n: usize) -> RatedTrait {
    let mut t = RatedTrait::with_base(0);
    for _ in 0..n {
        t.add_chargen();
    }
    t
}

// ---------------------------------------------------------------------------
// Accessors
// ---------------------------------------------------------------------------

#[test]
fn ability_craft_returns_best_rating_and_primary_is_first() {
    let mut c = valid_dawn();
    c.crafts = vec![
        Craft {
            focus: "Water".to_string(),
            rating: rating_chargen(2),
        },
        Craft {
            focus: "Fire".to_string(),
            rating: rating_chargen(4),
        },
    ];
    assert_eq!(c.craft_rating_max(), 4);
    // `ability(Craft)` is the best craft — a "Craft N" requirement is met by
    // having *a* craft at N.
    assert_eq!(c.ability(AbilityKind::Craft), 4);
    assert_eq!(c.primary_craft().map(|cr| cr.focus.as_str()), Some("Water"));
}

#[test]
fn no_crafts_means_zero_rating() {
    let c = valid_dawn();
    assert!(c.crafts.is_empty());
    assert_eq!(c.craft_rating_max(), 0);
    assert_eq!(c.ability(AbilityKind::Craft), 0);
    assert!(c.primary_craft().is_none());
}

// ---------------------------------------------------------------------------
// Legacy migration
// ---------------------------------------------------------------------------

#[test]
fn legacy_craft_in_ability_map_migrates_into_crafts() {
    let mut c = valid_dawn();
    // Older sheets stored Craft as an ability-map entry. Simulate one with
    // real dots and make sure it lands in `crafts`.
    c.abilities.insert(AbilityKind::Craft, rating_chargen(3));
    c.migrate_legacy_craft();
    assert!(!c.abilities.contains_key(&AbilityKind::Craft));
    assert_eq!(c.crafts.len(), 1);
    assert_eq!(c.crafts[0].focus, "");
    assert_eq!(c.crafts[0].rating.dots(), 3);
}

#[test]
fn blank_legacy_craft_is_dropped() {
    let mut c = valid_dawn();
    c.abilities
        .insert(AbilityKind::Craft, RatedTrait::with_base(0));
    c.migrate_legacy_craft();
    assert!(!c.abilities.contains_key(&AbilityKind::Craft));
    assert!(c.crafts.is_empty());
}

#[test]
fn migration_does_not_clobber_existing_crafts() {
    let mut c = valid_dawn();
    c.crafts = vec![Craft {
        focus: "Water".to_string(),
        rating: rating_chargen(2),
    }];
    c.abilities.insert(AbilityKind::Craft, rating_chargen(3));
    c.migrate_legacy_craft();
    // The stale map entry is dropped, but the real crafts are left alone.
    assert!(!c.abilities.contains_key(&AbilityKind::Craft));
    assert_eq!(c.crafts.len(), 1);
    assert_eq!(c.crafts[0].focus, "Water");
}

// ---------------------------------------------------------------------------
// TOML roundtrip
// ---------------------------------------------------------------------------

#[test]
fn crafts_survive_toml_roundtrip() {
    let mut c = valid_dawn();
    c.crafts = vec![
        Craft {
            focus: "Water".to_string(),
            rating: rating_chargen(3),
        },
        Craft {
            focus: "Magitech".to_string(),
            rating: rating_chargen(1),
        },
    ];
    let text = toml::to_string_pretty(&c).expect("serialize");
    let back: exalted::Character = toml::from_str(&text).expect("parse");
    assert_eq!(back, c);
}

// ---------------------------------------------------------------------------
// Chargen accounting
// ---------------------------------------------------------------------------

/// Reallocating chargen ability dots into crafts keeps the character valid:
/// the craft dots count toward the 28-dot pool just like ability dots.
#[test]
fn craft_chargen_dots_count_toward_pool() {
    let mut c = valid_dawn();
    assert!(c.validate_chargen().is_ok());
    // Free 3 non-caste/favored dots (Investigation 2→0, Lore 3→2) and spend
    // them as Craft (Water) 2 + Craft (Stone) 1.
    c.abilities
        .get_mut(&AbilityKind::Investigation)
        .unwrap()
        .purchases
        .clear();
    c.abilities
        .get_mut(&AbilityKind::Lore)
        .unwrap()
        .purchases
        .truncate(2);
    c.crafts = vec![
        Craft {
            focus: "Water".to_string(),
            rating: rating_chargen(2),
        },
        Craft {
            focus: "Stone".to_string(),
            rating: rating_chargen(1),
        },
    ];
    let report = c.validate_chargen();
    assert!(report.is_ok(), "{:#?}", report.errors);
}

#[test]
fn craft_above_three_from_chargen_is_flagged() {
    let mut c = valid_dawn();
    c.crafts = vec![Craft {
        focus: "Water".to_string(),
        rating: rating_chargen(4),
    }];
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::AbilityChargenOverThree { ability, got }
                if ability.contains("Craft") && *got == 4
        )),
        "{:#?}",
        report.errors
    );
}

#[test]
fn favored_craft_with_no_dots_is_flagged() {
    let mut c = valid_dawn();
    // Favor Craft but allocate no craft dots — at least one craft focus must
    // carry a chargen dot.
    c.favored_abilities.push(AbilityKind::Craft);
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::FavoredAbilityZeroDots { ability } if ability == "Craft"
        )),
        "{:#?}",
        report.errors
    );
}

// ---------------------------------------------------------------------------
// XP accounting
// ---------------------------------------------------------------------------

#[test]
fn new_craft_via_xp_counts_and_validates() {
    let mut c = valid_dawn();
    let cost = xp_cost_new_ability();
    c.xp_earned = cost;
    c.xp_banked = 0;
    let mut water = RatedTrait::with_base(0);
    water.add_xp(cost);
    c.crafts = vec![Craft {
        focus: "Water".to_string(),
        rating: water,
    }];
    assert_eq!(total_xp_spent(&c), cost);
    let report = c.validate_xp();
    assert!(report.is_ok(), "{:#?}", report.errors);
}

// ---------------------------------------------------------------------------
// Charm prerequisites — a "Craft N" charm is satisfied by any craft at N.
// ---------------------------------------------------------------------------

#[test]
fn craft_charm_min_ability_uses_best_craft() {
    init_database().ok();
    let mut c = valid_dawn();
    // object-strengthening-touch is a Craft charm with mins_ability = 2.
    c.charms.push(CharmRef::lookup(
        "first-craft-excellency",
        DotSource::BonusPoints { spent: 4 },
    ));
    c.charms.push(CharmRef::lookup(
        "object-strengthening-touch",
        DotSource::BonusPoints { spent: 4 },
    ));

    // No crafts → Craft rating 0 < 2 → ability-min error.
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::CharmAbilityBelowMin { ability, required, .. }
                if ability == "Craft" && *required == 2
        )),
        "expected CharmAbilityBelowMin for Craft; got {:#?}",
        report.errors
    );

    // A single Water craft at 2 satisfies it.
    c.crafts = vec![Craft {
        focus: "Water".to_string(),
        rating: rating_chargen(2),
    }];
    let report = c.validate_chargen();
    assert!(
        !report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::CharmAbilityBelowMin { .. })),
        "Craft 2 should satisfy mins_ability 2; got {:#?}",
        report.errors
    );
}

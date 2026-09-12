mod common;

use common::valid_dawn;
use exalted::character::{
    AbilityKind, AttributeKind, BackgroundKind, BackgroundRef, DotPurchase, DotSource, Intimacy,
    IntimacyKind, RatedTrait, Specialty,
};
use exalted::error::ValidationError;
use exalted::rules::xp_costs::{
    xp_cost_ability_increase, xp_cost_attribute_increase, xp_cost_specialty,
};

#[test]
fn baseline_xp_validates() {
    let c = valid_dawn();
    let report = c.validate_xp();
    assert!(report.is_ok(), "{:?}", report.errors);
}

#[test]
fn favored_ability_xp_cost_one_less() {
    // Buying Dodge 4→5 (favored): priced at the *current* rating (4) per
    // core p.276: (4 * 2) - 1 = 7 XP. This is the book's own worked example
    // (Marcus raising Jin's Martial Arts from 4 to 5, p.276).
    assert_eq!(xp_cost_ability_increase(4, true), 7);
    // Buying out-of-caste Lore 3→4: priced at current rating 3: 3*2 = 6 XP.
    assert_eq!(xp_cost_ability_increase(3, false), 6);
}

#[test]
fn correct_xp_purchase_passes() {
    let mut c = valid_dawn();
    // Three planned purchases:
    //   - Dodge 4 → 5 (favored): (4 * 2) - 1 = 7 XP
    //   - Ride 0 → 1 (new ability): 3 XP
    //   - Strength 4 → 5: priced at current rating 4: 4*4 = 16 XP
    let str_increase = xp_cost_attribute_increase(4);
    let total = 7 + 3 + str_increase;
    c.xp_earned = total;
    c.xp_banked = 0;

    let dodge = c.abilities.get_mut(&AbilityKind::Dodge).unwrap();
    dodge
        .purchases
        .push(DotPurchase::new(DotSource::Xp { spent: 7 }));

    let ride = c.abilities.get_mut(&AbilityKind::Ride).unwrap();
    ride.purchases
        .push(DotPurchase::new(DotSource::Xp { spent: 3 }));

    let str_ = c.attributes.get_mut(&AttributeKind::Strength).unwrap();
    str_.purchases.push(DotPurchase::new(DotSource::Xp {
        spent: str_increase,
    }));

    let report = c.validate_xp();
    assert!(report.is_ok(), "{:?}", report.errors);
}

#[test]
fn wrong_xp_cost_caught() {
    let mut c = valid_dawn();
    c.xp_earned = 100;
    c.xp_banked = 92;

    let dodge = c.abilities.get_mut(&AbilityKind::Dodge).unwrap();
    // Dodge 4 → 5 (favored) canonical = (4 * 2) - 1 = 7, but we paid 8.
    dodge
        .purchases
        .push(DotPurchase::new(DotSource::Xp { spent: 8 }));

    let report = c.validate_xp();
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::XpCostWrong {
            expected: 7,
            paid: 8,
            ..
        }
    )));
}

#[test]
fn overspending_caught() {
    let mut c = valid_dawn();
    c.xp_earned = 5;
    c.xp_banked = 0;

    // Buy a correctly-priced 7 XP favored ability increase (Dodge 4 → 5) —
    // too expensive for the 5 XP earned.
    let dodge = c.abilities.get_mut(&AbilityKind::Dodge).unwrap();
    dodge
        .purchases
        .push(DotPurchase::new(DotSource::Xp { spent: 7 }));

    let report = c.validate_xp();
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::XpOverspent {
            spent: 7,
            earned: 5
        }
    )));
}

#[test]
fn banked_total_must_balance() {
    let mut c = valid_dawn();
    c.xp_earned = 10;
    c.xp_banked = 10; // claim nothing spent
    let dodge = c.abilities.get_mut(&AbilityKind::Dodge).unwrap();
    dodge
        .purchases
        .push(DotPurchase::new(DotSource::Xp { spent: 7 }));
    let report = c.validate_xp();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::XpBankedWrong { banked: 10, .. }))
    );
}

#[test]
fn specialty_costs_three_xp() {
    let mut c = valid_dawn();
    c.xp_earned = 3;
    c.xp_banked = 0;
    let melee = c.abilities.get_mut(&AbilityKind::Melee).unwrap();
    melee.specialties.push(Specialty {
        name: "Daiklave".to_string(),
        source: DotSource::Xp {
            spent: xp_cost_specialty(),
        },
    });
    let report = c.validate_xp();
    assert!(report.is_ok(), "{:?}", report.errors);
}

#[test]
fn background_and_intimacy_xp_spends_are_noted_not_errors() {
    // Exalted 2E prices no XP purchase for Background dots or Intimacies
    // (no row on the p.276 table; Backgrounds shift through play, Intimacies
    // are built with commitment actions). Recording one as `DotSource::Xp`
    // should surface as a note, not fail validation, and should still count
    // toward the XP ledger.
    let mut c = valid_dawn();
    c.xp_earned = 8;
    c.xp_banked = 0;

    let mut backing = RatedTrait::with_base(0);
    backing
        .purchases
        .push(DotPurchase::new(DotSource::Xp { spent: 5 }));
    c.backgrounds
        .push(BackgroundRef::lookup_kind(BackgroundKind::Backing, backing));

    c.intimacies.push(Intimacy {
        description: "A debt of honor".to_string(),
        kind: IntimacyKind::Cause,
        source: DotSource::Xp { spent: 3 },
        rating: 1,
    });

    let report = c.validate_xp();
    assert!(report.is_ok(), "{:?}", report.errors);
    assert!(report.notes.iter().any(|e| matches!(
        e,
        ValidationError::XpPurchaseNotPriced { spent: 5, trait_name }
            if trait_name.starts_with("Background::")
    )));
    assert!(report.notes.iter().any(|e| matches!(
        e,
        ValidationError::XpPurchaseNotPriced { spent: 3, trait_name }
            if trait_name == "Intimacy::A debt of honor"
    )));
}

mod common;

use common::valid_dawn;
use exalted::character::{AbilityKind, AttributeKind, DotPurchase, DotSource, Specialty};
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
    // Buying Dodge 4→5 (favored): (5 * 2) - 1 = 9 XP.
    assert_eq!(xp_cost_ability_increase(5, true), 9);
    // Buying out-of-caste Lore 3→4: 4*2 = 8 XP.
    assert_eq!(xp_cost_ability_increase(4, false), 8);
}

#[test]
fn correct_xp_purchase_passes() {
    let mut c = valid_dawn();
    // Three planned purchases:
    //   - Dodge 4 → 5 (favored): 9 XP
    //   - Ride 0 → 1 (new ability): 3 XP
    //   - Strength 4 → 5: 5*4 = 20 XP
    let str_increase = xp_cost_attribute_increase(5);
    let total = 9 + 3 + str_increase;
    c.xp_earned = total;
    c.xp_banked = 0;

    let dodge = c.abilities.get_mut(&AbilityKind::Dodge).unwrap();
    dodge
        .purchases
        .push(DotPurchase::new(DotSource::Xp { spent: 9 }));

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
    // Dodge 4 → 5 (favored) canonical = 9, but we paid 8.
    dodge
        .purchases
        .push(DotPurchase::new(DotSource::Xp { spent: 8 }));

    let report = c.validate_xp();
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::XpCostWrong {
            expected: 9,
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

    // Buy a 9 XP favored ability increase — too expensive.
    let dodge = c.abilities.get_mut(&AbilityKind::Dodge).unwrap();
    dodge
        .purchases
        .push(DotPurchase::new(DotSource::Xp { spent: 9 }));

    let report = c.validate_xp();
    assert!(report.errors.iter().any(|e| matches!(
        e,
        ValidationError::XpOverspent {
            spent: 9,
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
        .push(DotPurchase::new(DotSource::Xp { spent: 9 }));
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

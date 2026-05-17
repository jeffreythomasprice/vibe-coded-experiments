mod common;

use common::valid_dawn;
use exalted::character::{AbilityKind, AttributeKind, DotSource, Specialty};
use exalted::rules::{
    dice_pool, dodge_dv, join_battle, mdv_dodge, personal_essence_max, peripheral_essence_max,
    soak_bashing, soak_lethal, wound_penalty,
};

#[test]
fn dex_plus_dodge_pool() {
    let c = valid_dawn();
    // Dex = 1 base + 3 chargen + 1 BP = 5. Dodge = 0 base + 3 chargen + 1 BP = 4.
    assert_eq!(c.attribute(AttributeKind::Dexterity), 5);
    assert_eq!(c.ability(AbilityKind::Dodge), 4);
    assert_eq!(dice_pool(&c, AttributeKind::Dexterity, AbilityKind::Dodge, None), 9);
}

#[test]
fn specialty_adds_one_capped_at_three() {
    let mut c = valid_dawn();
    let melee = c.abilities.get_mut(&AbilityKind::Melee).unwrap();
    for _ in 0..5 {
        melee.specialties.push(Specialty {
            name: "Daiklave".to_string(),
            source: DotSource::ChargenPriority,
        });
    }
    // Specialty contribution caps at 3.
    let pool = dice_pool(&c, AttributeKind::Dexterity, AbilityKind::Melee, Some("Daiklave"));
    // Dex 5 + Melee 2 + Specialty min(5, 3) = 10
    assert_eq!(pool, 5 + 2 + 3);
}

#[test]
fn dodge_dv_solar_rounding() {
    let c = valid_dawn();
    // Dex 5 + Dodge 4 + Essence 2 = 11. ⌈11/2⌉ = 6. +2 = 8.
    assert_eq!(dodge_dv(&c), 8);
}

#[test]
fn mdv_dodge_uses_will_integrity_essence() {
    let c = valid_dawn();
    // Willpower 6 + Integrity 0 + Essence 2 = 8. ⌊8/2⌋ = 4.
    assert_eq!(mdv_dodge(&c), 4);
}

#[test]
fn soak_uses_stamina() {
    let c = valid_dawn();
    // Stamina 3 → bashing 3, lethal ⌊3/2⌋ = 1
    assert_eq!(soak_bashing(&c), 3);
    assert_eq!(soak_lethal(&c), 1);
}

#[test]
fn join_battle_is_wits_plus_awareness() {
    let c = valid_dawn();
    // Wits 2 + Awareness 4 = 6
    assert_eq!(join_battle(&c), 6);
}

#[test]
fn essence_pools_use_published_formulas() {
    let c = valid_dawn();
    // Essence 2, Willpower 6, Virtues 3+3+1+2 = 9.
    // Personal = 2*3 + 6 = 12.
    // Peripheral = 2*7 + 6 + 9 = 29.
    assert_eq!(personal_essence_max(&c), 12);
    assert_eq!(peripheral_essence_max(&c), 29);
}

#[test]
fn wound_penalty_uses_track_tier() {
    let mut c = valid_dawn();
    // No damage = 0.
    assert_eq!(wound_penalty(&c), 0);
    // 1 bashing → -0.
    c.pool_state.health_damage.bashing = 1;
    assert_eq!(wound_penalty(&c), 0);
    // 4 damage total → on the second -2 row.
    c.pool_state.health_damage.bashing = 4;
    assert_eq!(wound_penalty(&c), -2);
    // 6 damage total → -4.
    c.pool_state.health_damage.bashing = 6;
    assert_eq!(wound_penalty(&c), -4);
}

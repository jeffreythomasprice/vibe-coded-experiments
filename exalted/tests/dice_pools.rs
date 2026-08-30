mod common;

use common::valid_dawn;
use exalted::character::equipment::DamageType;
use exalted::character::{AbilityKind, Armor, AttributeKind, DotSource, Specialty, Weapon};
use exalted::rules::{
    best_parry_weapon, dice_pool, dodge_dv, join_battle, mdv_dodge, mdv_parry, parry_dv,
    peripheral_essence_max, personal_essence_max, soak_bashing, soak_lethal, wound_penalty,
};

fn weapon(name: &str, ability: AbilityKind, defense: i8) -> Weapon {
    Weapon {
        name: name.to_string(),
        ability,
        speed: 5,
        accuracy: 0,
        damage: 0,
        damage_type: DamageType::Lethal,
        defense,
        rate: 1,
        range_yards: None,
        tags: Vec::new(),
        attunement_motes: None,
        artifact_name: None,
    }
}

fn armor_with_mobility_penalty(penalty: u8) -> Armor {
    Armor {
        name: "Test armor".to_string(),
        soak_bashing: 0,
        soak_lethal: 0,
        soak_aggravated: 0,
        hardness_bashing: 0,
        hardness_lethal: 0,
        mobility_penalty: penalty,
        fatigue: 0,
        attunement_motes: None,
        artifact_name: None,
    }
}

#[test]
fn dex_plus_dodge_pool() {
    let c = valid_dawn();
    // Dex = 1 base + 3 chargen + 1 BP = 5. Dodge = 0 base + 3 chargen + 1 BP = 4.
    assert_eq!(c.attribute(AttributeKind::Dexterity), 5);
    assert_eq!(c.ability(AbilityKind::Dodge), 4);
    assert_eq!(
        dice_pool(&c, AttributeKind::Dexterity, AbilityKind::Dodge, None),
        9
    );
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
    let pool = dice_pool(
        &c,
        AttributeKind::Dexterity,
        AbilityKind::Melee,
        Some("Daiklave"),
    );
    // Dex 5 + Melee 2 + Specialty min(5, 3) = 10
    assert_eq!(pool, 5 + 2 + 3);
}

#[test]
fn dodge_dv_floors() {
    let c = valid_dawn();
    // Dex 5 + Dodge 4 + Essence 2 = 11. ⌊11/2⌋ = 5.
    assert_eq!(dodge_dv(&c), 5);
}

#[test]
fn dodge_dv_subtracts_mobility_penalty() {
    let mut c = valid_dawn();
    c.equipment.armor = Some(armor_with_mobility_penalty(3));
    // Dodge DV 5 (see dodge_dv_floors), minus mobility penalty 3 = 2.
    assert_eq!(dodge_dv(&c), 2);
}

#[test]
fn mdv_dodge_uses_will_integrity_essence() {
    let c = valid_dawn();
    // Willpower 6 + Integrity 0 + Essence 2 = 8. ⌊8/2⌋ = 4.
    assert_eq!(mdv_dodge(&c, None), 4);
}

#[test]
fn mdv_dodge_adds_matching_specialty() {
    let mut c = valid_dawn();
    let integrity = c.abilities.get_mut(&AbilityKind::Integrity).unwrap();
    for _ in 0..2 {
        integrity.specialties.push(Specialty {
            name: "Stoicism".to_string(),
            source: DotSource::ChargenPriority,
        });
    }
    // Willpower 6 + Integrity 0 + specialty 2 + Essence 2 = 10. ⌊10/2⌋ = 5.
    assert_eq!(mdv_dodge(&c, Some("Stoicism")), 5);
    // Non-matching specialty name contributes nothing.
    assert_eq!(mdv_dodge(&c, Some("Nerves of Steel")), 4);
}

#[test]
fn mdv_parry_floors_and_adds_specialty() {
    let mut c = valid_dawn();
    // Charisma 3 (base 1 + 2 chargen from valid_dawn) + Presence 4 (bumped
    // from base 0 here) = 7. ⌊7/2⌋ = 3.
    let presence = c.abilities.get_mut(&AbilityKind::Presence).unwrap();
    for _ in 0..4 {
        presence.add_chargen();
    }
    assert_eq!(
        mdv_parry(&c, AttributeKind::Charisma, AbilityKind::Presence, None),
        3
    );
    let presence = c.abilities.get_mut(&AbilityKind::Presence).unwrap();
    presence.specialties.push(Specialty {
        name: "Rhetoric".to_string(),
        source: DotSource::ChargenPriority,
    });
    // + specialty 1 = 8. ⌊8/2⌋ = 4.
    assert_eq!(
        mdv_parry(
            &c,
            AttributeKind::Charisma,
            AbilityKind::Presence,
            Some("Rhetoric")
        ),
        4
    );
}

#[test]
fn parry_dv_floors_with_positive_weapon_defense() {
    let c = valid_dawn();
    // Dex 5 + Melee 2 + staff Defense 2 = 9. ⌊9/2⌋ = 4.
    let staff = weapon("Staff", AbilityKind::Melee, 2);
    assert_eq!(parry_dv(&c, &staff), 4);
}

#[test]
fn parry_dv_clamps_to_zero_with_unwieldy_weapon() {
    let c = valid_dawn();
    // Dex 5 + Melee 2 + sledge Defense -3 = 4. ⌊4/2⌋ = 2 (still positive here);
    // push further negative to confirm the floor at 0.
    let sledge = weapon("Sledge", AbilityKind::Melee, -10);
    assert_eq!(parry_dv(&c, &sledge), 0);
}

#[test]
fn parry_dv_ignores_mobility_penalty() {
    let mut c = valid_dawn();
    c.equipment.armor = Some(armor_with_mobility_penalty(3));
    let staff = weapon("Staff", AbilityKind::Melee, 2);
    // Same as the unencumbered case (4): mobility penalty only hits Dodge DV.
    assert_eq!(parry_dv(&c, &staff), 4);
}

#[test]
fn best_parry_weapon_skips_ranged_abilities() {
    let mut c = valid_dawn();
    // Bow would have the highest raw Parry DV, but Archery can't parry.
    let bow = weapon("Bow", AbilityKind::Archery, 4);
    let sword = weapon("Sword", AbilityKind::Melee, 0);
    c.equipment.weapons = vec![bow, sword];
    let best = best_parry_weapon(&c).expect("a melee weapon is present");
    assert_eq!(best.name, "Sword");
}

#[test]
fn best_parry_weapon_none_when_unarmed() {
    let c = valid_dawn();
    assert!(c.equipment.weapons.is_empty());
    assert!(best_parry_weapon(&c).is_none());
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

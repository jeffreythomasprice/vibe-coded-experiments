mod common;

use common::valid_dawn;
use exalted::character::{
    AbilityKind, BackgroundKind, BackgroundRef, CharmRef, DotPurchase, DotSource, KnownLanguage,
    LanguageFamily, RatedTrait, Specialty,
};
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
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::AbilityChargenDotsWrong { got: 27 }))
    );
}

#[test]
fn virtue_below_three_breaks_primary() {
    let mut c = valid_dawn();
    // Primary virtue Compassion is currently 3 (1 base + 2 chargen). Strip
    // both chargen dots and move them to Conviction (already at 3 — would
    // make 5 which violates virtue chargen-over-4 rule, so add elsewhere).
    {
        let comp = c
            .virtues
            .get_mut(&exalted::character::VirtueKind::Compassion)
            .unwrap();
        comp.purchases.clear();
    }
    {
        let temp = c
            .virtues
            .get_mut(&exalted::character::VirtueKind::Temperance)
            .unwrap();
        temp.add_chargen();
        temp.add_chargen();
    }
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::PrimaryVirtueTooLow { .. }))
    );
}

#[test]
fn bonus_point_total_mismatch_caught() {
    let mut c = valid_dawn();
    // Add an extra BP-purchased dot of Resources (worth 2 BP since it's the
    // 5th dot, above the ≤3 threshold). Pushes total to 17.
    let db = exalted::rules::database::database();
    let resources = c
        .backgrounds
        .iter_mut()
        .find(|b| b.kind(db) == Some(BackgroundKind::Resources))
        .unwrap();
    resources
        .trait_mut()
        .purchases
        .push(DotPurchase::new(DotSource::BonusPoints { spent: 2 }));
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::BonusPointsWrong { got: 17 }))
    );
}

#[test]
fn charm_count_wrong_caught() {
    let mut c = valid_dawn();
    // Remove the second-to-last chargen charm; leaves 9 ChargenPriority charms.
    c.charms
        .retain(|ch| !ch.is_id("first-athletics-excellency"));
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::CharmCountWrong { got: 9 }))
    );
}

#[test]
fn caste_favored_charm_minimum_caught() {
    let mut c = valid_dawn();
    // Replace 6 of the 10 chargen-priority C/F charms with charms outside
    // caste/favored. Lore is non-caste, non-favored.
    let replacements = [
        "first-lore-excellency",
        "first-medicine-excellency",
        "first-investigation-excellency",
        "first-occult-excellency",
        "first-craft-excellency",
        "first-performance-excellency",
    ];
    let mut idx = 0;
    for charm in c.charms.iter_mut() {
        if !charm.source().is_chargen_priority() {
            continue;
        }
        if idx >= replacements.len() {
            break;
        }
        // Don't replace charms that are already non-C/F.
        *charm = CharmRef::lookup(replacements[idx], charm.source());
        idx += 1;
    }
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::CasteFavoredCharmsTooFew { .. }))
    );
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
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::EssenceOverMaxAtChargen { .. }))
    );
}

#[test]
fn ox_body_over_resistance_caught() {
    let mut c = valid_dawn();
    // valid_dawn has Resistance 0. Swap in an Ox-Body charm to trigger the
    // cap (got 1, max 0).
    if let Some(charm) = c
        .charms
        .iter_mut()
        .find(|ch| ch.is_id("first-war-excellency"))
    {
        *charm = CharmRef::lookup("ox-body-technique", DotSource::ChargenPriority);
    }
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::OxBodyOverResistance { got: 1, max: 0 })),
        "expected OxBodyOverResistance, got {:#?}",
        report.errors
    );
}

#[test]
fn followers_without_support_caught() {
    let mut c = valid_dawn();
    // valid_dawn has Resources 4 (3 chargen + 1 BP), Mentor 3, Contacts 3,
    // and no Backing / Influence / Followers. Add a Followers 5 instance
    // (chargen-priority dots) — Resources is the highest support at 4, so
    // Followers 5 > 4 should fire FollowersWithoutSupport.
    // Use BP for clarity (we don't care about the chargen pool here; the
    // test is about the interlock).
    let mut followers = RatedTrait::with_base(0);
    for _ in 0..3 {
        followers.add_chargen();
    }
    followers.add_bonus(1);
    followers.add_bonus(2);
    c.backgrounds.push(BackgroundRef::lookup_kind(
        BackgroundKind::Followers,
        followers,
    ));
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::FollowersWithoutSupport {
                followers: 5,
                support: 4
            }
        )),
        "expected FollowersWithoutSupport, got {:#?}",
        report.errors
    );
}

#[test]
fn bp_cost_mismatch_caught_on_attribute() {
    let mut c = valid_dawn();
    // valid_dawn pays 4 BP for Dex 4→5. Change it to a wrong amount.
    let dex = c
        .attributes
        .get_mut(&exalted::character::AttributeKind::Dexterity)
        .unwrap();
    for p in dex.purchases.iter_mut() {
        if let DotSource::BonusPoints { spent } = p.source {
            // Wrong BP cost: should be 4, say it was 3.
            p.source = DotSource::BonusPoints { spent: spent - 1 };
        }
    }
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::BpCostWrong { trait_name, .. } if trait_name.starts_with("Attribute::"))),
        "expected BpCostWrong on Attribute, got {:#?}",
        report.errors
    );
}

#[test]
fn tribal_tongues_counted_separately_from_families() {
    let mut c = valid_dawn();
    // Linguistics 0 → max 1 family + 0 tribal tongues. Add a tribal tongue:
    // should error on tribal cap but NOT on family cap.
    c.languages.push(KnownLanguage {
        family: LanguageFamily::TribalTongue("Marukan".to_string()),
        dialect_specialty: None,
        native: false,
    });
    let report = c.validate_chargen();
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::TooManyTribalTongues { got: 1, max: 0 })),
        "expected TooManyTribalTongues, got {:#?}",
        report.errors
    );
    assert!(
        !report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::TooManyLanguages { .. })),
        "tribal tongue should not count against family cap; got {:#?}",
        report.errors
    );
}

#[test]
fn report_notes_unknown_charm_softly() {
    let mut c = valid_dawn();
    // Replace the last chargen-priority charm with one whose id doesn't
    // appear in the rules database. The validator should emit a soft note,
    // not an error, for the unknown id.
    let last = c
        .charms
        .iter_mut()
        .rev()
        .find(|ch| ch.source().is_chargen_priority())
        .unwrap();
    *last = CharmRef::lookup("some-made-up-charm", DotSource::ChargenPriority);
    let report = c.validate_chargen();
    assert!(
        report
            .notes
            .iter()
            .any(|n| matches!(n, ValidationError::UnknownCharm { charm } if charm == "some-made-up-charm")),
        "expected UnknownCharm note, got notes {:?}",
        report.notes
    );
    let _ = err_kinds(&report);
}

// ---------------------------------------------------------------------------
// Specialty caps (rules p.77): max 3 distinct specialties per Ability, max 3
// dots within any one specialty. Linguistics is exempt from both.
// ---------------------------------------------------------------------------

/// Push `n` `Specialty` entries with the same name and the given source.
/// Each entry counts as one rating dot for that specialty.
fn add_spec(t: &mut RatedTrait, name: &str, n: u8, source: DotSource) {
    for _ in 0..n {
        t.specialties.push(Specialty {
            name: name.to_string(),
            source,
        });
    }
}

#[test]
fn three_dot_specialty_in_caste_ability_validates() {
    let mut c = valid_dawn();
    // Awareness is favored. Add a 3-dot specialty paid for with BP: in a
    // C/F ability, 3 dots cost ⌈3/2⌉ = 2 BP. Encode as 1 + 1 + 0.
    add_spec(
        c.abilities.get_mut(&AbilityKind::Awareness).unwrap(),
        "Listening",
        1,
        DotSource::BonusPoints { spent: 1 },
    );
    add_spec(
        c.abilities.get_mut(&AbilityKind::Awareness).unwrap(),
        "Listening",
        1,
        DotSource::BonusPoints { spent: 1 },
    );
    add_spec(
        c.abilities.get_mut(&AbilityKind::Awareness).unwrap(),
        "Listening",
        1,
        DotSource::BonusPoints { spent: 0 },
    );
    // Rebalance BP: remove the 2 BP we now spent on the specialty from the
    // Dexterity 4→5 purchase (drop one BP-dot there).
    let dex = c
        .attributes
        .get_mut(&exalted::character::AttributeKind::Dexterity)
        .unwrap();
    // valid_dawn pushes a single 4-BP dot on Dex. Replace it with a 2-BP one
    // and add a chargen rebalance elsewhere is hard; simpler: remove the BP
    // entirely and accept that this character has Dex 4. That changes the
    // chargen-priority math too, so just drop the BP and accept the report
    // may still flag bonus-point total — we only care about the specialty
    // cap not firing.
    dex.purchases
        .retain(|p| !matches!(p.source, DotSource::BonusPoints { .. }));

    let report = c.validate_chargen();
    assert!(
        !report.errors.iter().any(|e| matches!(
            e,
            ValidationError::SpecialtiesOverMax { .. }
                | ValidationError::SpecialtyOverMaxDots { .. }
        )),
        "3-dot specialty should be within caps, but got: {:?}",
        report.errors
    );
}

#[test]
fn four_dot_specialty_caught() {
    let mut c = valid_dawn();
    add_spec(
        c.abilities.get_mut(&AbilityKind::Melee).unwrap(),
        "Sword",
        4,
        DotSource::Xp { spent: 0 },
    );
    let report = c.validate_chargen();
    let found = report.errors.iter().any(|e| {
        matches!(
            e,
            ValidationError::SpecialtyOverMaxDots { ability, specialty, got }
                if ability == "Melee" && specialty == "Sword" && *got == 4
        )
    });
    assert!(
        found,
        "expected SpecialtyOverMaxDots on Melee::Sword=4, got: {:?}",
        report.errors
    );
}

#[test]
fn linguistics_specialty_dots_uncapped() {
    let mut c = valid_dawn();
    // 5 dots in a single Linguistics dialect specialty is fine — Linguistics
    // is exempt from the per-specialty dot cap.
    add_spec(
        c.abilities.get_mut(&AbilityKind::Linguistics).unwrap(),
        "Old Realm",
        5,
        DotSource::Xp { spent: 0 },
    );
    let report = c.validate_chargen();
    assert!(
        !report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::SpecialtyOverMaxDots { .. })),
        "Linguistics should be exempt; got: {:?}",
        report.errors
    );
}

#[test]
fn four_distinct_specialties_caught() {
    let mut c = valid_dawn();
    // Athletics is favored; 4 distinct 1-dot specialties exceeds the cap.
    let ath = c.abilities.get_mut(&AbilityKind::Athletics).unwrap();
    for name in ["Running", "Climbing", "Swimming", "Jumping"] {
        ath.specialties.push(Specialty {
            name: name.to_string(),
            source: DotSource::Xp { spent: 0 },
        });
    }
    let report = c.validate_chargen();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::SpecialtiesOverMax { ability, got }
                if ability == "Athletics" && *got == 4
        )),
        "expected SpecialtiesOverMax on Athletics=4, got: {:?}",
        report.errors
    );
}

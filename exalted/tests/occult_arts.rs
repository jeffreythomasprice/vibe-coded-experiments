//! Thaumaturgy (Occult Arts) validation and accounting.
//!
//! `valid_dawn` is a fully-valid Solar with Occult 3 (non-Caste/Favored) and
//! Lore 3 — enough to reach an Art's Adept Degree but not Master, and enough
//! Lore for Alchemy's Initiate but not its Adept requirement. That lets these
//! tests exercise the ladders without perturbing the chargen pools.

mod common;

use common::valid_dawn;
use exalted::character::xp::{total_bp_spent, total_xp_spent};
use exalted::character::{DotSource, OccultArt, Procedure};
use exalted::error::ValidationError;
use exalted::rules::chargen::{
    art_degree_bp_cost, art_degree_occult_min, procedure_bp_cost, procedure_occult_min,
};
use exalted::rules::xp_costs::{xp_cost_art_degree, xp_cost_procedure};

/// Build an Art at `degree`, each Degree paid with the canonical XP cost for a
/// non-Caste/Favored Occult character (10 XP).
fn art_xp(id: &str, degree: u8) -> OccultArt {
    let mut a = OccultArt::lookup(id);
    for _ in 0..degree {
        a.rating.add_xp(10);
    }
    a
}

fn has_error(
    report: &exalted::error::ValidationReport,
    pred: impl Fn(&ValidationError) -> bool,
) -> bool {
    report.errors.iter().any(pred)
}

fn has_note(
    report: &exalted::error::ValidationReport,
    pred: impl Fn(&ValidationError) -> bool,
) -> bool {
    report.notes.iter().any(pred)
}

// --------------------------------------------------------------------------
// Cost tables
// --------------------------------------------------------------------------

#[test]
fn degree_costs_reflect_occult_discount() {
    assert_eq!(art_degree_bp_cost(false), 5);
    assert_eq!(art_degree_bp_cost(true), 4);
    assert_eq!(xp_cost_art_degree(false), 10);
    assert_eq!(xp_cost_art_degree(true), 8);
}

#[test]
fn procedure_costs_and_floors() {
    assert_eq!(xp_cost_procedure(), 1);
    // 3 Procedures per 1 BP → ceil(n/3).
    assert_eq!(procedure_bp_cost(0), 0);
    assert_eq!(procedure_bp_cost(1), 1);
    assert_eq!(procedure_bp_cost(3), 1);
    assert_eq!(procedure_bp_cost(4), 2);
    // Occult floors: Degrees 1/3/5; Procedures 1 (Initiate/Adept) and 3 (Master).
    assert_eq!(art_degree_occult_min(1), 1);
    assert_eq!(art_degree_occult_min(2), 3);
    assert_eq!(art_degree_occult_min(3), 5);
    assert_eq!(procedure_occult_min(1), 1);
    assert_eq!(procedure_occult_min(2), 1);
    assert_eq!(procedure_occult_min(3), 3);
}

// --------------------------------------------------------------------------
// Happy path
// --------------------------------------------------------------------------

#[test]
fn valid_art_passes_all_validation() {
    let mut c = valid_dawn();
    // Astrology to Adept (needs Occult 3 — met), no extra requirements.
    c.occult_arts.push(art_xp("astrology", 2));
    // Pay for it: 2 Degrees × 10 = 20 XP.
    c.xp_earned = 20;
    c.xp_banked = 0;

    let chargen = c.validate_chargen();
    assert!(chargen.is_ok(), "chargen: {:?}", chargen.errors);
    let xp = c.validate_xp();
    assert!(xp.is_ok(), "xp: {:?}", xp.errors);
    assert_eq!(c.thaumaturgy_dice_bonus(), 2);
    assert_eq!(c.occult_art_degree("astrology"), 2);
}

// --------------------------------------------------------------------------
// Structural checks (check_occult_arts)
// --------------------------------------------------------------------------

#[test]
fn master_degree_requires_occult_five() {
    let mut c = valid_dawn(); // Occult 3
    c.occult_arts.push(art_xp("astrology", 3)); // Master needs Occult 5
    let report = c.validate_chargen();
    assert!(has_error(&report, |e| matches!(
        e,
        ValidationError::ArtOccultTooLow {
            required: 5,
            got: 3,
            ..
        }
    )));
}

#[test]
fn degree_above_three_is_rejected() {
    let mut c = valid_dawn();
    c.occult_arts.push(art_xp("astrology", 4));
    let report = c.validate_chargen();
    assert!(has_error(&report, |e| matches!(
        e,
        ValidationError::ArtDegreeOverMax { got: 4, .. }
    )));
}

#[test]
fn per_art_ability_requirement_enforced() {
    let mut c = valid_dawn(); // Lore 3, Occult 3
    // Alchemy Adept needs Occult 3 (met) AND Lore 4 (Lore 3 → unmet).
    c.occult_arts.push(art_xp("alchemy", 2));
    let report = c.validate_chargen();
    assert!(
        has_error(&report, |e| matches!(
            e,
            ValidationError::ArtRequirementUnmet {
                required: 4,
                got: 3,
                ..
            }
        )),
        "expected Lore requirement failure, got {:?}",
        report.errors
    );
}

#[test]
fn alchemy_initiate_lore_requirement_met() {
    let mut c = valid_dawn(); // Lore 3 ≥ 2
    c.occult_arts.push(art_xp("alchemy", 1)); // Initiate needs Lore 2
    c.xp_earned = 10;
    c.xp_banked = 0;
    let report = c.validate_chargen();
    assert!(
        !has_error(&report, |e| matches!(
            e,
            ValidationError::ArtRequirementUnmet { .. }
        )),
        "Initiate Alchemy should satisfy Lore 2: {:?}",
        report.errors
    );
}

#[test]
fn unknown_art_id_flagged() {
    let mut c = valid_dawn();
    c.occult_arts.push(art_xp("not-a-real-art", 1));
    let report = c.validate_chargen();
    assert!(has_error(&report, |e| matches!(
        e,
        ValidationError::UnknownArtId { .. }
    )));
}

#[test]
fn procedure_above_occult_floor_flagged() {
    let mut c = valid_dawn();
    // Drop Occult to 2 (also perturbs the ability pool, but we only assert the
    // procedure floor error). A Master-rank Procedure needs Occult 3.
    c.abilities
        .get_mut(&exalted::character::AbilityKind::Occult)
        .unwrap()
        .purchases
        .truncate(2);
    let mut art = OccultArt::lookup("astrology");
    art.procedures.push(Procedure::new(
        "Grand Augury",
        3,
        DotSource::Xp { spent: 1 },
    ));
    c.occult_arts.push(art);
    let report = c.validate_chargen();
    assert!(has_error(&report, |e| matches!(
        e,
        ValidationError::ProcedureOccultTooLow {
            required: 3,
            got: 2,
            ..
        }
    )));
}

#[test]
fn procedure_subsumed_by_degree_is_a_note() {
    let mut c = valid_dawn();
    let mut art = art_xp("astrology", 2); // Adept
    // An Initiate-rank Procedure is covered by the owned Adept Degree.
    art.procedures.push(Procedure::new(
        "Minor Reading",
        1,
        DotSource::Xp { spent: 1 },
    ));
    c.occult_arts.push(art);
    c.xp_earned = 21;
    c.xp_banked = 0;
    let report = c.validate_chargen();
    // Note, not a hard error.
    assert!(has_note(&report, |e| matches!(
        e,
        ValidationError::ProcedureCoveredByDegree { .. }
    )));
    assert!(!has_error(&report, |e| matches!(
        e,
        ValidationError::ProcedureCoveredByDegree { .. }
    )));
}

// --------------------------------------------------------------------------
// Cost-ledger checks (validate_bp / validate_xp)
// --------------------------------------------------------------------------

#[test]
fn wrong_degree_xp_cost_flagged() {
    let mut c = valid_dawn();
    let mut art = OccultArt::lookup("astrology");
    art.rating.add_xp(7); // wrong: should be 10
    c.occult_arts.push(art);
    c.xp_earned = 7;
    c.xp_banked = 0;
    let report = c.validate_xp();
    assert!(has_error(&report, |e| matches!(
        e,
        ValidationError::XpCostWrong {
            paid: 7,
            expected: 10,
            ..
        }
    )));
}

#[test]
fn wrong_procedure_bp_aggregate_flagged() {
    let mut c = valid_dawn();
    let mut art = OccultArt::lookup("astrology");
    // Three BP procedures should aggregate to ceil(3/3) = 1 BP. Pay 2 → wrong.
    art.procedures
        .push(Procedure::new("A", 1, DotSource::BonusPoints { spent: 2 }));
    art.procedures
        .push(Procedure::new("B", 1, DotSource::BonusPoints { spent: 0 }));
    art.procedures
        .push(Procedure::new("C", 1, DotSource::BonusPoints { spent: 0 }));
    c.occult_arts.push(art);
    let report = c.validate_chargen();
    assert!(has_error(&report, |e| matches!(
        e,
        ValidationError::BpCostWrong {
            paid: 2,
            expected: 1,
            ..
        }
    )));
}

// --------------------------------------------------------------------------
// Totals
// --------------------------------------------------------------------------

#[test]
fn arts_count_toward_xp_and_bp_totals() {
    let base = valid_dawn();
    let base_xp = total_xp_spent(&base);
    let base_bp = total_bp_spent(&base);

    let mut c = valid_dawn();
    let mut art = art_xp("astrology", 2); // 2 × 10 XP
    art.procedures
        .push(Procedure::new("Rote", 3, DotSource::Xp { spent: 1 })); // 1 XP
    // Two more procedures bought with BP → ceil(2/3) = 1 BP.
    art.procedures
        .push(Procedure::new("P1", 1, DotSource::BonusPoints { spent: 1 }));
    art.procedures
        .push(Procedure::new("P2", 1, DotSource::BonusPoints { spent: 0 }));
    c.occult_arts.push(art);

    assert_eq!(total_xp_spent(&c), base_xp + 21);
    assert_eq!(total_bp_spent(&c), base_bp + 1);
}

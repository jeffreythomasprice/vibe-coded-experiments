//! Coverage for the per-award XP history: round-trip through TOML,
//! validation that the award sum must match `xp_earned`, and both renderers.

mod common;

use chrono::{DateTime, Utc};
use common::valid_dawn;
use exalted::character::XpAward;
use exalted::error::ValidationError;
use exalted::render::{character_to_markdown, character_to_pdf};

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn award(amount: u32, body: &str, created: &str, updated: &str) -> XpAward {
    XpAward {
        amount,
        body: body.to_string(),
        created_at: parse_ts(created),
        updated_at: parse_ts(updated),
    }
}

#[test]
fn xp_awards_round_trip_through_toml() {
    let mut c = valid_dawn();
    c.xp_earned = 7;
    c.xp_banked = 7;
    c.xp_awards = vec![
        award(
            4,
            "Session 1: base + stunt",
            "2026-04-12T23:30:00Z",
            "2026-04-12T23:30:00Z",
        ),
        award(
            3,
            "Session 2: downtime training",
            "2026-04-19T23:30:00Z",
            "2026-04-20T08:15:00Z",
        ),
    ];

    let text = toml::to_string_pretty(&c).expect("serialize");
    let decoded: exalted::Character = toml::from_str(&text).expect("deserialize");
    assert_eq!(c, decoded);
    assert_eq!(decoded.xp_awards.len(), 2);
    assert_eq!(decoded.xp_awards[0].amount, 4);
}

#[test]
fn empty_xp_awards_skip_field_on_serialize() {
    let c = valid_dawn();
    assert!(c.xp_awards.is_empty());
    let text = toml::to_string_pretty(&c).expect("serialize");
    assert!(
        !text.contains("xp_awards"),
        "empty xp_awards should not be serialized; got:\n{text}"
    );
}

#[test]
fn xp_award_sum_mismatch_caught() {
    let mut c = valid_dawn();
    c.xp_earned = 10;
    c.xp_banked = 10;
    // Awards sum to 9, not 10.
    c.xp_awards = vec![
        award(5, "a", "2026-04-12T23:30:00Z", "2026-04-12T23:30:00Z"),
        award(4, "b", "2026-04-19T23:30:00Z", "2026-04-19T23:30:00Z"),
    ];

    let report = c.validate_xp();
    assert!(
        report.errors.iter().any(|e| matches!(
            e,
            ValidationError::XpAwardSumMismatch { sum: 9, earned: 10 }
        )),
        "expected XpAwardSumMismatch, got: {:?}",
        report.errors
    );
}

#[test]
fn matching_xp_award_sum_validates() {
    let mut c = valid_dawn();
    c.xp_earned = 7;
    c.xp_banked = 7;
    c.xp_awards = vec![
        award(4, "a", "2026-04-12T23:30:00Z", "2026-04-12T23:30:00Z"),
        award(3, "b", "2026-04-19T23:30:00Z", "2026-04-19T23:30:00Z"),
    ];
    let report = c.validate_xp();
    assert!(report.is_ok(), "{:?}", report.errors);
}

#[test]
fn empty_xp_awards_skip_sum_check() {
    // Legacy mode: no awards → no XpAwardSumMismatch even though sum (0) != earned.
    let mut c = valid_dawn();
    c.xp_earned = 5;
    c.xp_banked = 5;
    assert!(c.xp_awards.is_empty());
    let report = c.validate_xp();
    assert!(
        !report
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::XpAwardSumMismatch { .. })),
        "no awards should skip the sum check"
    );
}

#[test]
fn markdown_renders_xp_history_table() {
    let mut c = valid_dawn();
    c.xp_earned = 4;
    c.xp_banked = 4;
    c.xp_awards = vec![award(
        4,
        "Session 1: rescued the orphans",
        "2026-04-12T23:30:00Z",
        "2026-04-12T23:30:00Z",
    )];

    let md = character_to_markdown(&c);
    assert!(md.contains("### XP History"), "missing heading in:\n{md}");
    assert!(md.contains("| 2026-04-12 | 4 | Session 1: rescued the orphans |"));
}

#[test]
fn pdf_appends_experience_page_when_only_awards_present() {
    exalted::rules::database::init_database().ok();
    let mut c = valid_dawn();
    c.xp_earned = 4;
    c.xp_banked = 4;
    c.xp_awards = vec![award(
        4,
        "Session 1: rescued the orphans",
        "2026-04-12T23:30:00Z",
        "2026-04-12T23:30:00Z",
    )];

    // The MrGone template ships with 4 pages; rendering with at least one
    // award must add an appendix page even though no Notes are present.
    let baseline_pages = {
        let mut clean = valid_dawn();
        clean.xp_awards.clear();
        let bytes = character_to_pdf(&clean).expect("render baseline");
        lopdf::Document::load_mem(&bytes)
            .expect("parse baseline pdf")
            .get_pages()
            .len()
    };
    let bytes = character_to_pdf(&c).expect("render");
    let doc = lopdf::Document::load_mem(&bytes).expect("parse rendered pdf");
    assert!(
        doc.get_pages().len() > baseline_pages,
        "expected at least one appendix page beyond baseline ({baseline_pages})"
    );
}

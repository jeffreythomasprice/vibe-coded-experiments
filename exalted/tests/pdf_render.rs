//! Integration tests for the PDF rendering path.

mod common;

use std::path::PathBuf;

use common::valid_dawn;
use exalted::character::{AbilityKind, AttributeKind};
use exalted::render::character_to_pdf;
use exalted::rules::database::init_database;
use lopdf::{Document, Object};

fn render() -> Document {
    init_database().ok();
    let c = valid_dawn();
    let bytes = character_to_pdf(&c).expect("render pdf");
    Document::load_mem(&bytes).expect("output parseable as pdf")
}

fn read_text_field(doc: &Document, name: &str) -> Option<String> {
    let id = find_field(doc, name)?;
    let dict = doc.get_dictionary(id).ok()?;
    match dict.get(b"V").ok()? {
        Object::String(b, _) => std::str::from_utf8(b).ok().map(str::to_string),
        _ => None,
    }
}

fn read_checkbox(doc: &Document, name: &str) -> Option<String> {
    let id = find_field(doc, name)?;
    let dict = doc.get_dictionary(id).ok()?;
    match dict.get(b"V").ok()? {
        Object::Name(n) => std::str::from_utf8(n).ok().map(str::to_string),
        _ => None,
    }
}

fn find_field(doc: &Document, name: &str) -> Option<lopdf::ObjectId> {
    let catalog = doc.catalog().ok()?;
    let form_obj = catalog.get(b"AcroForm").ok()?;
    let form_dict = match form_obj {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    let fields_arr = form_dict.get(b"Fields").ok()?.as_array().ok()?;
    for f in fields_arr {
        if let Ok(id) = f.as_reference() {
            if let Some(found) = find_in_subtree(doc, id, name) {
                return Some(found);
            }
        }
    }
    None
}

fn find_in_subtree(
    doc: &Document,
    id: lopdf::ObjectId,
    target: &str,
) -> Option<lopdf::ObjectId> {
    let dict = doc.get_dictionary(id).ok()?;
    let local = dict.get(b"T").ok().and_then(|o| match o {
        Object::String(b, _) => std::str::from_utf8(b).ok().map(str::to_string),
        _ => None,
    });
    if local.as_deref() == Some(target) {
        return Some(id);
    }
    if let Ok(kids) = dict.get(b"Kids").and_then(|o| o.as_array()) {
        for k in kids {
            if let Ok(kid_id) = k.as_reference() {
                if doc
                    .get_dictionary(kid_id)
                    .map(|d| d.has(b"T"))
                    .unwrap_or(false)
                {
                    if let Some(found) = find_in_subtree(doc, kid_id, target) {
                        return Some(found);
                    }
                }
            }
        }
    }
    None
}

#[test]
fn pdf_renders_and_parses() {
    let doc = render();
    assert!(doc.objects.len() > 0, "rendered PDF has no objects");
}

#[test]
fn pdf_name_field_matches_character() {
    let doc = render();
    let v = read_text_field(&doc, "name").expect("name field set");
    assert_eq!(v, "Test Solar");
}

#[test]
fn pdf_caste_field_matches_character() {
    let doc = render();
    let v = read_text_field(&doc, "caste").expect("caste field set");
    assert_eq!(v, "Dawn");
}

#[test]
fn strength_dots_checked_correctly() {
    let c = valid_dawn();
    let strength = c.attribute(AttributeKind::Strength) as usize;
    let doc = render();
    // dot1..dot5 are Strength.
    for i in 0..5 {
        let field = format!("dot{}", i + 1);
        let v = read_checkbox(&doc, &field).expect("strength dot has value");
        let expected = if i < strength { "Yes" } else { "Off" };
        assert_eq!(v, expected, "{}: expected {}, got {}", field, expected, v);
    }
}

#[test]
fn ability_caste_marks_match_character() {
    let c = valid_dawn();
    let doc = render();
    for (i, kind) in AbilityKind::ALL.iter().enumerate() {
        let field = format!("skillscheck{}", i + 1);
        let v = read_checkbox(&doc, &field).expect("caste mark has value");
        let expected = if c.is_caste_or_favored_ability(*kind) {
            "Yes"
        } else {
            "Off"
        };
        assert_eq!(
            v, expected,
            "{} for {:?}: expected {}, got {}",
            field, kind, expected, v
        );
    }
}

#[test]
fn ability_dots_match_character() {
    let c = valid_dawn();
    let doc = render();
    // Spot-check every ability against the character's actual rating.
    for kind in AbilityKind::ALL {
        let rating = c.ability(*kind) as usize;
        let pos = AbilityKind::ALL.iter().position(|k| *k == *kind).unwrap();
        for dot_idx in 0..5 {
            let dot_n = 46 + pos * 5 + dot_idx;
            let field = format!("dot{}", dot_n);
            let v = read_checkbox(&doc, &field)
                .unwrap_or_else(|| panic!("ability dot {} missing", field));
            let expected = if dot_idx < rating { "Yes" } else { "Off" };
            assert_eq!(
                v, expected,
                "{} (ability {:?} dot {}, rating {}): expected {}, got {}",
                field,
                kind,
                dot_idx + 1,
                rating,
                expected,
                v
            );
        }
    }
}

#[test]
fn pdf_can_be_written_to_disk() {
    let c = valid_dawn();
    init_database().ok();
    let bytes = character_to_pdf(&c).expect("render pdf");
    let tmp = std::env::temp_dir().join("exalted-pdf-render-test.pdf");
    std::fs::write(&tmp, &bytes).expect("write tmp");
    assert!(tmp.exists());
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn no_subcommand_arg_smuggling() {
    // The PDF render path must be reachable via the public API at all
    // — guard against accidental privatization of `character_to_pdf`.
    let _ = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let _: fn(&exalted::Character) -> _ = character_to_pdf;
}

// ---------------------------------------------------------------------------
// Specialty rendering. Each row has a "specialtyN" text label, 5 dot bubbles
// (dot171..dot195 in row order), and a C/F checkbox (skillscheck26..30).
// Duplicate-name specialty entries collapse into a single row whose dot
// count equals the number of entries.
// ---------------------------------------------------------------------------

fn render_with<F: FnOnce(&mut exalted::Character)>(f: F) -> Document {
    init_database().ok();
    let mut c = valid_dawn();
    f(&mut c);
    let bytes = character_to_pdf(&c).expect("render pdf");
    Document::load_mem(&bytes).expect("output parseable")
}

#[test]
fn specialty_row_writes_ability_and_name() {
    let doc = render_with(|c| {
        c.abilities
            .get_mut(&AbilityKind::Melee)
            .unwrap()
            .specialties
            .push(exalted::character::Specialty {
                name: "Sword".to_string(),
                source: exalted::character::DotSource::Xp { spent: 0 },
            });
    });
    let v = read_text_field(&doc, "specialties1").expect("specialty1 written");
    assert_eq!(v, "Melee: Sword");
}

#[test]
fn three_dot_specialty_fills_three_dot_bubbles() {
    let doc = render_with(|c| {
        let melee = c.abilities.get_mut(&AbilityKind::Melee).unwrap();
        for _ in 0..3 {
            melee.specialties.push(exalted::character::Specialty {
                name: "Sword".to_string(),
                source: exalted::character::DotSource::Xp { spent: 0 },
            });
        }
    });
    // Row 0 dot bubbles are dot171..dot175.
    for (i, field) in ["dot171", "dot172", "dot173", "dot174", "dot175"]
        .iter()
        .enumerate()
    {
        let v = read_checkbox(&doc, field).expect("specialty dot has value");
        let expected = if i < 3 { "Yes" } else { "Off" };
        assert_eq!(v, expected, "{}: expected {}, got {}", field, expected, v);
    }
}

#[test]
fn duplicate_specialty_entries_collapse_to_one_row() {
    let doc = render_with(|c| {
        let melee = c.abilities.get_mut(&AbilityKind::Melee).unwrap();
        for _ in 0..3 {
            melee.specialties.push(exalted::character::Specialty {
                name: "Sword".to_string(),
                source: exalted::character::DotSource::Xp { spent: 0 },
            });
        }
        c.abilities
            .get_mut(&AbilityKind::Athletics)
            .unwrap()
            .specialties
            .push(exalted::character::Specialty {
                name: "Running".to_string(),
                source: exalted::character::DotSource::Xp { spent: 0 },
            });
    });
    // Row 1 (specialties2) should be the Athletics one, not another "Sword".
    let v = read_text_field(&doc, "specialties2").expect("specialty2 written");
    assert_eq!(v, "Athletics: Running");
}

#[test]
fn specialty_caste_mark_tracks_parent_ability() {
    // Melee is a Dawn caste ability for valid_dawn — its specialty row
    // should have the C/F box ticked. Lore is neither caste nor favored —
    // its specialty row should not.
    let doc = render_with(|c| {
        c.abilities
            .get_mut(&AbilityKind::Melee)
            .unwrap()
            .specialties
            .push(exalted::character::Specialty {
                name: "Sword".to_string(),
                source: exalted::character::DotSource::Xp { spent: 0 },
            });
        c.abilities
            .get_mut(&AbilityKind::Lore)
            .unwrap()
            .specialties
            .push(exalted::character::Specialty {
                name: "Ancient".to_string(),
                source: exalted::character::DotSource::Xp { spent: 0 },
            });
    });
    // Iteration order of c.abilities is by AbilityKind::ALL, which puts
    // Melee (Dawn) before Lore (Twilight), so row 0 = Melee, row 1 = Lore.
    let melee_row = read_checkbox(&doc, "skillscheck26").expect("row 1 C/F box");
    assert_eq!(melee_row, "Yes", "Melee specialty should be C/F-marked");
    let lore_row = read_checkbox(&doc, "skillscheck27").expect("row 2 C/F box");
    assert_eq!(lore_row, "Off", "Lore specialty should not be C/F-marked");
}

// ---------------------------------------------------------------------------
// Health track. Slots are grouped by penalty: -0 is `healthcheck1..5`,
// -1 is `healthcheck6..10`, -2 is `healthcheck11..20`, -4 is `healthcheck21`,
// Incap is `healthcheck22`. Each bucket holds the default level(s) at the
// low end and reserves the rest for Ox-Body Technique purchases.
// ---------------------------------------------------------------------------

fn assert_health_checks(doc: &Document, expected_checked: &[&str]) {
    use std::collections::HashSet;
    let want: HashSet<&str> = expected_checked.iter().copied().collect();
    for n in 1..=22 {
        let field = format!("healthcheck{}", n);
        let v = read_checkbox(doc, &field).unwrap_or_else(|| {
            panic!("{} missing from rendered PDF", field);
        });
        let expected = if want.contains(field.as_str()) { "Yes" } else { "Off" };
        assert_eq!(
            v, expected,
            "{}: expected {}, got {}", field, expected, v
        );
    }
}

#[test]
fn health_track_no_ox_body_three_damage() {
    // Base track: -0, -1, -1, -2, -2, -4, Incap. With 3 damage the first
    // three wound levels (the -0 and the two -1s) should be checked.
    let doc = render_with(|c| {
        c.pool_state.health_damage.bashing = 3;
    });
    assert_health_checks(&doc, &["healthcheck1", "healthcheck6", "healthcheck7"]);
}

#[test]
fn health_track_no_ox_body_overflows_into_incap() {
    // 8 damage > 7 base levels: fills every base wound row plus Incap.
    // The 8th point of damage corresponds to the first Dying row, which
    // has no PDF slot.
    let doc = render_with(|c| {
        c.pool_state.health_damage.bashing = 8;
    });
    assert_health_checks(
        &doc,
        &[
            "healthcheck1",  // -0
            "healthcheck6",  // -1
            "healthcheck7",  // -1
            "healthcheck11", // -2
            "healthcheck12", // -2
            "healthcheck21", // -4
            "healthcheck22", // Incap
        ],
    );
}

#[test]
fn health_track_ox_body_one_zero_extends_minus_zero_row() {
    // One OneZero Ox-Body purchase adds a second -0 level. With 2 damage
    // both -0 slots should be checked and the -1 row should stay empty.
    use exalted::character::{CharmRef, DotSource};
    use exalted::rules::health::OxBodyPattern;

    let doc = render_with(|c| {
        // Resistance must be ≥ 1 to permit one Ox-Body purchase.
        c.abilities
            .get_mut(&AbilityKind::Resistance)
            .unwrap()
            .add_chargen();
        c.charms.push(CharmRef::Lookup {
            id: "ox-body-technique".to_string(),
            source: DotSource::BonusPoints { spent: 4 },
            non_solar: false,
            notes: None,
            ox_body_pattern: Some(OxBodyPattern::OneZero),
        });
        c.pool_state.health_damage.bashing = 2;
    });
    assert_health_checks(&doc, &["healthcheck1", "healthcheck2"]);
}

#[test]
fn health_track_ox_body_minus_one_two_minus_two_routes_by_penalty() {
    // OneMinusOneTwoMinusTwo adds one -1 and two -2 levels. With 5 damage
    // the order is: -0, -1, -1, -1 (ox-body extra), -2 — i.e. the
    // ox-body -1 fills the third -1 slot (healthcheck8) and the first
    // damage in the -2 row hits healthcheck11.
    use exalted::character::{CharmRef, DotSource};
    use exalted::rules::health::OxBodyPattern;

    let doc = render_with(|c| {
        c.abilities
            .get_mut(&AbilityKind::Resistance)
            .unwrap()
            .add_chargen();
        c.charms.push(CharmRef::Lookup {
            id: "ox-body-technique".to_string(),
            source: DotSource::BonusPoints { spent: 4 },
            non_solar: false,
            notes: None,
            ox_body_pattern: Some(OxBodyPattern::OneMinusOneTwoMinusTwo),
        });
        c.pool_state.health_damage.bashing = 5;
    });
    assert_health_checks(
        &doc,
        &[
            "healthcheck1",  // -0
            "healthcheck6",  // base -1
            "healthcheck7",  // base -1
            "healthcheck8",  // ox-body -1
            "healthcheck11", // base -2
        ],
    );
}

//! Integration tests for the PDF rendering path.

mod common;

use std::path::PathBuf;

use common::valid_dawn;
use exalted::character::{AbilityKind, AttributeKind, KnownLanguage, LanguageFamily};
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

/// Abilities in the order the MrGone PDF lays them out (column-major by
/// caste pair). This is NOT the same as `AbilityKind::ALL` — the sheet
/// groups Dawn+Night in column 1, Zenith+Eclipse in column 2, Twilight
/// alone in column 3, whereas `ALL` orders castes Dawn, Zenith, Twilight,
/// Night, Eclipse. Field numbering (`skillscheck1..25`, `dot46..170`)
/// follows the column-major layout.
const PDF_ABILITY_ORDER: [AbilityKind; 25] = [
    AbilityKind::Archery,
    AbilityKind::MartialArts,
    AbilityKind::Melee,
    AbilityKind::Thrown,
    AbilityKind::War,
    AbilityKind::Athletics,
    AbilityKind::Awareness,
    AbilityKind::Dodge,
    AbilityKind::Larceny,
    AbilityKind::Stealth,
    AbilityKind::Integrity,
    AbilityKind::Performance,
    AbilityKind::Presence,
    AbilityKind::Resistance,
    AbilityKind::Survival,
    AbilityKind::Bureaucracy,
    AbilityKind::Linguistics,
    AbilityKind::Ride,
    AbilityKind::Sail,
    AbilityKind::Socialize,
    AbilityKind::Craft,
    AbilityKind::Investigation,
    AbilityKind::Lore,
    AbilityKind::Medicine,
    AbilityKind::Occult,
];

#[test]
fn ability_caste_marks_match_character() {
    let c = valid_dawn();
    let doc = render();
    for (i, kind) in PDF_ABILITY_ORDER.iter().enumerate() {
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
    for (pos, kind) in PDF_ABILITY_ORDER.iter().enumerate() {
        let rating = c.ability(*kind) as usize;
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

// ---------------------------------------------------------------------------
// Languages. Each row has a "languagesN" text field and an "LCheckN"
// checkbox. The checkbox is ticked iff the language in slot N is the
// character's native tongue (analogous to the C/F mark on an ability row).
// ---------------------------------------------------------------------------

fn assert_only_native_checked(doc: &Document, native_slot: usize) {
    for n in 1..=15 {
        let field = format!("LCheck{}", n);
        let v = read_checkbox(doc, &field)
            .unwrap_or_else(|| panic!("{} missing from rendered PDF", field));
        let expected = if n == native_slot { "Yes" } else { "Off" };
        assert_eq!(
            v, expected,
            "{}: expected {}, got {}",
            field, expected, v
        );
    }
}

#[test]
fn language_native_checkbox_marks_only_native_slot() {
    // valid_dawn has exactly one language (Riverspeak) marked native in
    // slot 1. LCheck1 should be ticked; LCheck2..15 should be clear.
    let doc = render();
    assert_only_native_checked(&doc, 1);
}

#[test]
fn language_text_no_longer_appends_native_suffix() {
    // The native flag is now conveyed by LCheck, so the text field for
    // the native language must not contain the legacy "(native)" suffix.
    let doc = render();
    let v = read_text_field(&doc, "languages1").expect("languages1 written");
    assert!(
        !v.contains("(native)"),
        "languages1 should not contain '(native)' suffix, got: {:?}",
        v
    );
}

#[test]
fn language_native_checkbox_tracks_native_at_non_first_slot() {
    // Native language can be at any slot; ordering in c.languages is
    // preserved by the renderer. Put native at slot 3 and confirm only
    // LCheck3 is ticked.
    let doc = render_with(|c| {
        c.languages = vec![
            KnownLanguage {
                family: LanguageFamily::HighRealm,
                dialect_specialty: None,
                native: false,
            },
            KnownLanguage {
                family: LanguageFamily::OldRealm,
                dialect_specialty: None,
                native: false,
            },
            KnownLanguage {
                family: LanguageFamily::Riverspeak,
                dialect_specialty: Some("Nexus".to_string()),
                native: true,
            },
        ];
    });
    assert_only_native_checked(&doc, 3);
}

// ---------------------------------------------------------------------------
// Page-4 spillover: background slots 9–18 use `backgrounds9..18` text fields
// and `e2dot1..e2dot50` dot fields; specialty rows 6–15 use `specx1..specx10`
// text fields and `e2dot51..e2dot100` dot fields.
// ---------------------------------------------------------------------------

#[test]
fn background_9_spills_to_page4_dot_and_text_fields() {
    use exalted::character::{BackgroundKind, BackgroundRef, RatedTrait};

    let doc = render_with(|c| {
        // valid_dawn starts with a few backgrounds; push enough total so a
        // 9th exists. We use a known background with a 3-dot rating so we
        // can assert against a specific dot count.
        while c.backgrounds.len() < 8 {
            c.backgrounds.push(BackgroundRef::lookup_kind(
                BackgroundKind::Resources,
                RatedTrait::with_base(1),
            ));
        }
        c.backgrounds.push(
            BackgroundRef::lookup_kind(BackgroundKind::Allies, RatedTrait::with_base(3))
                .with_label("Page 4 ally"),
        );
    });

    // 9th background → `backgrounds9` text + `e2dot1..e2dot5` dots.
    let label = read_text_field(&doc, "backgrounds9").expect("backgrounds9 written");
    assert!(
        label.contains("Page 4 ally"),
        "backgrounds9 should include label, got: {:?}",
        label
    );
    for (i, field) in ["e2dot1", "e2dot2", "e2dot3", "e2dot4", "e2dot5"]
        .iter()
        .enumerate()
    {
        let v = read_checkbox(&doc, field).expect("e2dot has value");
        let expected = if i < 3 { "Yes" } else { "Off" };
        assert_eq!(v, expected, "{}: expected {}, got {}", field, expected, v);
    }
}

#[test]
fn specialty_row_6_uses_page4_specx_and_e2dot51() {
    use exalted::character::{DotSource, Specialty};

    // Push enough specialties across distinct abilities so row 6 is filled.
    // `super::specialties::rows()` walks AbilityKind::ALL and yields one
    // row per (ability, name); 6 different abilities with one specialty
    // each gives us 6 rows.
    let doc = render_with(|c| {
        let abilities = [
            AbilityKind::Archery,
            AbilityKind::MartialArts,
            AbilityKind::Melee,
            AbilityKind::Thrown,
            AbilityKind::War,
            AbilityKind::Athletics,
        ];
        for (i, kind) in abilities.iter().enumerate() {
            c.abilities.get_mut(kind).unwrap().specialties.push(Specialty {
                name: format!("Spec{}", i + 1),
                source: DotSource::Xp { spent: 0 },
            });
        }
    });

    // The 6th row should land in specx1 + e2dot51..e2dot55.
    let text = read_text_field(&doc, "specx1").expect("specx1 written");
    assert!(
        text.contains("Athletics"),
        "specx1 should describe the 6th specialty (Athletics: Spec6), got: {:?}",
        text
    );
    for (i, field) in ["e2dot51", "e2dot52", "e2dot53", "e2dot54", "e2dot55"]
        .iter()
        .enumerate()
    {
        let v = read_checkbox(&doc, field).expect("e2dot specialty dot has value");
        let expected = if i < 1 { "Yes" } else { "Off" };
        assert_eq!(v, expected, "{}: expected {}, got {}", field, expected, v);
    }
}

// ---------------------------------------------------------------------------
// Intimacy rating dots (Idot1..Idot100, 10 rows × 10 dots).
// ---------------------------------------------------------------------------

#[test]
fn intimacy_rating_fills_idot_row() {
    use exalted::character::{DotSource, Intimacy, IntimacyKind};

    let doc = render_with(|c| {
        c.intimacies.clear();
        c.intimacies.push(Intimacy {
            description: "Test target".to_string(),
            kind: IntimacyKind::Cause,
            source: DotSource::Base,
            rating: 4,
        });
    });

    for (i, field) in [
        "Idot1", "Idot2", "Idot3", "Idot4", "Idot5", "Idot6", "Idot7", "Idot8", "Idot9", "Idot10",
    ]
    .iter()
    .enumerate()
    {
        let v = read_checkbox(&doc, field).expect("Idot has value");
        let expected = if i < 4 { "Yes" } else { "Off" };
        assert_eq!(v, expected, "{}: expected {}, got {}", field, expected, v);
    }
}

// ---------------------------------------------------------------------------
// Familiar — name in `fam1`, total damage drives `Fcheck1..Fcheck30`.
// ---------------------------------------------------------------------------

#[test]
fn familiar_damage_fills_fcheck_track() {
    use exalted::character::{state::HealthDamage, Familiar};

    let doc = render_with(|c| {
        c.familiar = Some(Familiar {
            name: "Sparrow".to_string(),
            health_damage: HealthDamage {
                bashing: 2,
                lethal: 1,
                aggravated: 0,
            },
        });
    });

    let name = read_text_field(&doc, "fam1").expect("fam1 written");
    assert_eq!(name, "Sparrow");

    // 3 total points of damage → Fcheck1..Fcheck3 Yes, rest Off.
    for n in 1..=30 {
        let field = format!("Fcheck{}", n);
        let v = read_checkbox(&doc, &field).expect("Fcheck has value");
        let expected = if n <= 3 { "Yes" } else { "Off" };
        assert_eq!(v, expected, "{}: expected {}, got {}", field, expected, v);
    }
}

#[test]
fn no_familiar_clears_fcheck_track() {
    // valid_dawn has no familiar; every Fcheck box should be unchecked.
    let doc = render();
    for n in 1..=30 {
        let field = format!("Fcheck{}", n);
        let v = read_checkbox(&doc, &field).expect("Fcheck has value");
        assert_eq!(v, "Off", "{}: expected Off, got {}", field, v);
    }
}

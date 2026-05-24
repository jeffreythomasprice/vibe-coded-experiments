//! Integration tests for the PDF rendering path.

mod common;

use std::path::PathBuf;
use std::sync::OnceLock;

use common::valid_dawn;
use exalted::character::{AbilityKind, AttributeKind, KnownLanguage, LanguageFamily};
use exalted::render::character_to_pdf;
use exalted::rules::database::init_database;
use lopdf::{Document, Object};

/// PDF bytes for the unmodified `valid_dawn` character. Rendered once and
/// shared across every test that needs the baseline — the render+serialize
/// pipeline is the single most expensive thing this test file does.
fn baseline_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        init_database().ok();
        let c = valid_dawn();
        character_to_pdf(&c).expect("render pdf")
    })
}

/// Parsed `Document` for the unmodified `valid_dawn` baseline. The read
/// helpers (`read_text_field`, `read_checkbox`, `find_field`) only inspect
/// the document, so sharing a single `&'static Document` across tests is safe.
fn baseline_doc() -> &'static Document {
    static DOC: OnceLock<Document> = OnceLock::new();
    DOC.get_or_init(|| Document::load_mem(baseline_bytes()).expect("output parseable as pdf"))
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

fn find_in_subtree(doc: &Document, id: lopdf::ObjectId, target: &str) -> Option<lopdf::ObjectId> {
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
    let doc = baseline_doc();
    assert!(doc.objects.len() > 0, "rendered PDF has no objects");
}

#[test]
fn pdf_name_field_matches_character() {
    let doc = baseline_doc();
    let v = read_text_field(doc, "name").expect("name field set");
    assert_eq!(v, "Test Solar");
}

#[test]
fn pdf_caste_field_matches_character() {
    let doc = baseline_doc();
    let v = read_text_field(doc, "caste").expect("caste field set");
    assert_eq!(v, "Dawn");
}

#[test]
fn strength_dots_checked_correctly() {
    let c = valid_dawn();
    let strength = c.attribute(AttributeKind::Strength) as usize;
    let doc = baseline_doc();
    // dot1..dot5 are Strength.
    for i in 0..5 {
        let field = format!("dot{}", i + 1);
        let v = read_checkbox(doc, &field).expect("strength dot has value");
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
    let doc = baseline_doc();
    for (i, kind) in PDF_ABILITY_ORDER.iter().enumerate() {
        let field = format!("skillscheck{}", i + 1);
        let v = read_checkbox(doc, &field).expect("caste mark has value");
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
    let doc = baseline_doc();
    for (pos, kind) in PDF_ABILITY_ORDER.iter().enumerate() {
        let rating = c.ability(*kind) as usize;
        for dot_idx in 0..5 {
            let dot_n = 46 + pos * 5 + dot_idx;
            let field = format!("dot{}", dot_n);
            let v = read_checkbox(doc, &field)
                .unwrap_or_else(|| panic!("ability dot {} missing", field));
            let expected = if dot_idx < rating { "Yes" } else { "Off" };
            assert_eq!(
                v,
                expected,
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

/// The renderer must bake an appearance stream for every text field it
/// writes — viewers like pdf.js / Firefox don't honor `/NeedAppearances`
/// and would otherwise show the field blank. Spot-check the `name` widget:
/// it should have an `/AP /N` Form XObject whose content stream contains
/// the literal text we wrote.
#[test]
fn text_field_has_baked_appearance_stream() {
    let doc = baseline_doc();
    let widget_id = find_field(doc, "name").expect("name field exists");
    let widget = doc.get_dictionary(widget_id).expect("widget dict");
    let ap = widget
        .get(b"AP")
        .expect("widget has /AP")
        .as_dict()
        .expect("/AP is a dict");
    let n_ref = ap.get(b"N").expect("/AP has /N");
    let xobject_id = n_ref.as_reference().expect("/N is an indirect ref");
    let stream = doc
        .get_object(xobject_id)
        .expect("xobject exists")
        .as_stream()
        .expect("xobject is a stream");
    let content = std::str::from_utf8(&stream.content).expect("ascii content");
    assert!(
        content.contains("Test Solar"),
        "appearance stream should contain the field value, got: {:?}",
        content
    );
}

#[test]
fn pdf_can_be_written_to_disk() {
    let bytes = baseline_bytes();
    let tmp = std::env::temp_dir().join("exalted-pdf-render-test.pdf");
    std::fs::write(&tmp, bytes).expect("write tmp");
    assert!(tmp.exists());
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn page4_athletics_boxes_filled() {
    // valid_dawn → Str 4, Dex 5 (chargen 4 + BP 1), Athletics 3, no armor, full HP.
    //   Move            = Dex (5)
    //   Dash            = Dex + 6 (11)
    //   Jump vertical   = Str + Ath (7)
    //   Jump horizontal = vert × 2 (14)
    //   Lift            = (Str + Ath = 7) → 650 lbs
    let doc = baseline_doc();
    assert_eq!(read_text_field(doc, "Mo").as_deref(), Some("5"));
    assert_eq!(read_text_field(doc, "DA").as_deref(), Some("11"));
    assert_eq!(read_text_field(doc, "VJ").as_deref(), Some("7"));
    assert_eq!(read_text_field(doc, "HJ").as_deref(), Some("14"));
    assert_eq!(read_text_field(doc, "LI").as_deref(), Some("650"));
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
        let expected = if want.contains(field.as_str()) {
            "Yes"
        } else {
            "Off"
        };
        assert_eq!(v, expected, "{}: expected {}, got {}", field, expected, v);
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
            notes: Vec::new(),
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
            notes: Vec::new(),
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
        assert_eq!(v, expected, "{}: expected {}, got {}", field, expected, v);
    }
}

#[test]
fn language_native_checkbox_marks_only_native_slot() {
    // valid_dawn has exactly one language (Riverspeak) marked native in
    // slot 1. LCheck1 should be ticked; LCheck2..15 should be clear.
    let doc = baseline_doc();
    assert_only_native_checked(doc, 1);
}

#[test]
fn language_text_no_longer_appends_native_suffix() {
    // The native flag is now conveyed by LCheck, so the text field for
    // the native language must not contain the legacy "(native)" suffix.
    let doc = baseline_doc();
    let v = read_text_field(doc, "languages1").expect("languages1 written");
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
            c.abilities
                .get_mut(kind)
                .unwrap()
                .specialties
                .push(Specialty {
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
    use exalted::character::{Familiar, state::HealthDamage};

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

// ---------------------------------------------------------------------------
// Essence dots 7–10. The MrGone template only has six dot widgets, so dots
// 7–10 are drawn as overlay content (a second row below the original six).
// We assert by inspecting the appended page content stream, which carries a
// `% exalted-essence-overflow` marker and four circle paths (filled with `f`
// or stroked with `S`).
// ---------------------------------------------------------------------------

fn essence_overflow_stream_content(doc: &Document) -> String {
    // Scan every stream object in the document and concatenate any that
    // carry the overflow marker comment. The overlay is appended as an
    // uncompressed stream (no `/Filter`), so we can read it byte-for-byte
    // without decoding.
    let mut combined = String::new();
    for (_, obj) in &doc.objects {
        if let Object::Stream(s) = obj {
            // Try raw bytes first; fall back to decompressed for streams
            // that picked up a /Filter on save.
            let bytes = if let Ok(text) = std::str::from_utf8(&s.content) {
                if text.contains("exalted-essence-overflow") {
                    Some(s.content.clone())
                } else {
                    None
                }
            } else {
                None
            };
            let bytes = bytes.or_else(|| s.decompressed_content().ok());
            if let Some(b) = bytes {
                if let Ok(text) = std::str::from_utf8(&b) {
                    if text.contains("exalted-essence-overflow") {
                        combined.push_str(text);
                    }
                }
            }
        }
    }
    combined
}

#[test]
fn essence_overflow_fills_dots_7_and_8_when_rating_is_8() {
    use exalted::character::RatedTrait;
    let doc = render_with(|c| {
        c.essence = RatedTrait::with_base(8);
    });
    let s = essence_overflow_stream_content(&doc);
    assert!(
        s.contains("% exalted-essence-overflow"),
        "overlay stream missing marker at Essence 8, got: {:?}",
        s
    );
    // 4 dots in the overflow row → 4 circle paths, each terminated by `f`
    // (filled) or `S` (stroked). With Essence 8, dots 7 and 8 are filled
    // and 9 and 10 are stroked.
    let fills = s.matches("\nf\n").count();
    let strokes = s.matches("\nS\n").count();
    assert_eq!(
        fills, 2,
        "expected 2 filled overflow dots, got {} (stream: {:?})",
        fills, s
    );
    assert_eq!(
        strokes, 2,
        "expected 2 stroked overflow dots, got {} (stream: {:?})",
        strokes, s
    );
}

#[test]
fn essence_overflow_not_drawn_when_rating_below_7() {
    // valid_dawn has Essence 2 — none of dots 7–10 would be filled, so
    // we don't render the overflow row at all.
    let doc = baseline_doc();
    let s = essence_overflow_stream_content(doc);
    assert!(
        s.is_empty(),
        "expected no overflow stream below rating 7, got: {:?}",
        s
    );
}

#[test]
fn essence_overflow_drawn_at_exactly_rating_7() {
    // Boundary: rating 7 fills dot 7 and leaves 8–10 empty.
    use exalted::character::RatedTrait;
    let doc = render_with(|c| {
        c.essence = RatedTrait::with_base(7);
    });
    let s = essence_overflow_stream_content(&doc);
    let fills = s.matches("\nf\n").count();
    let strokes = s.matches("\nS\n").count();
    assert_eq!(
        fills, 1,
        "expected 1 filled overflow dot at Essence 7, got {}",
        fills
    );
    assert_eq!(
        strokes, 3,
        "expected 3 stroked overflow dots at Essence 7, got {}",
        strokes
    );
}

#[test]
fn no_familiar_clears_fcheck_track() {
    // valid_dawn has no familiar; every Fcheck box should be unchecked.
    let doc = baseline_doc();
    for n in 1..=30 {
        let field = format!("Fcheck{}", n);
        let v = read_checkbox(doc, &field).expect("Fcheck has value");
        assert_eq!(v, "Off", "{}: expected Off, got {}", field, v);
    }
}

// ---------------------------------------------------------------------------
// CHARMS table: each row populates 5 columns
// (NAME | TYPE | DURATION | COST | EFFECT).
//
// Row N (1..14) maps to fields:
//   NAME      = charms/sorceryN
//   TYPE      = charms/sorcery(N+14)
//   DURATION  = charms/sorcery(N+28)
//   COST      = charms/sorcery(N+42)
//   EFFECT    = charms/sorcery(N+56)
// ---------------------------------------------------------------------------

#[test]
fn charm_row_fills_all_five_columns() {
    use exalted::character::{CharmRef, DotSource};
    use exalted::rules::database::{CharmEntry, CharmType};
    use std::collections::BTreeMap;

    // Use a Custom entry so the test is fully self-contained — no dependency
    // on the bulk-authored effect strings in charms.toml.
    let custom = CharmEntry {
        id: "test-render-charm".to_string(),
        name: "Test Render Charm".to_string(),
        exalt_type: "solar".to_string(),
        ability: "archery".to_string(),
        cost: "3m, 1wp".to_string(),
        mins_ability: 1,
        mins_essence: 1,
        charm_type: CharmType::Supplemental,
        type_detail: "".to_string(),
        keywords: vec![],
        duration: "Instant".to_string(),
        prerequisites: vec![],
        mins_attribute: BTreeMap::new(),
        source: "test".to_string(),
        pages: "1".to_string(),
        effect: "Test effect summary".to_string(),
        description: "test".to_string(),
    };
    let doc = render_with(|c| {
        c.charms.clear();
        c.charms.push(CharmRef::Custom {
            entry: custom,
            source: DotSource::BonusPoints { spent: 4 },
            non_solar: false,
            notes: Vec::new(),
            ox_body_pattern: None,
        });
    });

    assert_eq!(
        read_text_field(&doc, "charms/sorcery1").as_deref(),
        Some("Test Render Charm"),
        "NAME column"
    );
    assert_eq!(
        read_text_field(&doc, "charms/sorcery15").as_deref(),
        Some("Supplemental"),
        "TYPE column"
    );
    assert_eq!(
        read_text_field(&doc, "charms/sorcery29").as_deref(),
        Some("Instant"),
        "DURATION column"
    );
    assert_eq!(
        read_text_field(&doc, "charms/sorcery43").as_deref(),
        Some("3m, 1wp"),
        "COST column"
    );
    assert_eq!(
        read_text_field(&doc, "charms/sorcery57").as_deref(),
        Some("Test effect summary"),
        "EFFECT column"
    );
}

// ---------------------------------------------------------------------------
// SORCERY table (page 3): spells write to `sox{N}` fields. Row 1 uses N=1,
// with NAME=sox1, DURATION=sox11, COST=sox21, EFFECT=sox31. Spells must NOT
// leak into the CHARMS table on page 2, nor into the page-2 5-row stub that
// uses `charms/sorcery{M}x` (those rows visually look like a charms overflow
// and aren't the labeled SORCERY table).
// ---------------------------------------------------------------------------

#[test]
fn spell_row_writes_to_sorcery_table_not_charms_table() {
    use exalted::character::{DotSource, SpellCircle, SpellRef};
    use exalted::rules::database::SpellEntry;

    let custom = SpellEntry {
        id: "test-render-spell".to_string(),
        name: "Test Render Spell".to_string(),
        circle: SpellCircle::Terrestrial,
        cost: "15m".to_string(),
        keywords: vec![],
        duration: "One scene".to_string(),
        target: "Caster".to_string(),
        source: "test".to_string(),
        pages: "1".to_string(),
        effect: "Spell effect summary".to_string(),
        description: "test".to_string(),
    };
    let doc = render_with(|c| {
        c.charms.clear();
        c.spells.push(SpellRef::Custom {
            entry: custom,
            source: DotSource::BonusPoints { spent: 7 },
            notes: Vec::new(),
        });
    });

    assert_eq!(
        read_text_field(&doc, "sox1").as_deref(),
        Some("Test Render Spell"),
        "SORCERY NAME column"
    );
    assert_eq!(
        read_text_field(&doc, "sox11").as_deref(),
        Some("One scene"),
        "SORCERY DURATION column"
    );
    assert_eq!(
        read_text_field(&doc, "sox21").as_deref(),
        Some("15m"),
        "SORCERY COST column"
    );
    assert_eq!(
        read_text_field(&doc, "sox31").as_deref(),
        Some("Spell effect summary"),
        "SORCERY EFFECT column"
    );

    let charms_row1 = read_text_field(&doc, "charms/sorcery1").unwrap_or_default();
    assert!(
        !charms_row1.contains("Test Render Spell"),
        "spell name leaked into CHARMS NAME row 1: {:?}",
        charms_row1
    );
    let charms_stub_row1 = read_text_field(&doc, "charms/sorcery10x").unwrap_or_default();
    assert!(
        !charms_stub_row1.contains("Test Render Spell"),
        "spell name leaked into page-2 charms stub: {:?}",
        charms_stub_row1
    );
}

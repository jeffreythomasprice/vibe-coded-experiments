//! Fill named text fields on the MrGone sheet from a `Character`.
//!
//! See `field_map.rs` for the dot/checkbox half. This module handles fields
//! that the sheet has given a meaningful name to: identity (`name`, `caste`,
//! `concept`, `MOTIVATION`, `anima`), attribute/ability values
//! (`attributes1`–`9`, `skills1`–`25`), defensive stats (`soak1`–`3`,
//! `dv1`–`3`), weapon lines, charm/spell lines, backgrounds, intimacies,
//! languages, and XP.

use lopdf::Document;

use crate::character::xp::total_xp_spent;
use crate::character::{
    AbilityKind, AttributeKind, BackgroundRef, Character, KnownLanguage, LanguageFamily, Weapon,
};
use crate::rules::database::{database, RulesDatabase};
use crate::rules::defense::{
    dodge_dv, join_battle, mdv_dodge, parry_dv, soak_aggravated, soak_bashing, soak_lethal,
};
use crate::rules::derived::{lift_lbs, movement};
use crate::rules::essence::{personal_essence_max, peripheral_essence_max};

use super::acroform::{set_text_field, FieldIndex};
use super::field_map;
use super::super::names::{ability_name, caste_name, intimacy_kind};
use super::PdfRenderError;

pub(super) fn fill(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    fill_identity(doc, index, c)?;
    fill_attributes(doc, index, c)?;
    fill_abilities(doc, index, c)?;
    fill_specialties(doc, index, c)?;
    fill_backgrounds(doc, index, c)?;
    fill_intimacies(doc, index, c)?;
    fill_languages(doc, index, c)?;
    fill_weapons(doc, index, c)?;
    fill_armor_and_defense(doc, index, c)?;
    fill_charms_and_spells(doc, index, c)?;
    fill_combos(doc, index, c)?;
    fill_xp_and_essence(doc, index, c)?;
    fill_virtue_flaw(doc, index, c)?;
    fill_familiar(doc, index, c)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

fn fill_identity(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    let id = &c.identity;
    write(doc, index, "name", &id.name)?;
    write(doc, index, "caste", caste_name(c.caste))?;
    write(doc, index, "concept", &id.concept)?;
    write(doc, index, "MOTIVATION", &id.motivation)?;
    write(doc, index, "anima", &id.anima.totem)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Attributes (numeric)
// ---------------------------------------------------------------------------

fn fill_attributes(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    // Sheet ordering matches `AttributeKind::ALL`:
    //   1=Str, 2=Dex, 3=Sta, 4=Cha, 5=Man, 6=App, 7=Per, 8=Int, 9=Wits
    for (i, kind) in AttributeKind::ALL.iter().enumerate() {
        let field = format!("attributes{}", i + 1);
        write(doc, index, &field, &c.attribute(*kind).to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Abilities (numeric values + caste/favored marks set elsewhere)
// ---------------------------------------------------------------------------

fn fill_abilities(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for (i, kind) in AbilityKind::ALL.iter().enumerate() {
        let field = format!("skills{}", i + 1);
        write(doc, index, &field, &c.ability(*kind).to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Specialties — at most 5 rows; duplicate-name entries are aggregated into
// a single row whose rating is the count of those entries (the row's dot
// bubbles are filled in `dots.rs`).
// ---------------------------------------------------------------------------

fn fill_specialties(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for (i, row) in super::specialties::rows(c).iter().enumerate() {
        let Some(field) = field_map::specialty_text_field(i) else {
            break;
        };
        let text = format!("{}: {}", ability_name(row.ability), row.name);
        write(doc, index, &field, &text)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Backgrounds
// ---------------------------------------------------------------------------

fn fill_backgrounds(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    // 18 background slots total (8 on page 1, 10 on page 4). Dots are
    // handled in `dots.rs`; text labels are `backgrounds1`..`backgrounds18`.
    let db = database();
    let mut overflow = 0usize;
    for (i, bg) in c.backgrounds.iter().enumerate() {
        let Some(field) = field_map::background_text_field(i) else {
            overflow += 1;
            continue;
        };
        let label = format_background_label(bg, db);
        if index.has(&field) {
            write(doc, index, &field, &label)?;
        }
    }
    if overflow > 0 {
        eprintln!(
            "warning: {} background label(s) beyond slot 18 not rendered",
            overflow
        );
    }
    Ok(())
}

fn format_background_label(bg: &BackgroundRef, db: &RulesDatabase) -> String {
    if bg.label().is_empty() {
        bg.display_name(db).to_string()
    } else {
        format!("{} ({})", bg.display_name(db), bg.label())
    }
}

// ---------------------------------------------------------------------------
// Intimacies
// ---------------------------------------------------------------------------

fn fill_intimacies(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    // The sheet has 10 named intimacy slots.
    for (i, intimacy) in c.intimacies.iter().take(10).enumerate() {
        let text = if matches!(intimacy.kind, crate::character::IntimacyKind::Other) {
            intimacy.description.clone()
        } else {
            format!("{} [{}]", intimacy.description, intimacy_kind(intimacy.kind))
        };
        let field = format!("intimacies{}", i + 1);
        if index.has(&field) {
            write(doc, index, &field, &text)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Languages
// ---------------------------------------------------------------------------

fn fill_languages(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for (i, lang) in c.languages.iter().take(15).enumerate() {
        let text = format_language(lang);
        let field = format!("languages{}", i + 1);
        if index.has(&field) {
            write(doc, index, &field, &text)?;
        }
    }
    Ok(())
}

fn format_language(l: &KnownLanguage) -> String {
    let family = match &l.family {
        LanguageFamily::TribalTongue(name) => format!("Tribal Tongue: {}", name),
        other => format!("{:?}", other),
    };
    let dialect = l
        .dialect_specialty
        .as_ref()
        .map(|d| format!(" — {}", d))
        .unwrap_or_default();
    format!("{}{}", family, dialect)
}

// ---------------------------------------------------------------------------
// Weapons
// ---------------------------------------------------------------------------

fn fill_weapons(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for (i, w) in c.equipment.weapons.iter().take(4).enumerate() {
        let field = format!("weapons{}", i + 1);
        if index.has(&field) {
            write(doc, index, &field, &format_weapon_line(w))?;
        }
    }
    Ok(())
}

fn format_weapon_line(w: &Weapon) -> String {
    let dmg_type = match w.damage_type {
        crate::character::equipment::DamageType::Bashing => "B",
        crate::character::equipment::DamageType::Lethal => "L",
        crate::character::equipment::DamageType::Aggravated => "A",
    };
    format!(
        "{} (Spd {:+} / Acc {:+} / Dmg {:+}{} / Def {:+} / Rate {})",
        w.name, w.speed, w.accuracy, w.damage, dmg_type, w.defense, w.rate
    )
}

// ---------------------------------------------------------------------------
// Armor, soak, DVs
// ---------------------------------------------------------------------------

fn fill_armor_and_defense(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    write(doc, index, "soak1", &soak_bashing(c).to_string())?;
    write(doc, index, "soak2", &soak_lethal(c).to_string())?;
    write(doc, index, "soak3", &soak_aggravated(c).to_string())?;

    // dv1=Dodge DV, dv2=parry DV (best melee weapon), dv3=mental dodge DV.
    write(doc, index, "dv1", &dodge_dv(c).to_string())?;
    let parry = c
        .equipment
        .weapons
        .iter()
        .map(|w| parry_dv(c, w))
        .max()
        .unwrap_or(0);
    write(doc, index, "dv2", &parry.to_string())?;
    write(doc, index, "dv3", &mdv_dodge(c).to_string())?;

    if index.has("armor") {
        if let Some(a) = &c.equipment.armor {
            let line = format!(
                "{} (B/L/A {}/{}/{}, mob -{}, fat {})",
                a.name,
                a.soak_bashing,
                a.soak_lethal,
                a.soak_aggravated,
                a.mobility_penalty,
                a.fatigue
            );
            write(doc, index, "armor", &line)?;
        }
    }

    let m = movement(c);
    if index.has("combat") {
        let line = format!(
            "Join Battle {} · Move {} / Dash {} · Jump {}v / {}h",
            join_battle(c),
            m.move_,
            m.dash,
            m.jump_vertical,
            m.jump_horizontal
        );
        write(doc, index, "combat", &line)?;
    }

    // Page 4 athletics boxes: Move / Dash / Vertical Jump / Horizontal Jump / Lift.
    write(doc, index, "Mo", &m.move_.to_string())?;
    write(doc, index, "DA", &m.dash.to_string())?;
    write(doc, index, "VJ", &m.jump_vertical.to_string())?;
    write(doc, index, "HJ", &m.jump_horizontal.to_string())?;
    write(doc, index, "LI", &lift_lbs(c).to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Charms (CHARMS table) and spells (SORCERY table).
//
// The sheet exposes two tables that share the `charms/sorcery…` field prefix:
//
//   CHARMS table — 14 rows × 5 columns (NAME | TYPE | DURATION | COST | EFFECT)
//     Row N (1..14): `sorceryN`, `sorcery{N+14}`, `sorcery{N+28}`,
//                    `sorcery{N+42}`, `sorcery{N+56}`.
//
//   SORCERY table — 5 rows × 5 columns (NAME | (unused) | DURATION | COST | EFFECT)
//     Row N (1..5), let M = N+9:
//     `sorcery{M}x`, `sorcery{M+14}x`, `sorcery{M+28}x`,
//     `sorcery{M+42}x`, `sorcery{M+56}x`.
//     The second column is present in the AcroForm but not labeled on the
//     printed sheet; we leave it blank.
//
// Field layout was verified via `cargo run --example dump_dots`.
// ---------------------------------------------------------------------------

const CHARM_ROWS: usize = 14;
const SPELL_ROWS: usize = 5;

fn fill_charms_and_spells(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    let db = database();

    for (i, charm) in c.charms.iter().take(CHARM_ROWS).enumerate() {
        let n = i + 1;
        write(doc, index, &format!("charms/sorcery{}", n), charm.display_name(db))?;
        if let Some(entry) = charm.entry(db) {
            write(doc, index, &format!("charms/sorcery{}", n + 14), entry.charm_type.display())?;
            write(doc, index, &format!("charms/sorcery{}", n + 28), &entry.duration)?;
            write(doc, index, &format!("charms/sorcery{}", n + 42), &entry.cost)?;
            write(doc, index, &format!("charms/sorcery{}", n + 56), &entry.effect)?;
        }
    }

    for (i, spell) in c.spells.iter().take(SPELL_ROWS).enumerate() {
        let m = i + 10;
        write(doc, index, &format!("charms/sorcery{}x", m), spell.display_name(db))?;
        if let Some(entry) = spell.entry(db) {
            // Column 2 (`sorcery{m+14}x`) is unlabeled on the sheet — skip.
            write(doc, index, &format!("charms/sorcery{}x", m + 28), &entry.duration)?;
            write(doc, index, &format!("charms/sorcery{}x", m + 42), &entry.cost)?;
            write(doc, index, &format!("charms/sorcery{}x", m + 56), &entry.effect)?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Combos
//
// MrGone's sheet has a Combos chart of 8 rows × 3 columns. Field positions
// confirmed via `cargo run --example dump_dots`:
//   - Cost column   (x = 143.7): combos1..combos8
//   - Name column   (x = 379.2): combos9..combos16
//   - Charms column (x = 539.5): combos17..combos24
// Row i (1-indexed, 1..=8) lives at the same y-coordinate across all three
// columns, so combo i fills (cost = combos{i}, name = combos{i+8},
// charms = combos{i+16}). Overflow past 8 combos is silently dropped.
// ---------------------------------------------------------------------------

const COMBO_ROWS: usize = 8;

fn fill_combos(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    let db = database();
    for (i, combo) in c.combos.iter().take(COMBO_ROWS).enumerate() {
        let row = i + 1;
        let cost_field = format!("combos{}", row);
        let name_field = format!("combos{}", row + 8);
        let charms_field = format!("combos{}", row + 16);

        let name = if combo.name.is_empty() {
            "<unnamed>"
        } else {
            combo.name.as_str()
        };
        write(doc, index, &name_field, name)?;

        let charms_label = combo
            .charm_ids
            .iter()
            .map(|id| {
                c.charms
                    .iter()
                    .find(|ch| ch.is_id(id))
                    .map(|ch| ch.display_name(db).to_string())
                    .unwrap_or_else(|| id.clone())
            })
            .collect::<Vec<_>>()
            .join(", ");
        write(doc, index, &charms_field, &charms_label)?;

        let cost_label = combo_cost_summary(combo, c, db);
        write(doc, index, &cost_field, &cost_label)?;
    }
    Ok(())
}

/// Compact "mote totals + 1wp" string for the Combo's Cost cell. Charm cost
/// strings are free-form (`"3m"`, `"1m per die"`, etc.) so we just join them
/// rather than attempting arithmetic; the WP for activation is always +1.
fn combo_cost_summary(
    combo: &crate::character::Combo,
    c: &Character,
    db: &RulesDatabase,
) -> String {
    let parts: Vec<String> = combo
        .charm_ids
        .iter()
        .filter_map(|id| c.charms.iter().find(|ch| ch.is_id(id)))
        .filter_map(|ch| ch.entry(db))
        .map(|e| e.cost.clone())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        "+1wp".to_string()
    } else {
        format!("{}, +1wp", parts.join(" + "))
    }
}

// ---------------------------------------------------------------------------
// XP and essence pool labels
// ---------------------------------------------------------------------------

fn fill_xp_and_essence(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    if index.has("exp") {
        let spent = total_xp_spent(c);
        let remaining = c.xp_earned.saturating_sub(spent);
        write(doc, index, "exp", &format!("{} (spent {} of {})", remaining, spent, c.xp_earned))?;
    }
    // EP = Personal Essence pool max; AP = Peripheral Essence pool max.
    write(doc, index, "EP", &personal_essence_max(c).to_string())?;
    write(doc, index, "AP", &peripheral_essence_max(c).to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Virtue flaw line(s)
// ---------------------------------------------------------------------------

fn fill_virtue_flaw(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    if let Some(flaw) = &c.virtue_flaw {
        if index.has("virtueflaw1") {
            write(doc, index, "virtueflaw1", &format!("{:?}", flaw))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Familiar — only the name is wired up today; the rest of the `fam*` fields
// on the sheet stay blank until the Familiar model grows.
// ---------------------------------------------------------------------------

fn fill_familiar(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    if let Some(fam) = &c.familiar {
        write(doc, index, field_map::FAMILIAR_NAME_FIELD, &fam.name)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write(
    doc: &mut Document,
    index: &FieldIndex,
    name: &str,
    value: &str,
) -> Result<(), PdfRenderError> {
    if index.has(name) {
        set_text_field(doc, index, name, value)?;
    }
    Ok(())
}

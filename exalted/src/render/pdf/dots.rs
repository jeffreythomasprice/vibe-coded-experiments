//! Fill rating dot/circle checkboxes on the MrGone sheet — the visual
//! ●○○○○ representation that the markdown renderer prints with Unicode.
//!
//! Each rated trait gets a row of 5 (or for willpower/essence, more) tick
//! boxes. We check the first `rating` of them and leave the rest unchecked.
//! Field-name tables and `AttributeKind`/`AbilityKind`/`VirtueKind` position
//! lookups live in `field_map.rs`.

use lopdf::Document;

use crate::character::{AbilityKind, AttributeKind, Character, VirtueKind};

use super::PdfRenderError;
use super::acroform::{FieldIndex, set_checkbox};
use super::field_map;

pub(super) fn fill(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    fill_attribute_dots(doc, index, c)?;
    fill_ability_dots(doc, index, c)?;
    fill_specialty_dots(doc, index, c)?;
    fill_virtue_dots(doc, index, c)?;
    fill_willpower_dots(doc, index, c)?;
    fill_essence_dots(doc, index, c)?;
    fill_essence_overflow_dots(doc, index, c)?;
    fill_background_dots(doc, index, c)?;
    fill_intimacy_dots(doc, index, c)?;
    Ok(())
}

fn fill_attribute_dots(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for kind in AttributeKind::ALL {
        let rating = c.attribute(*kind) as usize;
        for i in 0..5 {
            if let Some(field) = field_map::attribute_dot(*kind, i) {
                if index.has(field) {
                    set_checkbox(doc, index, field, i < rating)?;
                }
            }
        }
    }
    Ok(())
}

fn fill_ability_dots(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for kind in AbilityKind::ALL {
        let rating = c.ability(*kind) as usize;
        for i in 0..5 {
            if let Some(field) = field_map::ability_dot(*kind, i) {
                if index.has(field) {
                    set_checkbox(doc, index, field, i < rating)?;
                }
            }
        }
    }
    Ok(())
}

/// Fill the 5-dot rating bubbles on each rendered specialty row. The sheet
/// caps display at 5 dots per row; the rulebook caps most specialties at 3
/// (Linguistics excepted) — overflow is silently truncated for display.
fn fill_specialty_dots(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for (row_idx, row) in super::specialties::rows(c).iter().enumerate() {
        let Some(fields) = field_map::specialty_dots(row_idx) else {
            break;
        };
        let rating = (row.dots as usize).min(fields.len());
        for (i, field) in fields.iter().enumerate() {
            if index.has(field) {
                set_checkbox(doc, index, field, i < rating)?;
            }
        }
    }
    Ok(())
}

fn fill_virtue_dots(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for kind in VirtueKind::ALL {
        let rating = c.virtue(*kind) as usize;
        for i in 0..5 {
            if let Some(field) = field_map::virtue_dot(*kind, i) {
                if index.has(field) {
                    set_checkbox(doc, index, field, i < rating)?;
                }
            }
        }
    }
    Ok(())
}

fn fill_willpower_dots(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    let rating = c.willpower_dots() as usize;
    for (i, field) in field_map::WILLPOWER_DOTS.iter().enumerate() {
        if index.has(field) {
            set_checkbox(doc, index, field, i < rating)?;
        }
    }
    Ok(())
}

fn fill_essence_dots(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    // MrGone sheet only carries AcroForm fields for dots 1–6; dots 7–10 are
    // drawn separately as overlay content by `fill_essence_overflow_dots`.
    let rating = (c.essence_dots() as usize).min(field_map::ESSENCE_DOTS.len());
    for (i, field) in field_map::ESSENCE_DOTS.iter().enumerate() {
        if index.has(field) {
            set_checkbox(doc, index, field, i < rating)?;
        }
    }
    Ok(())
}

/// Per p.78–79 of the 2E core, Essence ranges 1–10 (Solars start at 2 and
/// cap at 5 for new characters; only century-old beings reach 6+). The
/// MrGone editable PDF only includes six dot widgets, so we draw the
/// remaining four (positions 7–10) as overlay content in a second row
/// directly below the original six, matching the markdown renderer.
fn fill_essence_overflow_dots(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    let rating = c.essence_dots() as usize;
    // Skip the overlay entirely when no overflow dot would be filled. This
    // keeps the rendered sheet visually identical to the unmodified
    // template for the common case (Essence 1–6).
    if rating < 7 {
        return Ok(());
    }
    let Some((d1_widget, d1)) = super::acroform::first_widget_rect(doc, index, "essencedot1")
    else {
        return Ok(());
    };
    let Some((_, d6)) = super::acroform::first_widget_rect(doc, index, "essencedot6") else {
        return Ok(());
    };
    let Some(page_id) = super::overlay::page_containing_widget(doc, d1_widget) else {
        return Ok(());
    };

    let cx1 = (d1[0] + d1[2]) / 2.0;
    let cy1 = (d1[1] + d1[3]) / 2.0;
    let cx6 = (d6[0] + d6[2]) / 2.0;
    let width = d1[2] - d1[0];
    let height = d1[3] - d1[1];
    let spacing = (cx6 - cx1) / 5.0;
    // Slight inset so our drawn circle visually matches the widget glyphs
    // rather than spilling to the rect edge.
    let radius = (width.min(height) / 2.0 - 2.5).max(1.0);
    // Drop a row below the original six. The MrGone sheet has only a thin
    // gap before the next content block, so we sit close to the original
    // row (about a third of a dot's height below).
    let row_y = cy1 - height * 1.5 + 9.0;
    // Center the four overflow dots horizontally within the span of the
    // original six. Four dots at `spacing` apart cover 3 × spacing of the
    // 5 × spacing total, leaving 1 × spacing of slack — half on each side.
    let row_left_cx = cx1 + spacing;

    let mut content: Vec<u8> = Vec::new();
    content.extend_from_slice(b"\n% exalted-essence-overflow\nq\n0 G\n0 g\n0.5 w\n");
    for i in 0..4 {
        let dot_index = 7 + i;
        let cx = row_left_cx + (i as f64) * spacing;
        super::overlay::append_circle(&mut content, cx, row_y, radius, rating >= dot_index);
    }
    content.extend_from_slice(b"Q\n");

    super::overlay::append_page_content(doc, page_id, content)?;
    Ok(())
}

fn fill_background_dots(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    // 18 background slots total (8 page-1 + 10 page-4). Anything past slot
    // 18 truncates with a warning.
    let mut overflow = 0usize;
    for (slot, bg) in c.backgrounds.iter().enumerate() {
        let Some(row) = field_map::background_dots(slot) else {
            overflow += 1;
            continue;
        };
        let rating = bg.trait_().dots() as usize;
        for (i, field) in row.iter().enumerate() {
            if index.has(field) {
                set_checkbox(doc, index, field, i < rating)?;
            }
        }
    }
    if overflow > 0 {
        eprintln!(
            "warning: {} background dot row(s) beyond slot 18 not rendered",
            overflow
        );
    }
    Ok(())
}

fn fill_intimacy_dots(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    // 10 slots × 10 rating dots on page 4. Intimacies past slot 10 truncate
    // with a warning (matches text-field truncation in `text_fields.rs`).
    let mut overflow = 0usize;
    for (slot, intimacy) in c.intimacies.iter().enumerate() {
        if slot >= field_map::INTIMACY_DOTS.len() {
            overflow += 1;
            continue;
        }
        let rating = (intimacy.rating as usize).min(field_map::INTIMACY_DOTS[slot].len());
        for (i, field) in field_map::INTIMACY_DOTS[slot].iter().enumerate() {
            if index.has(field) {
                set_checkbox(doc, index, field, i < rating)?;
            }
        }
    }
    if overflow > 0 {
        eprintln!(
            "warning: {} intimacy/-ies beyond slot 10 not rendered",
            overflow
        );
    }
    Ok(())
}

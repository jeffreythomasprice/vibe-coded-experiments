//! Fill rating dot/circle checkboxes on the MrGone sheet — the visual
//! ●○○○○ representation that the markdown renderer prints with Unicode.
//!
//! Each rated trait gets a row of 5 (or for willpower/essence, more) tick
//! boxes. We check the first `rating` of them and leave the rest unchecked.
//! Field-name tables and `AttributeKind`/`AbilityKind`/`VirtueKind` position
//! lookups live in `dot_map.rs`.

use lopdf::Document;

use crate::character::{AbilityKind, AttributeKind, Character, VirtueKind};

use super::acroform::{set_checkbox, FieldIndex};
use super::dot_map;
use super::PdfRenderError;

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
    fill_background_dots(doc, index, c)?;
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
            if let Some(field) = dot_map::attribute_dot(*kind, i) {
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
            if let Some(field) = dot_map::ability_dot(*kind, i) {
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
        let Some(fields) = dot_map::specialty_dots(row_idx) else {
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
            if let Some(field) = dot_map::virtue_dot(*kind, i) {
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
    for (i, field) in dot_map::WILLPOWER_DOTS.iter().enumerate() {
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
    // Sheet caps essence at 6 dots; truncate higher ratings.
    let rating = (c.essence_dots() as usize).min(dot_map::ESSENCE_DOTS.len());
    for (i, field) in dot_map::ESSENCE_DOTS.iter().enumerate() {
        if index.has(field) {
            set_checkbox(doc, index, field, i < rating)?;
        }
    }
    Ok(())
}

fn fill_background_dots(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    // First 8 backgrounds fill the first 8 slots; extras truncate silently.
    for (slot, bg) in c
        .backgrounds
        .iter()
        .take(dot_map::BACKGROUND_SLOT_DOTS.len())
        .enumerate()
    {
        let rating = bg.trait_.dots() as usize;
        let row = &dot_map::BACKGROUND_SLOT_DOTS[slot];
        for (i, field) in row.iter().enumerate() {
            if index.has(field) {
                set_checkbox(doc, index, field, i < rating)?;
            }
        }
    }
    Ok(())
}

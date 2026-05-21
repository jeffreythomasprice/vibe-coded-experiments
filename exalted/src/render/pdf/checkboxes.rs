//! Fill semantically-named checkbox groups on the MrGone sheet:
//! health track, limit track, caste/favored marks, willpower-temp track,
//! virtue channels, and the personal/peripheral essence mote pools.

use lopdf::Document;

use crate::character::{AbilityKind, Character, VirtueKind};

use super::acroform::{set_checkbox, FieldIndex};
use super::dot_map;
use super::PdfRenderError;

pub(super) fn fill(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    fill_caste_favored_marks(doc, index, c)?;
    fill_specialty_caste_marks(doc, index, c)?;
    fill_health_track(doc, index, c)?;
    fill_limit_track(doc, index, c)?;
    fill_willpower_temp(doc, index, c)?;
    fill_virtue_channels(doc, index, c)?;
    fill_essence_pools(doc, index, c)?;
    Ok(())
}

fn fill_caste_favored_marks(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for kind in AbilityKind::ALL {
        let field = dot_map::ability_caste_mark(*kind);
        if index.has(field) {
            set_checkbox(doc, index, field, c.is_caste_or_favored_ability(*kind))?;
        }
    }
    Ok(())
}

/// Tick the C/F box on each rendered specialty row when its parent Ability
/// is Caste or Favored — the in-caste discount that applies to the Ability
/// applies to its specialties as well (rules p.77).
fn fill_specialty_caste_marks(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for (i, row) in super::specialties::rows(c).iter().enumerate() {
        if let Some(field) = dot_map::specialty_caste_mark(i) {
            if index.has(field) {
                set_checkbox(
                    doc,
                    index,
                    field,
                    c.is_caste_or_favored_ability(row.ability),
                )?;
            }
        }
    }
    Ok(())
}

fn fill_health_track(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    let damage = c.pool_state.health_damage.total() as usize;
    for (i, field) in dot_map::HEALTH_TRACK.iter().enumerate() {
        if index.has(field) {
            set_checkbox(doc, index, field, i < damage)?;
        }
    }
    Ok(())
}

fn fill_limit_track(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    let limit = c.pool_state.limit as usize;
    for (i, field) in dot_map::LIMIT_TRACK.iter().enumerate() {
        if index.has(field) {
            set_checkbox(doc, index, field, i < limit)?;
        }
    }
    Ok(())
}

fn fill_willpower_temp(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    // Mark off the boxes corresponding to spent temporary willpower.
    let spent = c.pool_state.willpower_temporary as usize;
    for (i, field) in dot_map::WILLPOWER_TEMP.iter().enumerate() {
        if index.has(field) {
            set_checkbox(doc, index, field, i < spent)?;
        }
    }
    Ok(())
}

fn fill_virtue_channels(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    // The 5 channel boxes are filled left-to-right as channels are *spent*.
    for kind in VirtueKind::ALL {
        let dots = c.virtue(*kind);
        if dots == 0 {
            continue;
        }
        let remaining = c.pool_state.channels_remaining(*kind, dots);
        let spent = dots.saturating_sub(remaining) as usize;
        for i in 0..5 {
            if let Some(field) = dot_map::virtue_channel(*kind, i) {
                if index.has(field) {
                    set_checkbox(doc, index, field, i < spent)?;
                }
            }
        }
    }
    Ok(())
}

fn fill_essence_pools(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    let personal_spent =
        (c.pool_state.personal_motes_spent as usize).min(dot_map::PERSONAL_ESSENCE_BOXES);
    for i in 0..dot_map::PERSONAL_ESSENCE_BOXES {
        let f = dot_map::personal_essence_field(i);
        if index.has(&f) {
            set_checkbox(doc, index, &f, i < personal_spent)?;
        }
    }
    let peripheral_spent =
        (c.pool_state.peripheral_motes_spent as usize).min(dot_map::PERIPHERAL_ESSENCE_BOXES);
    for i in 0..dot_map::PERIPHERAL_ESSENCE_BOXES {
        let f = dot_map::peripheral_essence_field(i);
        if index.has(&f) {
            set_checkbox(doc, index, &f, i < peripheral_spent)?;
        }
    }
    Ok(())
}

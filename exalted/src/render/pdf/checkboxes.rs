//! Fill semantically-named checkbox groups on the MrGone sheet:
//! health track, limit track, caste/favored marks, willpower-temp track,
//! virtue channels, and the personal/peripheral essence mote pools.

use std::collections::{HashMap, HashSet};

use lopdf::Document;

use crate::character::{AbilityKind, Character, VirtueKind};
use crate::rules::health::{HealthLevelKind, health_track};

use super::PdfRenderError;
use super::acroform::{FieldIndex, set_checkbox};
use super::field_map;

pub(super) fn fill(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    fill_caste_favored_marks(doc, index, c)?;
    fill_specialty_caste_marks(doc, index, c)?;
    fill_health_track(doc, index, c)?;
    fill_limit_track(doc, index, c)?;
    fill_language_checkboxes(doc, index, c)?;
    fill_willpower_temp(doc, index, c)?;
    fill_virtue_channels(doc, index, c)?;
    fill_essence_pools(doc, index, c)?;
    fill_familiar_health(doc, index, c)?;
    Ok(())
}

fn fill_caste_favored_marks(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for kind in AbilityKind::ALL {
        let field = field_map::ability_caste_mark(*kind);
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
        if let Some(field) = field_map::specialty_caste_mark(i) {
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
    let track = health_track(c);
    let damage = c.pool_state.health_damage.total() as usize;

    let mut cursor: HashMap<(i8, HealthLevelKind), usize> = HashMap::new();
    let mut touched: HashSet<&'static str> = HashSet::new();
    let mut dropped_dying: usize = 0;
    let mut overflow: HashMap<(i8, HealthLevelKind), usize> = HashMap::new();

    for (i, level) in track.iter().enumerate() {
        let bucket = (level.penalty, level.kind);
        let n = cursor.entry(bucket).or_insert(0);

        if level.kind == HealthLevelKind::Dying {
            dropped_dying += 1;
            *n += 1;
            continue;
        }

        let slots = field_map::health_slots(level.penalty, level.kind);
        if let Some(field) = slots.get(*n).copied() {
            if index.has(field) {
                set_checkbox(doc, index, field, i < damage)?;
                touched.insert(field);
            }
        } else {
            *overflow.entry(bucket).or_insert(0) += 1;
        }
        *n += 1;
    }

    // Clear any slots the per-bucket walk didn't write to. Without this,
    // a stale check from the template (or a future re-render) could leak
    // through, since different Ox-Body layouts use different slot subsets.
    for field in field_map::ALL_HEALTH_SLOTS.iter() {
        if !touched.contains(field) && index.has(field) {
            set_checkbox(doc, index, field, false)?;
        }
    }

    if dropped_dying > 0 {
        eprintln!(
            "warning: {} Dying health row(s) not rendered (PDF template has no slots for them)",
            dropped_dying
        );
    }
    for ((penalty, kind), n) in overflow {
        eprintln!(
            "warning: {} health level(s) at penalty {} kind {:?} could not be rendered (PDF row is full)",
            n, penalty, kind
        );
    }
    Ok(())
}

fn fill_limit_track(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    let limit = c.pool_state.limit as usize;
    for (i, field) in field_map::LIMIT_TRACK.iter().enumerate() {
        if index.has(field) {
            set_checkbox(doc, index, field, i < limit)?;
        }
    }
    Ok(())
}

/// Tick the per-row checkbox of the slot whose language is native; clear
/// the rest. Mirrors the analogous Caste/Favored mark on ability rows.
fn fill_language_checkboxes(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    for (i, field) in field_map::LANGUAGE_CHECKBOXES.iter().enumerate() {
        if !index.has(field) {
            continue;
        }
        let native = c.languages.get(i).is_some_and(|l| l.native);
        set_checkbox(doc, index, field, native)?;
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
    for (i, field) in field_map::WILLPOWER_TEMP.iter().enumerate() {
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
            if let Some(field) = field_map::virtue_channel(*kind, i) {
                if index.has(field) {
                    set_checkbox(doc, index, field, i < spent)?;
                }
            }
        }
    }
    Ok(())
}

/// Tick the first N familiar-health checkboxes for total damage taken. When
/// the character has no familiar, every box is cleared.
fn fill_familiar_health(
    doc: &mut Document,
    index: &FieldIndex,
    c: &Character,
) -> Result<(), PdfRenderError> {
    let damage = c
        .familiar
        .as_ref()
        .map(|f| f.health_damage.total() as usize)
        .unwrap_or(0)
        .min(field_map::FAMILIAR_HEALTH_SLOTS.len());
    for (i, field) in field_map::FAMILIAR_HEALTH_SLOTS.iter().enumerate() {
        if index.has(field) {
            set_checkbox(doc, index, field, i < damage)?;
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
        (c.pool_state.personal_motes_spent as usize).min(field_map::PERSONAL_ESSENCE_BOXES);
    for i in 0..field_map::PERSONAL_ESSENCE_BOXES {
        let f = field_map::personal_essence_field(i);
        if index.has(&f) {
            set_checkbox(doc, index, &f, i < personal_spent)?;
        }
    }
    let peripheral_spent =
        (c.pool_state.peripheral_motes_spent as usize).min(field_map::PERIPHERAL_ESSENCE_BOXES);
    for i in 0..field_map::PERIPHERAL_ESSENCE_BOXES {
        let f = field_map::peripheral_essence_field(i);
        if index.has(&f) {
            set_checkbox(doc, index, &f, i < peripheral_spent)?;
        }
    }
    Ok(())
}

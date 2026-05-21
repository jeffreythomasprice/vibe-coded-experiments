//! Fill semantically-named checkbox groups on the MrGone sheet:
//! health track, limit track, caste/favored marks, willpower-temp track,
//! virtue channels, and the personal/peripheral essence mote pools.

use std::collections::{HashMap, HashSet};

use lopdf::Document;

use crate::character::{AbilityKind, Character, VirtueKind};
use crate::rules::health::{health_track, HealthLevelKind};

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

        let slots = dot_map::health_slots(level.penalty, level.kind);
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
    for field in dot_map::ALL_HEALTH_SLOTS.iter() {
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

//! Layout the per-row data for the sheet's five specialty write-in slots.
//!
//! Aggregates a `Character`'s specialty entries into one row per distinct
//! (ability, name), then truncates at 5 rows (the number the sheet has).
//! Used by `text_fields`, `checkboxes`, and `dots` so all three agree on
//! which specialty lands in which row.

use crate::character::{AbilityKind, Character};

use super::dot_map::SPECIALTY_ROWS;

pub(super) struct SpecialtyRow {
    pub ability: AbilityKind,
    pub name: String,
    pub dots: u8,
}

pub(super) fn rows(c: &Character) -> Vec<SpecialtyRow> {
    let mut out: Vec<SpecialtyRow> = Vec::new();
    for kind in AbilityKind::ALL {
        if out.len() >= SPECIALTY_ROWS {
            break;
        }
        if let Some(t) = c.abilities.get(kind) {
            for (name, dots) in t.aggregated_specialties() {
                out.push(SpecialtyRow {
                    ability: *kind,
                    name,
                    dots,
                });
                if out.len() >= SPECIALTY_ROWS {
                    break;
                }
            }
        }
    }
    out
}

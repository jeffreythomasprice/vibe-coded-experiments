//! Layout the per-row data for the sheet's five specialty write-in slots.
//!
//! Aggregates a `Character`'s specialty entries into one row per distinct
//! (ability, name), then truncates at 5 rows (the number the sheet has).
//! Used by `text_fields`, `checkboxes`, and `dots` so all three agree on
//! which specialty lands in which row.

use crate::character::{AbilityKind, Character};

use super::field_map::SPECIALTY_ROWS;

pub(super) struct SpecialtyRow {
    pub ability: AbilityKind,
    pub name: String,
    pub dots: u8,
    /// When set, the text field is written verbatim instead of the usual
    /// `"{ability}: {name}"` formatting. Used for the secondary crafts that
    /// borrow the specialty rows — their label is already `"Craft: Fire"`.
    pub label_override: Option<String>,
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
                    label_override: None,
                });
                if out.len() >= SPECIALTY_ROWS {
                    break;
                }
            }
        }
    }
    // The sheet has a single Craft row, so secondary crafts (everything past
    // the primary) borrow the specialty rows: the focus name + its dots. They
    // are not true specialties, just a convenient place to record the rating.
    for craft in c.crafts.iter().skip(1) {
        if out.len() >= SPECIALTY_ROWS {
            break;
        }
        let label = if craft.focus.is_empty() {
            "Craft".to_string()
        } else {
            format!("Craft: {}", craft.focus)
        };
        out.push(SpecialtyRow {
            ability: AbilityKind::Craft,
            name: craft.focus.clone(),
            dots: craft.rating.dots(),
            label_override: Some(label),
        });
    }
    out
}

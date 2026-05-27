//! Virtues section: 4 virtues + primary marker.

use crate::character::{DotSource, RatedTrait, VirtueKind};
use crate::render::names::virtue_name;
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::DotSourceKind;
use crate::ui::widgets::rated_trait::{RatedTraitOpts, rated_trait_editor};

const VIRTUE_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::Base,
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Virtues");

    let prev_primary = state.character.primary_virtue;
    let mut primary = prev_primary;
    ui.horizontal(|ui| {
        ui.label("Primary Virtue");
        egui::ComboBox::from_id_salt("primary-virtue-combo")
            .selected_text(match primary {
                Some(v) => virtue_name(v),
                None => "<unset>",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut primary, None, "<unset>");
                for v in VirtueKind::ALL {
                    ui.selectable_value(&mut primary, Some(*v), virtue_name(*v));
                }
            });
    });
    if primary != prev_primary {
        state.character.primary_virtue = primary;
        state.mark_dirty_with("virtues.primary");
    }

    let default_source = if state.character.is_in_chargen() {
        DotSource::ChargenPriority
    } else {
        DotSource::Xp { spent: 0 }
    };
    let mut any_changed = false;
    for v in VirtueKind::ALL {
        let entry = state
            .character
            .virtues
            .entry(*v)
            .or_insert_with(|| RatedTrait::with_base(1));
        let label = if Some(*v) == state.character.primary_virtue {
            format!("{} ★", virtue_name(*v))
        } else {
            virtue_name(*v).to_string()
        };
        let mut opts = RatedTraitOpts {
            label: &label,
            max_dots: 5,
            allowed_sources: VIRTUE_SOURCES,
            default_add_source: default_source,
            show_specialties: false,
            selectable: None,
        };
        if rated_trait_editor(ui, ("virtue", *v as usize), entry, &mut opts) {
            any_changed = true;
        }
    }
    if any_changed {
        state.mark_dirty_with("virtues.dots");
    }
}

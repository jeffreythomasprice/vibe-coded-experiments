//! Attributes section: AttributePriority assignment + each attribute's
//! RatedTrait editor.

use crate::character::{AttributeGroup, DotSource, RatedTrait};
use crate::render::names::{attr_name, group_name};
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::DotSourceKind;
use crate::ui::widgets::rated_trait::{RatedTraitOpts, rated_trait_editor};

const ATTRIBUTE_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::Base,
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Attributes");

    attribute_priority_editor(ui, state);
    ui.add_space(8.0);

    // Three columns: Physical | Social | Mental.
    let default_source = if state.character.is_in_chargen() {
        DotSource::ChargenPriority
    } else {
        DotSource::Xp { spent: 0 }
    };
    let mut any_changed = false;
    egui::Grid::new("attributes-grid")
        .num_columns(3)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for group in AttributeGroup::ALL {
                let pri_dots = state.character.attribute_priority.dots_for(*group);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(group_name(*group)).strong().underline());
                    ui.small(format!("chargen pool: {} dots", pri_dots));
                });
            }
            ui.end_row();
            for group in AttributeGroup::ALL {
                ui.vertical(|ui| {
                    for attr in group.members() {
                        let entry = state
                            .character
                            .attributes
                            .entry(*attr)
                            .or_insert_with(|| RatedTrait::with_base(1));
                        let opts = RatedTraitOpts {
                            label: attr_name(*attr),
                            max_dots: 5,
                            allowed_sources: ATTRIBUTE_SOURCES,
                            default_add_source: default_source,
                            show_specialties: false,
                        };
                        if rated_trait_editor(ui, ("attr", *attr as usize), entry, &opts) {
                            any_changed = true;
                        }
                    }
                });
            }
            ui.end_row();
        });
    if any_changed {
        state.mark_dirty();
    }
}

fn attribute_priority_editor(ui: &mut egui::Ui, state: &mut AppState) {
    ui.label("Priority assignment (8 / 6 / 4)");
    let mut any = false;
    ui.horizontal(|ui| {
        ui.label("Primary");
        any |= priority_combo(
            ui,
            "primary-group",
            &mut state.character.attribute_priority.primary,
        );
        ui.label("Secondary");
        any |= priority_combo(
            ui,
            "secondary-group",
            &mut state.character.attribute_priority.secondary,
        );
        ui.label("Tertiary");
        any |= priority_combo(
            ui,
            "tertiary-group",
            &mut state.character.attribute_priority.tertiary,
        );
    });
    if any {
        state.mark_dirty();
    }
}

fn priority_combo(ui: &mut egui::Ui, id: &str, slot: &mut AttributeGroup) -> bool {
    let prev = *slot;
    egui::ComboBox::from_id_salt(id)
        .selected_text(group_name(*slot))
        .show_ui(ui, |ui| {
            for g in AttributeGroup::ALL {
                ui.selectable_value(slot, *g, group_name(*g));
            }
        });
    *slot != prev
}

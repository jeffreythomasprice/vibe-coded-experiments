//! Charms section: list current charms (grouped by ability), edit each
//! one's source / notes / non_solar flag, and add new charms via the picker.

use crate::character::CharmRef;
use crate::render::names::ability_name;
use crate::rules::database::database;
use crate::ui::pickers::charm_picker::CharmPickerState;
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::{dot_source_editor, DotSourceKind};
use crate::ui::widgets::icon_button::trash_button_with_label;
use crate::ui::widgets::notes_list::notes_editor;

const CHARM_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.charms.len();
    ui.heading(format!("Charms ({})", count));

    ui.horizontal(|ui| {
        if ui.button("+ Add charm…").clicked() {
            state.charm_picker = Some(CharmPickerState::new_for_character(&state.character));
        }
        ui.small("Picker filters the rules database; choose how to pay there.");
    });

    let db = database();
    let mut any_changed = false;
    let mut delete_idx: Option<usize> = None;
    for (i, charm) in state.character.charms.iter_mut().enumerate() {
        let id_str = charm.id().to_string();
        let name = charm.display_name(db).to_string();
        let ability_label = charm.ability(db).map(ability_name).unwrap_or("—");
        let header = format!("[{}] {}", ability_label, name);
        let entry_effect = db.charm(&id_str).map(|e| {
            (
                e.effect.clone(),
                format!(
                    "min: {}/{} A/E   cost: {}",
                    e.mins_ability, e.mins_essence, e.cost
                ),
            )
        });
        egui::CollapsingHeader::new(header)
            .id_salt(("charm", i))
            .default_open(false)
            .show(ui, |ui| {
                let (source, non_solar, notes) = match charm {
                    CharmRef::Lookup {
                        source,
                        non_solar,
                        notes,
                        ..
                    } => (source, non_solar, notes),
                    CharmRef::Custom {
                        source,
                        non_solar,
                        notes,
                        ..
                    } => (source, non_solar, notes),
                };

                ui.horizontal(|ui| {
                    ui.label("source");
                    if dot_source_editor(ui, ("charm-src", i), source, CHARM_SOURCES) {
                        any_changed = true;
                    }
                    if ui.checkbox(non_solar, "non-Solar").changed() {
                        any_changed = true;
                    }
                    if trash_button_with_label(ui, "remove").clicked() {
                        delete_idx = Some(i);
                    }
                });
                match &entry_effect {
                    Some((effect, mins)) => {
                        if !effect.is_empty() {
                            ui.small(effect);
                        }
                        ui.small(mins);
                    }
                    None => {
                        ui.small(format!("id {} not in rules database", id_str));
                    }
                }
                ui.add_space(4.0);
                ui.label("Notes");
                if notes_editor(ui, ("charm-notes", i), notes) {
                    any_changed = true;
                }
            });
    }
    if let Some(i) = delete_idx {
        state.character.charms.remove(i);
        any_changed = true;
    }
    if any_changed {
        state.mark_dirty();
    }
}

//! Spells section: list known spells (grouped by circle), edit source / notes,
//! add new spells via the picker.

use crate::character::SpellRef;
use crate::render::names::spell_circle_label;
use crate::rules::database::database;
use crate::ui::pickers::spell_picker::SpellPickerState;
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::{DotSourceKind, dot_source_editor};
use crate::ui::widgets::icon_button::trash_button_with_label;
use crate::ui::widgets::notes_list::notes_editor;

const SPELL_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.spells.len();
    ui.heading(format!("Spells ({})", count));

    if ui.button("+ Add spell…").clicked() {
        state.spell_picker = Some(SpellPickerState::new_for_character(&state.character));
    }

    let db = database();
    let mut any_changed = false;
    let mut delete_idx: Option<usize> = None;
    for (i, spell) in state.character.spells.iter_mut().enumerate() {
        let id_opt = spell.id().map(|s| s.to_string());
        let name = spell.display_name(db).to_string();
        let circle_label = spell.circle(db).map(spell_circle_label).unwrap_or("—");
        let header = format!("[{}] {}", circle_label, name);
        let entry_detail = id_opt.as_deref().and_then(|id| {
            db.spell(id).map(|e| {
                (
                    e.effect.clone(),
                    format!("cost: {}   duration: {}", e.cost, e.duration),
                )
            })
        });
        egui::CollapsingHeader::new(header)
            .id_salt(("spell", i))
            .default_open(false)
            .show(ui, |ui| {
                let (source, notes) = match spell {
                    SpellRef::Lookup { source, notes, .. } => (source, notes),
                    SpellRef::Custom { source, notes, .. } => (source, notes),
                };

                ui.horizontal(|ui| {
                    ui.label("source");
                    if dot_source_editor(ui, ("spell-src", i), source, SPELL_SOURCES) {
                        any_changed = true;
                    }
                    if trash_button_with_label(ui, "remove").clicked() {
                        delete_idx = Some(i);
                    }
                });
                match &entry_detail {
                    Some((effect, detail)) => {
                        if !effect.is_empty() {
                            ui.small(effect);
                        }
                        ui.small(detail);
                    }
                    None => {
                        if let Some(id) = &id_opt {
                            ui.small(format!("id {} not in rules database", id));
                        }
                    }
                }
                ui.add_space(4.0);
                ui.label("Notes");
                if notes_editor(ui, ("spell-notes", i), notes) {
                    any_changed = true;
                }
            });
    }
    if let Some(i) = delete_idx {
        state.character.spells.remove(i);
        any_changed = true;
    }
    if any_changed {
        state.mark_dirty();
    }
}

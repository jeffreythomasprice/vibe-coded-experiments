//! Spell picker: filter the rules database's ~223 spell entries by circle,
//! keywords, and a text search.

use crate::character::{Character, DotSource, SpellCircle, SpellRef};
use crate::render::names::spell_circle_label;
use crate::rules::database::{database, SpellEntry};
use crate::ui::pickers::PickerOutcome;
use crate::ui::widgets::dot_source::{dot_source_editor, DotSourceKind};

const PICKER_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub struct SpellPickerState {
    pub search: String,
    pub circle_filter: Option<SpellCircle>,
    pub selected_id: Option<String>,
    pub source: DotSource,
}

impl SpellPickerState {
    pub fn new_for_character(_c: &Character) -> Self {
        Self {
            search: String::new(),
            circle_filter: None,
            selected_id: None,
            source: DotSource::ChargenPriority,
        }
    }
}

pub fn show(
    ctx: &egui::Context,
    state: &mut SpellPickerState,
    _character: &Character,
) -> PickerOutcome<SpellRef> {
    let mut outcome = PickerOutcome::Stay;
    let db = database();

    let mut entries: Vec<&SpellEntry> = db
        .iter_spells()
        .filter(|e| passes_filters(e, state))
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    egui::Window::new("Add Spell")
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_size([720.0, 480.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.add(
                    egui::TextEdit::singleline(&mut state.search)
                        .desired_width(220.0)
                        .hint_text("name / id / keyword / effect"),
                );

                ui.label("Circle");
                egui::ComboBox::from_id_salt("spell-picker-circle")
                    .selected_text(match state.circle_filter {
                        None => "(any)",
                        Some(c) => spell_circle_label(c),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.circle_filter, None, "(any)");
                        for c in SpellCircle::ALL {
                            ui.selectable_value(
                                &mut state.circle_filter,
                                Some(*c),
                                spell_circle_label(*c),
                            );
                        }
                    });
            });

            ui.separator();

            ui.label(format!("{} match(es)", entries.len()));

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .max_height(ui.available_height() - 80.0)
                .show(ui, |ui| {
                    egui::Grid::new("spell-picker-results")
                        .num_columns(3)
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("Name");
                            ui.strong("Circle");
                            ui.strong("Cost");
                            ui.end_row();

                            for entry in &entries {
                                let selected =
                                    state.selected_id.as_deref() == Some(&entry.id);
                                let resp = ui.selectable_label(selected, &entry.name);
                                if resp.clicked() {
                                    state.selected_id = Some(entry.id.clone());
                                }
                                ui.label(spell_circle_label(entry.circle));
                                ui.label(if entry.cost.is_empty() {
                                    "—"
                                } else {
                                    entry.cost.as_str()
                                });
                                ui.end_row();
                            }
                        });
                });

            ui.separator();

            if let Some(id) = state.selected_id.clone() {
                if let Some(entry) = db.spell(&id) {
                    ui.label(egui::RichText::new(&entry.name).strong());
                    if !entry.effect.is_empty() {
                        ui.small(&entry.effect);
                    }
                    ui.small(format!(
                        "duration: {}   keywords: {}",
                        entry.duration,
                        if entry.keywords.is_empty() {
                            "—".to_string()
                        } else {
                            entry.keywords.join(", ")
                        }
                    ));
                }
            } else {
                ui.label("(select a spell above)");
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Pay with");
                dot_source_editor(
                    ui,
                    "spell-picker-source",
                    &mut state.source,
                    PICKER_SOURCES,
                );
            });

            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    outcome = PickerOutcome::Cancelled;
                }
                let can_add = state.selected_id.is_some();
                if ui
                    .add_enabled(can_add, egui::Button::new("Add spell"))
                    .clicked()
                {
                    if let Some(id) = state.selected_id.clone() {
                        outcome = PickerOutcome::Picked(SpellRef::Lookup {
                            id,
                            source: state.source,
                            notes: Vec::new(),
                        });
                    }
                }
            });
        });

    outcome
}

fn passes_filters(entry: &SpellEntry, state: &SpellPickerState) -> bool {
    if !state.search.is_empty() {
        let needle = state.search.to_ascii_lowercase();
        let in_text = entry.name.to_ascii_lowercase().contains(&needle)
            || entry.id.to_ascii_lowercase().contains(&needle)
            || entry.effect.to_ascii_lowercase().contains(&needle)
            || entry
                .keywords
                .iter()
                .any(|k| k.to_ascii_lowercase().contains(&needle));
        if !in_text {
            return false;
        }
    }
    if let Some(circle) = state.circle_filter {
        if entry.circle != circle {
            return false;
        }
    }
    true
}

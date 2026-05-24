//! Charm picker: filter the rules database's ~390 charm entries by ability,
//! min-ability / min-essence, exalt type, and a text search, then commit a
//! selection back as a new `CharmRef::Lookup`.

use crate::character::{AbilityKind, Character, CharmRef, DotSource};
use crate::render::names::ability_name;
use crate::rules::database::{CharmEntry, database};
use crate::ui::pickers::PickerOutcome;
use crate::ui::widgets::dot_source::{DotSourceKind, dot_source_editor};
use egui_extras::{Column, TableBuilder};

const PICKER_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub struct CharmPickerState {
    pub search: String,
    pub ability_filter: Option<AbilityKind>,
    pub exalt_filter: Option<String>,
    pub respect_character_caps: bool,
    pub selected_id: Option<String>,
    pub source: DotSource,
    pub non_solar: bool,
}

impl CharmPickerState {
    pub fn new_for_character(_c: &Character) -> Self {
        Self {
            search: String::new(),
            ability_filter: None,
            exalt_filter: Some("solar".to_string()),
            respect_character_caps: true,
            selected_id: None,
            source: DotSource::ChargenPriority,
            non_solar: false,
        }
    }
}

pub fn show(
    ctx: &egui::Context,
    state: &mut CharmPickerState,
    character: &Character,
) -> PickerOutcome<CharmRef> {
    let mut outcome = PickerOutcome::Stay;
    let db = database();

    // Collect ahead of the closure so we don't keep `db` borrowed across
    // mutable UI state.
    let mut entries: Vec<&CharmEntry> = db
        .iter_charms()
        .filter(|e| passes_filters(e, state, character))
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    egui::Window::new("Add Charm")
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_size([780.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.add(
                    egui::TextEdit::singleline(&mut state.search)
                        .desired_width(220.0)
                        .hint_text("name / id / keyword / effect"),
                );

                ui.label("Ability");
                egui::ComboBox::from_id_salt("charm-picker-ability")
                    .selected_text(match state.ability_filter {
                        None => "(any)",
                        Some(a) => ability_name(a),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut state.ability_filter, None, "(any)");
                        for a in AbilityKind::ALL {
                            ui.selectable_value(
                                &mut state.ability_filter,
                                Some(*a),
                                ability_name(*a),
                            );
                        }
                    });

                ui.label("Exalt");
                let exalt_label = state
                    .exalt_filter
                    .clone()
                    .unwrap_or_else(|| "(any)".to_string());
                egui::ComboBox::from_id_salt("charm-picker-exalt")
                    .selected_text(exalt_label)
                    .show_ui(ui, |ui| {
                        let options = [
                            "(any)",
                            "solar",
                            "lunar",
                            "sidereal",
                            "terrestrial",
                            "abyssal",
                        ];
                        for o in options {
                            let value = if o == "(any)" {
                                None
                            } else {
                                Some(o.to_string())
                            };
                            ui.selectable_value(&mut state.exalt_filter, value, o);
                        }
                    });

                ui.checkbox(&mut state.respect_character_caps, "only what I qualify for");
            });

            ui.separator();

            ui.label(format!("{} match(es)", entries.len()));

            let row_h = ui.spacing().interact_size.y;
            let available_body = (ui.available_height() - 130.0).max(120.0);

            TableBuilder::new(ui)
                .id_salt("charm-picker-results")
                .striped(true)
                .resizable(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(260.0).at_least(160.0).clip(true))
                .column(Column::initial(110.0).at_least(80.0))
                .column(Column::exact(48.0))
                .column(Column::exact(48.0))
                .column(Column::remainder().at_least(80.0))
                .min_scrolled_height(available_body)
                .max_scroll_height(available_body)
                .header(row_h, |mut header| {
                    header.col(|ui| {
                        ui.strong("Name");
                    });
                    header.col(|ui| {
                        ui.strong("Ability");
                    });
                    header.col(|ui| {
                        ui.strong("Min A");
                    });
                    header.col(|ui| {
                        ui.strong("Min E");
                    });
                    header.col(|ui| {
                        ui.strong("Type");
                    });
                })
                .body(|body| {
                    body.rows(row_h, entries.len(), |mut row| {
                        let entry = entries[row.index()];
                        let selected = state.selected_id.as_deref() == Some(&entry.id);
                        row.col(|ui| {
                            if ui.selectable_label(selected, &entry.name).clicked() {
                                state.selected_id = Some(entry.id.clone());
                            }
                        });
                        row.col(|ui| {
                            ui.label(if entry.ability.is_empty() {
                                "—"
                            } else {
                                entry.ability.as_str()
                            });
                        });
                        row.col(|ui| {
                            ui.label(entry.mins_ability.to_string());
                        });
                        row.col(|ui| {
                            ui.label(entry.mins_essence.to_string());
                        });
                        row.col(|ui| {
                            ui.label(entry.charm_type.display());
                        });
                    });
                });

            ui.separator();

            if let Some(id) = state.selected_id.clone() {
                if let Some(entry) = db.charm(&id) {
                    ui.label(egui::RichText::new(&entry.name).strong());
                    if !entry.effect.is_empty() {
                        ui.small(&entry.effect);
                    }
                    ui.small(format!(
                        "cost: {}   keywords: {}",
                        entry.cost,
                        if entry.keywords.is_empty() {
                            "—".to_string()
                        } else {
                            entry.keywords.join(", ")
                        }
                    ));
                }
            } else {
                ui.label("(select a charm above)");
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Pay with");
                dot_source_editor(ui, "charm-picker-source", &mut state.source, PICKER_SOURCES);
                ui.checkbox(&mut state.non_solar, "non-Solar (Eclipse-only)");
            });

            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    outcome = PickerOutcome::Cancelled;
                }
                let can_add = state.selected_id.is_some();
                if ui
                    .add_enabled(can_add, egui::Button::new("Add charm"))
                    .clicked()
                {
                    if let Some(id) = state.selected_id.clone() {
                        outcome = PickerOutcome::Picked(CharmRef::Lookup {
                            id,
                            source: state.source,
                            non_solar: state.non_solar,
                            notes: Vec::new(),
                            ox_body_pattern: None,
                        });
                    }
                }
            });
        });

    outcome
}

fn passes_filters(entry: &CharmEntry, state: &CharmPickerState, character: &Character) -> bool {
    // Text search.
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

    // Ability filter.
    if let Some(target) = state.ability_filter {
        if entry.ability_kind() != Some(target) {
            return false;
        }
    }

    // Exalt type filter.
    if let Some(exalt) = &state.exalt_filter {
        if !entry
            .exalt_type
            .to_ascii_lowercase()
            .contains(&exalt.to_ascii_lowercase())
        {
            return false;
        }
    }

    // "Only what I qualify for" — gate on character's current ability + essence.
    if state.respect_character_caps {
        if let Some(ab) = entry.ability_kind() {
            if character.ability(ab) < entry.mins_ability {
                return false;
            }
        }
        if character.essence_dots() < entry.mins_essence {
            return false;
        }
    }

    true
}

//! Spell picker: filter the rules database's ~223 spell entries by circle,
//! keywords, and a text search. Also offers a "Custom…" path for homebrew
//! spells whose full definition is embedded inline as a `SpellRef::Custom`.

use crate::character::{Character, DotSource, SpellCircle, SpellRef};
use crate::render::names::spell_circle_label;
use crate::rules::database::{SpellEntry, database};
use crate::ui::pickers::PickerOutcome;
use crate::ui::widgets::custom_entry::spell_entry_form;
use crate::ui::widgets::dot_source::{DotSourceKind, dot_source_editor};
use egui_extras::{Column, TableBuilder};

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
    pub custom: Option<SpellEntry>,
}

impl SpellPickerState {
    pub fn new_for_character(_c: &Character) -> Self {
        Self {
            search: String::new(),
            circle_filter: None,
            selected_id: None,
            source: DotSource::ChargenPriority,
            custom: None,
        }
    }
}

fn blank_custom_entry() -> SpellEntry {
    SpellEntry {
        id: String::new(),
        name: String::new(),
        circle: SpellCircle::Terrestrial,
        cost: String::new(),
        keywords: Vec::new(),
        duration: String::new(),
        target: String::new(),
        source: String::new(),
        pages: String::new(),
        effect: String::new(),
        description: String::new(),
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
        .default_size([720.0, 560.0])
        .show(ctx, |ui| {
            if state.custom.is_some() {
                show_custom_panel(ui, state, &mut outcome);
            } else {
                show_lookup_panel(ui, state, &entries, &mut outcome);
            }
        });

    outcome
}

fn show_lookup_panel(
    ui: &mut egui::Ui,
    state: &mut SpellPickerState,
    entries: &[&SpellEntry],
    outcome: &mut PickerOutcome<SpellRef>,
) {
    let db = crate::rules::database::database();

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

    let row_h = ui.spacing().interact_size.y;
    let available_body = (ui.available_height() - 160.0).max(120.0);

    TableBuilder::new(ui)
        .id_salt("spell-picker-results")
        .striped(true)
        .resizable(false)
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(280.0).at_least(180.0).clip(true))
        .column(Column::initial(120.0).at_least(90.0))
        .column(Column::remainder().at_least(80.0))
        .min_scrolled_height(available_body)
        .max_scroll_height(available_body)
        .header(row_h, |mut header| {
            header.col(|ui| {
                ui.strong("Name");
            });
            header.col(|ui| {
                ui.strong("Circle");
            });
            header.col(|ui| {
                ui.strong("Cost");
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
                    ui.label(spell_circle_label(entry.circle));
                });
                row.col(|ui| {
                    ui.label(if entry.cost.is_empty() {
                        "—"
                    } else {
                        entry.cost.as_str()
                    });
                });
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
            if ui.small_button("Duplicate as custom…").clicked() {
                let mut dup = entry.clone();
                dup.id = String::new();
                state.custom = Some(dup);
                state.selected_id = None;
            }
        }
    } else {
        ui.label("(select a spell above)");
    }

    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Pay with");
        dot_source_editor(ui, "spell-picker-source", &mut state.source, PICKER_SOURCES);
    });

    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            *outcome = PickerOutcome::Cancelled;
        }
        if ui.button("Custom…").clicked() {
            state.custom = Some(blank_custom_entry());
            state.selected_id = None;
        }
        let can_add = state.selected_id.is_some();
        if ui
            .add_enabled(can_add, egui::Button::new("Add spell"))
            .clicked()
        {
            if let Some(id) = state.selected_id.clone() {
                *outcome = PickerOutcome::Picked(SpellRef::Lookup {
                    id,
                    source: state.source,
                    notes: Vec::new(),
                });
            }
        }
    });
}

fn show_custom_panel(
    ui: &mut egui::Ui,
    state: &mut SpellPickerState,
    outcome: &mut PickerOutcome<SpellRef>,
) {
    ui.label(egui::RichText::new("Custom spell").strong());

    let body_height = (ui.available_height() - 120.0).max(120.0);
    egui::ScrollArea::vertical()
        .id_salt("spell-picker-custom-scroll")
        .auto_shrink([false; 2])
        .max_height(body_height)
        .show(ui, |ui| {
            if let Some(entry) = state.custom.as_mut() {
                spell_entry_form(ui, "spell-picker-custom", entry);
            }
        });

    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Pay with");
        dot_source_editor(
            ui,
            "spell-picker-custom-source",
            &mut state.source,
            PICKER_SOURCES,
        );
    });

    ui.horizontal(|ui| {
        if ui.button("Cancel").clicked() {
            *outcome = PickerOutcome::Cancelled;
        }
        if ui.button("Back to picker").clicked() {
            state.custom = None;
        }
        let custom_ready = state
            .custom
            .as_ref()
            .is_some_and(|c| !c.id.is_empty() && !c.name.is_empty());
        if ui
            .add_enabled(custom_ready, egui::Button::new("Add spell"))
            .clicked()
        {
            if let Some(entry) = state.custom.take() {
                *outcome = PickerOutcome::Picked(SpellRef::Custom {
                    entry,
                    source: state.source,
                    notes: Vec::new(),
                });
            }
        }
    });
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

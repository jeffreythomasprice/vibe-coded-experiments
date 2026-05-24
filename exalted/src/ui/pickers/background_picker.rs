//! Background picker: 11 core entries from the rules database, plus a
//! "Custom…" path that lets the user fully specify a homebrew background.
//! Picking a database entry produces a `BackgroundRef::Lookup`; the custom
//! path produces a `BackgroundRef::Custom` with a full `BackgroundEntry`
//! embedded inline.

use crate::character::{BackgroundKind, BackgroundRef, RatedTrait};
use crate::rules::database::{BackgroundEntry, database};
use crate::ui::pickers::PickerOutcome;
use crate::ui::widgets::custom_entry::background_entry_form;

pub struct BackgroundPickerState {
    pub selected_id: Option<String>,
    pub custom: Option<BackgroundEntry>,
}

impl BackgroundPickerState {
    pub fn new() -> Self {
        Self {
            selected_id: None,
            custom: None,
        }
    }
}

fn blank_custom_entry() -> BackgroundEntry {
    BackgroundEntry {
        id: String::new(),
        name: String::new(),
        kind: BackgroundKind::Allies,
        source: String::new(),
        pages: String::new(),
        description: String::new(),
    }
}

pub fn show(
    ctx: &egui::Context,
    state: &mut BackgroundPickerState,
) -> PickerOutcome<BackgroundRef> {
    let mut outcome = PickerOutcome::Stay;
    let db = database();
    let mut entries: Vec<&BackgroundEntry> = db.iter_backgrounds().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    egui::Window::new("Add Background")
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_size([620.0, 560.0])
        .show(ctx, |ui| {
            if state.custom.is_none() {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .max_height(ui.available_height() - 100.0)
                    .show(ui, |ui| {
                        for entry in &entries {
                            ui.push_id(&entry.id, |ui| {
                                let selected = state.selected_id.as_deref() == Some(&entry.id);
                                ui.horizontal(|ui| {
                                    if ui.selectable_label(selected, &entry.name).clicked() {
                                        state.selected_id = Some(entry.id.clone());
                                    }
                                    if ui.small_button("duplicate as custom…").clicked() {
                                        let mut dup = (*entry).clone();
                                        dup.id = String::new();
                                        state.custom = Some(dup);
                                        state.selected_id = None;
                                    }
                                });
                                if selected && !entry.description.is_empty() {
                                    ui.small(&entry.description);
                                }
                            });
                        }
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Custom…").clicked() {
                        state.custom = Some(blank_custom_entry());
                        state.selected_id = None;
                    }
                });
            } else if let Some(custom) = state.custom.as_mut() {
                ui.label(egui::RichText::new("Custom background").strong());
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .max_height(ui.available_height() - 80.0)
                    .show(ui, |ui| {
                        background_entry_form(ui, "bg-picker-custom", custom);
                    });
                if ui.button("Cancel custom").clicked() {
                    state.custom = None;
                }
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    outcome = PickerOutcome::Cancelled;
                }
                let lookup_ready = state.custom.is_none() && state.selected_id.is_some();
                let custom_ready = state
                    .custom
                    .as_ref()
                    .is_some_and(|c| !c.id.is_empty() && !c.name.is_empty());
                if ui
                    .add_enabled(
                        lookup_ready || custom_ready,
                        egui::Button::new("Add background"),
                    )
                    .clicked()
                {
                    if let Some(custom) = state.custom.take() {
                        outcome = PickerOutcome::Picked(BackgroundRef::Custom {
                            entry: custom,
                            trait_: RatedTrait::with_base(0),
                            label: String::new(),
                            notes: Vec::new(),
                        });
                    } else if let Some(id) = state.selected_id.clone() {
                        outcome = PickerOutcome::Picked(BackgroundRef::lookup(
                            id,
                            RatedTrait::with_base(0),
                        ));
                    }
                }
            });
        });

    outcome
}

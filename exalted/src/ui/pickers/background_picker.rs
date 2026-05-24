//! Background picker: only 11 entries, but the user may also choose to
//! create a one-off `Custom` background. The picker yields a fresh
//! `BackgroundRef` with an empty trait the section then lets the player rate.

use crate::character::{BackgroundKind, BackgroundRef, RatedTrait};
use crate::rules::database::{BackgroundEntry, database};
use crate::ui::pickers::PickerOutcome;

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
        .default_size([560.0, 480.0])
        .show(ctx, |ui| {
            if state.custom.is_none() {
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .max_height(ui.available_height() - 100.0)
                    .show(ui, |ui| {
                        for entry in &entries {
                            let selected = state.selected_id.as_deref() == Some(&entry.id);
                            if ui.selectable_label(selected, &entry.name).clicked() {
                                state.selected_id = Some(entry.id.clone());
                            }
                            if selected && !entry.description.is_empty() {
                                ui.small(&entry.description);
                            }
                        }
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Custom…").clicked() {
                        state.custom = Some(BackgroundEntry {
                            id: String::new(),
                            name: String::new(),
                            kind: BackgroundKind::Allies,
                            source: String::new(),
                            pages: String::new(),
                            description: String::new(),
                        });
                        state.selected_id = None;
                    }
                });
            } else if let Some(custom) = state.custom.as_mut() {
                ui.label(egui::RichText::new("Custom background").strong());
                ui.horizontal(|ui| {
                    ui.label("Id");
                    ui.add(egui::TextEdit::singleline(&mut custom.id).desired_width(180.0));
                    ui.label("Kind");
                    egui::ComboBox::from_id_salt("custom-bg-kind")
                        .selected_text(format!("{:?}", custom.kind))
                        .show_ui(ui, |ui| {
                            for k in BackgroundKind::ALL {
                                ui.selectable_value(&mut custom.kind, *k, format!("{:?}", k));
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut custom.name).desired_width(220.0));
                });
                ui.label("Description");
                ui.add(
                    egui::TextEdit::multiline(&mut custom.description)
                        .desired_width(f32::INFINITY)
                        .desired_rows(3),
                );
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

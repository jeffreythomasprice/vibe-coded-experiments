//! Arts picker: choose one of the 11 Thaumaturgy Arts from the rules database
//! and add it to the character at Initiate (Degree 1), paid with the selected
//! source. Further Degrees and Procedures are edited inline in the section.

use crate::character::{Character, DotPurchase, DotSource, OccultArt};
use crate::rules::database::{ArtEntry, database};
use crate::ui::pickers::PickerOutcome;
use crate::ui::widgets::dot_source::{DotSourceKind, dot_source_editor};
use egui_extras::{Column, TableBuilder};

const PICKER_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub struct ArtsPickerState {
    pub search: String,
    pub selected_id: Option<String>,
    pub source: DotSource,
}

impl ArtsPickerState {
    pub fn new_for_character(_c: &Character) -> Self {
        Self {
            search: String::new(),
            selected_id: None,
            source: DotSource::ChargenPriority,
        }
    }
}

pub fn show(
    ctx: &egui::Context,
    state: &mut ArtsPickerState,
    character: &Character,
) -> PickerOutcome<OccultArt> {
    let mut outcome = PickerOutcome::Stay;
    let db = database();

    // Arts the character already has — hide them from the list so each Art is
    // added once (further Degrees are bought inline, not by re-adding).
    let owned: std::collections::HashSet<&str> = character
        .occult_arts
        .iter()
        .map(|a| a.id.as_str())
        .collect();

    let mut entries: Vec<&ArtEntry> = db
        .iter_arts()
        .filter(|e| !owned.contains(e.id.as_str()) && passes_filter(e, state))
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    egui::Window::new("Add Occult Art")
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_size([680.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.add(
                    egui::TextEdit::singleline(&mut state.search)
                        .desired_width(240.0)
                        .hint_text("name / id"),
                );
            });
            ui.separator();
            ui.label(format!("{} match(es)", entries.len()));

            let row_h = ui.spacing().interact_size.y;
            let available_body = (ui.available_height() - 190.0).max(120.0);

            TableBuilder::new(ui)
                .id_salt("arts-picker-results")
                .striped(true)
                .resizable(false)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::initial(260.0).at_least(160.0).clip(true))
                .column(Column::remainder().at_least(120.0))
                .min_scrolled_height(available_body)
                .max_scroll_height(available_body)
                .header(row_h, |mut header| {
                    header.col(|ui| {
                        ui.strong("Art");
                    });
                    header.col(|ui| {
                        ui.strong("Source");
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
                            ui.label(format!("{} p.{}", entry.source, entry.pages));
                        });
                    });
                });

            ui.separator();

            if let Some(id) = state.selected_id.clone() {
                if let Some(entry) = db.art(&id) {
                    ui.label(egui::RichText::new(&entry.name).strong());
                    let reqs = if entry.requirements.is_empty() {
                        "universal Occult ladder only (Initiate 1 / Adept 3 / Master 5)".to_string()
                    } else {
                        entry
                            .requirements
                            .iter()
                            .map(|r| {
                                let ab = if r.focus.is_empty() {
                                    format!("{:?}", r.ability)
                                } else {
                                    format!("Craft({})", r.focus)
                                };
                                format!("{} {} @D{}", ab, r.min, r.degree)
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    ui.small(format!("Requirements: {}", reqs));
                    ui.small(&entry.description);
                }
            } else {
                ui.label("(select an Art above)");
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Pay first Degree with");
                dot_source_editor(ui, "arts-picker-source", &mut state.source, PICKER_SOURCES);
            });

            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    outcome = PickerOutcome::Cancelled;
                }
                let can_add = state.selected_id.is_some();
                if ui
                    .add_enabled(can_add, egui::Button::new("Add art (Initiate)"))
                    .clicked()
                    && let Some(id) = state.selected_id.clone()
                {
                    let mut art = OccultArt::lookup(id);
                    // Start at Initiate (Degree 1): one purchase with the
                    // chosen source. Adept/Master are added in the section.
                    art.rating.purchases.push(DotPurchase::new(state.source));
                    outcome = PickerOutcome::Picked(art);
                }
            });
        });

    outcome
}

fn passes_filter(entry: &ArtEntry, state: &ArtsPickerState) -> bool {
    if state.search.is_empty() {
        return true;
    }
    let needle = state.search.to_ascii_lowercase();
    entry.name.to_ascii_lowercase().contains(&needle)
        || entry.id.to_ascii_lowercase().contains(&needle)
        || entry.description.to_ascii_lowercase().contains(&needle)
}

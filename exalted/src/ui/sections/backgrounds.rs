//! Backgrounds section: list each `BackgroundRef`, edit its rated trait,
//! label (for disambiguation), and notes; add new entries via the picker.

use crate::character::{BackgroundRef, DotSource};
use crate::rules::database::database;
use crate::ui::pickers::background_picker::BackgroundPickerState;
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::DotSourceKind;
use crate::ui::widgets::icon_button::trash_button_with_label;
use crate::ui::widgets::notes_list::notes_editor;
use crate::ui::widgets::rated_trait::{rated_trait_editor, RatedTraitOpts};

const BG_TRAIT_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.backgrounds.len();
    ui.heading(format!("Backgrounds ({})", count));

    if ui.button("+ Add background…").clicked() {
        state.background_picker = Some(BackgroundPickerState::new());
    }

    let db = database();
    let default_source = if state.character.is_in_chargen() {
        DotSource::ChargenPriority
    } else {
        DotSource::Xp { spent: 0 }
    };
    let mut any_changed = false;
    let mut delete_idx: Option<usize> = None;
    for (i, bg) in state.character.backgrounds.iter_mut().enumerate() {
        let name = bg.display_name(db).to_string();
        let label = bg.label().to_string();
        let header = if label.is_empty() {
            name.clone()
        } else {
            format!("{} — {}", name, label)
        };
        egui::CollapsingHeader::new(header)
            .id_salt(("bg", i))
            .default_open(false)
            .show(ui, |ui| {
                let (label, trait_, notes) = match bg {
                    BackgroundRef::Lookup {
                        label, trait_, notes, ..
                    } => (label, trait_, notes),
                    BackgroundRef::Custom {
                        label, trait_, notes, ..
                    } => (label, trait_, notes),
                };
                {
                    ui.horizontal(|ui| {
                        ui.label("Label");
                        let resp = ui.add(
                            egui::TextEdit::singleline(label)
                                .desired_width(220.0)
                                .hint_text("e.g. \"Realm\" or specific artifact name"),
                        );
                        if resp.changed() {
                            any_changed = true;
                        }
                        if trash_button_with_label(ui, "remove").clicked() {
                            delete_idx = Some(i);
                        }
                    });

                    let opts = RatedTraitOpts {
                        label: &name,
                        max_dots: 5,
                        allowed_sources: BG_TRAIT_SOURCES,
                        default_add_source: default_source,
                        show_specialties: false,
                    };
                    if rated_trait_editor(ui, ("bg-trait", i), trait_, &opts) {
                        any_changed = true;
                    }

                    ui.add_space(4.0);
                    ui.label("Notes");
                    if notes_editor(ui, ("bg-notes", i), notes) {
                        any_changed = true;
                    }
                }
            });
    }
    if let Some(i) = delete_idx {
        state.character.backgrounds.remove(i);
        any_changed = true;
    }
    if any_changed {
        state.mark_dirty();
    }
}

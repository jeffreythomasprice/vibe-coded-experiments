//! Languages section: Vec<KnownLanguage>. Each entry is either one of the 9
//! named families or a TribalTongue(name). Exactly one entry must be marked
//! `native`.

use crate::character::{KnownLanguage, LanguageFamily};
use crate::ui::state::AppState;
use crate::ui::widgets::icon_button::trash_button;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.languages.len();
    ui.heading(format!("Languages ({})", count));
    ui.small(
        "One must be native; 1 + Linguistics non-tribal others; up to 4 × \
         Linguistics tribal tongues. Validation in the bottom panel.",
    );

    ui.horizontal(|ui| {
        if ui.button("+ Add language").clicked() {
            state.character.languages.push(KnownLanguage {
                family: LanguageFamily::LowRealm,
                dialect_specialty: None,
                native: false,
            });
            state.mark_dirty();
        }
        if ui.button("+ Add tribal tongue").clicked() {
            state.character.languages.push(KnownLanguage {
                family: LanguageFamily::TribalTongue(String::new()),
                dialect_specialty: None,
                native: false,
            });
            state.mark_dirty();
        }
    });

    let mut any_changed = false;
    let mut delete_idx: Option<usize> = None;
    for (i, lang) in state.character.languages.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            // Family combo. For tribal tongues we render an inline text field.
            match &mut lang.family {
                LanguageFamily::TribalTongue(name) => {
                    ui.label("Tribal");
                    let resp = ui.add(
                        egui::TextEdit::singleline(name)
                            .desired_width(160.0)
                            .hint_text("tribe name"),
                    );
                    if resp.changed() {
                        any_changed = true;
                    }
                }
                other => {
                    let prev = other.clone();
                    egui::ComboBox::from_id_salt(("language-family", i))
                        .selected_text(family_label(other))
                        .show_ui(ui, |ui| {
                            for f in LanguageFamily::NAMED {
                                ui.selectable_value(other, f.clone(), family_label(f));
                            }
                        });
                    if *other != prev {
                        any_changed = true;
                    }
                }
            }

            // Dialect specialty (optional; empty / whitespace is omitted on save).
            let mut dialect = lang.dialect_specialty.clone().unwrap_or_default();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut dialect)
                    .desired_width(140.0)
                    .hint_text("dialect (optional)"),
            );
            if resp.changed() {
                lang.dialect_specialty = if dialect.is_empty() {
                    None
                } else {
                    Some(dialect)
                };
                any_changed = true;
            }

            // Native checkbox.
            if ui.checkbox(&mut lang.native, "native").changed() {
                any_changed = true;
            }

            if trash_button(ui).clicked() {
                delete_idx = Some(i);
            }
        });
    }
    if let Some(i) = delete_idx {
        state.character.languages.remove(i);
        any_changed = true;
    }
    if any_changed {
        state.mark_dirty();
    }
}

fn family_label(f: &LanguageFamily) -> &'static str {
    match f {
        LanguageFamily::HighRealm => "High Realm",
        LanguageFamily::LowRealm => "Low Realm",
        LanguageFamily::OldRealm => "Old Realm",
        LanguageFamily::Riverspeak => "Riverspeak",
        LanguageFamily::Skytongue => "Skytongue",
        LanguageFamily::Flametongue => "Flametongue",
        LanguageFamily::Seatongue => "Seatongue",
        LanguageFamily::ForestTongue => "Forest Tongue",
        LanguageFamily::GuildCant => "Guild Cant",
        LanguageFamily::TribalTongue(_) => "Tribal Tongue",
    }
}

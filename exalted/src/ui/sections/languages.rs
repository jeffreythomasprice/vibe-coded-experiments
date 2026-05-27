//! Languages section: Vec<KnownLanguage>. Each entry is either one of the 9
//! named families or a TribalTongue(name). Exactly one entry must be marked
//! `native`.

use crate::character::{KnownLanguage, LanguageFamily};
use crate::ui::search::{self, MatchTarget, SectionId, TextEditOpts};
use crate::ui::state::AppState;
use crate::ui::widgets::icon_button::trash_button;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.languages.len();
    let heading_hl = state
        .search
        .highlight_for(MatchTarget::SectionHeading(SectionId::Languages));
    search::highlight_heading(
        ui,
        &format!("{} ({})", SectionId::Languages.label(), count),
        heading_hl,
        state.search.scroll_pending,
    );
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
            state.mark_dirty_with("languages.add");
        }
        if ui.button("+ Add tribal tongue").clicked() {
            state.character.languages.push(KnownLanguage {
                family: LanguageFamily::TribalTongue(String::new()),
                dialect_specialty: None,
                native: false,
            });
            state.mark_dirty_with("languages.add_tribal");
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
                    let hl = state.search.highlight_for(MatchTarget::LanguageTribal(i));
                    let resp = search::highlighted_singleline(
                        ui,
                        name,
                        &state.search.query,
                        hl,
                        TextEditOpts {
                            desired_width: 160.0,
                            hint: Some("tribe name"),
                        },
                        state.search.scroll_pending,
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
            let hl = state.search.highlight_for(MatchTarget::LanguageDialect(i));
            let resp = search::highlighted_singleline(
                ui,
                &mut dialect,
                &state.search.query,
                hl,
                TextEditOpts {
                    desired_width: 140.0,
                    hint: Some("dialect (optional)"),
                },
                state.search.scroll_pending,
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
        state.mark_dirty_with("languages.edit");
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

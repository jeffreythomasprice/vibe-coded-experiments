//! Occult Arts (Thaumaturgy) section: list the character's Arts, edit each
//! one's Degree (0-3) and its Procedures, add/remove Arts via the picker.
//!
//! Not wired into the Ctrl+F search index (unlike Charms/Spells); the global
//! search bar won't jump to this section. Everything else — editing, dirty
//! tracking, TOML round-trip — behaves like the other sections.

use crate::character::{DotSource, Note, Procedure};
use crate::rules::database::database;
use crate::ui::pickers::arts_picker::ArtsPickerState;
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::{DotSourceKind, dot_source_editor};
use crate::ui::widgets::icon_button::{trash_button, trash_button_with_label};
use crate::ui::widgets::rated_trait::{RatedTraitOpts, rated_trait_editor};

const ART_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

fn degree_label(degree: u8) -> &'static str {
    match degree {
        0 => "Apprentice",
        1 => "Initiate",
        2 => "Adept",
        3 => "Master",
        _ => "Degree >3",
    }
}

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let db = database();
    let bonus = state.character.thaumaturgy_dice_bonus();
    ui.heading(format!(
        "Occult Arts ({}) — +{} thaumaturgy dice",
        state.character.occult_arts.len(),
        bonus
    ));

    if ui.button("+ Add art…").clicked() {
        tracing::debug!(picker = "arts", "opened picker");
        state.arts_picker = Some(ArtsPickerState::new_for_character(&state.character));
    }

    let default_source = if state.character.is_in_chargen() {
        DotSource::ChargenPriority
    } else {
        DotSource::Xp { spent: 0 }
    };

    let mut any_changed = false;
    let mut delete_idx: Option<usize> = None;

    for (i, art) in state.character.occult_arts.iter_mut().enumerate() {
        let name = match art.entry(db) {
            Some(e) => e.name.clone(),
            None => format!("{} (unknown id)", art.id),
        };
        let degree = art.degree();
        let header = format!("{} — {} ({})", name, degree_label(degree), degree);

        egui::CollapsingHeader::new(header)
            .id_salt(("occult-art", i))
            .default_open(false)
            .show(ui, |ui| {
                // Requirements reminder from the rules database.
                if let Some(entry) = art.entry(db) {
                    if entry.requirements.is_empty() {
                        ui.small(
                            "Requires only the Occult ladder: Initiate 1 / Adept 3 / Master 5.",
                        );
                    } else {
                        let reqs: Vec<String> = entry
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
                            .collect();
                        ui.small(format!(
                            "Also requires: {} (plus Occult 1/3/5).",
                            reqs.join(", ")
                        ));
                    }
                }

                // Degree editor (0-3), reusing the standard RatedTrait widget.
                let mut opts = RatedTraitOpts {
                    label: "Degree",
                    max_dots: 3,
                    allowed_sources: ART_SOURCES,
                    default_add_source: default_source,
                    show_specialties: false,
                    selectable: None,
                    search: None,
                    label_target: None,
                    specialty_ability: None,
                };
                if rated_trait_editor(ui, ("art-degree", i), &mut art.rating, &mut opts) {
                    any_changed = true;
                }

                ui.add_space(4.0);

                // Procedures.
                let proc_header = if art.procedures.is_empty() {
                    "Procedures".to_string()
                } else {
                    format!("Procedures ({})", art.procedures.len())
                };
                egui::CollapsingHeader::new(proc_header)
                    .id_salt(("art-procs", i))
                    .default_open(!art.procedures.is_empty())
                    .show(ui, |ui| {
                        let mut del_proc: Option<usize> = None;
                        for (pi, proc) in art.procedures.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut proc.name)
                                            .desired_width(180.0)
                                            .hint_text("ritual name"),
                                    )
                                    .changed()
                                {
                                    any_changed = true;
                                }
                                ui.label("rank");
                                let mut d = proc.degree as u32;
                                if ui
                                    .add(egui::DragValue::new(&mut d).range(0u32..=3))
                                    .changed()
                                {
                                    proc.degree = d.min(3) as u8;
                                    any_changed = true;
                                }
                                if dot_source_editor(
                                    ui,
                                    ("art-proc-src", i, pi),
                                    &mut proc.source,
                                    ART_SOURCES,
                                ) {
                                    any_changed = true;
                                }
                                if trash_button(ui).clicked() {
                                    del_proc = Some(pi);
                                }
                            });
                        }
                        if let Some(pi) = del_proc {
                            art.procedures.remove(pi);
                            any_changed = true;
                        }
                        if ui.button("+ Add procedure").clicked() {
                            art.procedures.push(Procedure::new("", 0, default_source));
                            any_changed = true;
                        }
                    });

                // Notes — lightweight inline editor (add + edit body).
                ui.add_space(4.0);
                ui.label("Notes");
                let mut del_note: Option<usize> = None;
                for (ni, note) in art.notes.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let mut body = note.body.clone();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut body)
                                    .desired_width(320.0)
                                    .hint_text("note"),
                            )
                            .changed()
                        {
                            note.edit(body);
                            any_changed = true;
                        }
                        if trash_button(ui).clicked() {
                            del_note = Some(ni);
                        }
                    });
                }
                if let Some(ni) = del_note {
                    art.notes.remove(ni);
                    any_changed = true;
                }
                if ui.button("+ Add note").clicked() {
                    art.notes.push(Note::new(""));
                    any_changed = true;
                }

                ui.add_space(6.0);
                ui.separator();
                if trash_button_with_label(ui, "Remove art").clicked() {
                    delete_idx = Some(i);
                }
            });
    }

    if let Some(i) = delete_idx {
        state.character.occult_arts.remove(i);
        any_changed = true;
    }
    if any_changed {
        state.mark_dirty_with("occult_arts.edit");
    }
}

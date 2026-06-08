//! Abilities section: 3×2 grid by caste with per-row favored checkbox and
//! inline specialties. Top row: Dawn / Zenith / Twilight. Bottom row:
//! Night / Eclipse / (empty).

use crate::character::{AbilityKind, Caste, Craft, DotSource, RatedTrait};
use crate::render::names::{ability_name, caste_name};
use crate::ui::search::{self, MatchTarget, SectionId};
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::DotSourceKind;
use crate::ui::widgets::icon_button::trash_button;
use crate::ui::widgets::rated_trait::{
    RatedTraitOpts, Selectable, rated_trait_editor, rated_trait_editor_with_prefix,
};

const ABILITY_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

/// Canonical Craft focuses offered as autocomplete suggestions. Free text is
/// still allowed for homebrew crafts.
const CRAFT_SUGGESTIONS: &[&str] = &[
    "Air",
    "Earth",
    "Fire",
    "Water",
    "Wood",
    "Magitech",
    "Genesis",
    "Fate",
    "Glamour",
    "Moliation",
];

const TOP_ROW: [Caste; 3] = [Caste::Dawn, Caste::Zenith, Caste::Twilight];
const BOTTOM_ROW: [Caste; 2] = [Caste::Night, Caste::Eclipse];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let heading_hl = state
        .search
        .highlight_for(MatchTarget::SectionHeading(SectionId::Abilities));
    search::highlight_heading(
        ui,
        SectionId::Abilities.label(),
        heading_hl,
        state.search.scroll_pending,
    );
    ui.small(format!(
        "Favored: {}/5 selected. Caste abilities are always favored.",
        state.character.favored_abilities.len(),
    ));
    ui.small("Specialties are capped at 3 per ability except Linguistics.");

    let default_source = if state.character.is_in_chargen() {
        DotSource::ChargenPriority
    } else {
        DotSource::Xp { spent: 0 }
    };
    let mut any_changed = false;
    let mut clicked_ability: Option<AbilityKind> = None;
    let selected_ability = state.selection.ability;

    egui::Grid::new("abilities-grid")
        .num_columns(3)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for c in TOP_ROW {
                ui.vertical(|ui| {
                    render_caste_header(ui, state, c);
                });
            }
            ui.end_row();
            for c in TOP_ROW {
                ui.vertical(|ui| {
                    for ab in c.caste_abilities() {
                        if *ab == AbilityKind::Craft {
                            render_crafts(ui, state, default_source, &mut any_changed);
                        } else {
                            render_ability(
                                ui,
                                state,
                                *ab,
                                default_source,
                                selected_ability,
                                &mut clicked_ability,
                                &mut any_changed,
                            );
                        }
                    }
                });
            }
            ui.end_row();
            for c in BOTTOM_ROW {
                ui.vertical(|ui| {
                    render_caste_header(ui, state, c);
                });
            }
            ui.vertical(|_| {});
            ui.end_row();
            for c in BOTTOM_ROW {
                ui.vertical(|ui| {
                    for ab in c.caste_abilities() {
                        if *ab == AbilityKind::Craft {
                            render_crafts(ui, state, default_source, &mut any_changed);
                        } else {
                            render_ability(
                                ui,
                                state,
                                *ab,
                                default_source,
                                selected_ability,
                                &mut clicked_ability,
                                &mut any_changed,
                            );
                        }
                    }
                });
            }
            ui.vertical(|_| {});
            ui.end_row();
        });

    if let Some(ab) = clicked_ability {
        state.selection.toggle_ability(ab);
    }
    if any_changed {
        state.mark_dirty_with("abilities");
    }
}

fn render_caste_header(ui: &mut egui::Ui, state: &AppState, c: Caste) {
    let hl = state.search.highlight_for(MatchTarget::CasteHeading(c));
    let text = egui::RichText::new(caste_name(c)).strong().underline();
    match hl {
        None => {
            ui.label(text);
        }
        Some(k) => {
            let resp = egui::Frame::default()
                .fill(search::highlight_fill(k))
                .inner_margin(egui::Margin::symmetric(3, 0))
                .corner_radius(2)
                .show(ui, |ui| ui.label(text.color(egui::Color32::BLACK)))
                .inner;
            if k == search::HighlightKind::Focused && state.search.scroll_pending {
                resp.scroll_to_me(Some(egui::Align::Center));
            }
        }
    }
}

fn render_ability(
    ui: &mut egui::Ui,
    state: &mut AppState,
    ab: AbilityKind,
    default_source: DotSource,
    selected_ability: Option<AbilityKind>,
    clicked_ability: &mut Option<AbilityKind>,
    any_changed: &mut bool,
) {
    let is_caste = state.character.is_caste_ability(ab);
    let is_favored = state.character.is_favored_ability(ab);
    let label = ability_name(ab).to_string();
    let entry = state
        .character
        .abilities
        .entry(ab)
        .or_insert_with(|| RatedTrait::with_base(0));
    let mut clicked = false;
    let mut opts = RatedTraitOpts {
        label: &label,
        max_dots: 5,
        allowed_sources: ABILITY_SOURCES,
        default_add_source: default_source,
        show_specialties: true,
        selectable: Some(Selectable {
            is_selected: selected_ability == Some(ab),
            clicked: &mut clicked,
        }),
        search: Some(&state.search),
        label_target: Some(MatchTarget::AbilityLabel(ab)),
        specialty_ability: Some(ab),
    };
    let mut favored_toggle: Option<bool> = None;
    let row_changed = rated_trait_editor_with_prefix(
        ui,
        ("ability", ab as usize),
        entry,
        &mut opts,
        |ui| {
            let mut checked = is_caste || is_favored;
            let resp = ui.add_enabled(!is_caste, egui::Checkbox::new(&mut checked, ""));
            let resp = if is_caste {
                resp.on_disabled_hover_text(
                    "Checked when this is a favored ability. Locked on because caste abilities are always favored.",
                )
            } else {
                resp.on_hover_text("Checked when this is a favored ability.")
            };
            if !is_caste && resp.changed() {
                favored_toggle = Some(checked);
                true
            } else {
                false
            }
        },
    );
    if clicked {
        *clicked_ability = Some(ab);
    }
    if let Some(now_checked) = favored_toggle {
        if now_checked && !is_favored {
            state.character.favored_abilities.push(ab);
        } else if !now_checked && is_favored {
            state.character.favored_abilities.retain(|x| *x != ab);
        }
    }
    if row_changed {
        *any_changed = true;
    }
}

/// Render the Craft "ability" — which is really a family of separately-rated
/// crafts (Craft: Water, Craft: Fire, …) sharing one Caste/Favored slot. The
/// first craft is the primary one that lands on the sheet's single Craft row;
/// the rest ride the specialty rows. Shows a shared favored checkbox, then one
/// editor per craft (focus name + rating + specialties), plus add/remove and a
/// "make primary" control.
fn render_crafts(
    ui: &mut egui::Ui,
    state: &mut AppState,
    default_source: DotSource,
    any_changed: &mut bool,
) {
    let is_caste = state.character.is_caste_ability(AbilityKind::Craft);
    let is_favored = state.character.is_favored_ability(AbilityKind::Craft);

    // Shared Caste/Favored checkbox + group label.
    ui.horizontal(|ui| {
        let mut checked = is_caste || is_favored;
        let resp = ui.add_enabled(!is_caste, egui::Checkbox::new(&mut checked, ""));
        let resp = if is_caste {
            resp.on_disabled_hover_text(
                "Checked when Craft is a favored ability. Locked on because caste abilities are always favored.",
            )
        } else {
            resp.on_hover_text("Checked when Craft is a favored ability. Applies to every craft.")
        };
        if !is_caste && resp.changed() {
            if checked && !is_favored {
                state.character.favored_abilities.push(AbilityKind::Craft);
            } else if !checked && is_favored {
                state
                    .character
                    .favored_abilities
                    .retain(|x| *x != AbilityKind::Craft);
            }
            *any_changed = true;
        }
        let label_hl = state
            .search
            .highlight_for(MatchTarget::AbilityLabel(AbilityKind::Craft));
        if label_hl.is_some() {
            search::highlight_label(ui, "Craft", label_hl, state.search.scroll_pending);
        } else {
            ui.add_sized([140.0, 0.0], egui::Label::new(egui::RichText::new("Craft").strong()));
        }
    });

    let mut remove_idx: Option<usize> = None;
    let mut make_primary_idx: Option<usize> = None;
    let n = state.character.crafts.len();
    for i in 0..n {
        ui.horizontal(|ui| {
            // Focus name + canonical-craft suggestions.
            let focus = &mut state.character.crafts[i].focus;
            let resp = ui.add(
                egui::TextEdit::singleline(focus)
                    .hint_text("focus (e.g. Water)")
                    .desired_width(150.0),
            );
            if resp.changed() {
                *any_changed = true;
            }
            ui.menu_button("▾", |ui| {
                for s in CRAFT_SUGGESTIONS {
                    if ui.button(*s).clicked() {
                        *focus = (*s).to_string();
                        *any_changed = true;
                        ui.close();
                    }
                }
            });
            if i == 0 {
                ui.small("primary");
            } else if ui
                .small_button("★")
                .on_hover_text("Make this the primary craft (the one on the Craft row)")
                .clicked()
            {
                make_primary_idx = Some(i);
            }
            if trash_button(ui).clicked() {
                remove_idx = Some(i);
            }
        });

        // Rating + specialties for this craft. Empty label keeps the dot row
        // aligned with the other ability rows in the column.
        let mut opts = RatedTraitOpts {
            label: "",
            max_dots: 5,
            allowed_sources: ABILITY_SOURCES,
            default_add_source: default_source,
            show_specialties: true,
            selectable: None,
            search: Some(&state.search),
            label_target: None,
            // Craft specialties aren't individually search-addressable yet.
            specialty_ability: None,
        };
        if rated_trait_editor(
            ui,
            ("craft", i),
            &mut state.character.crafts[i].rating,
            &mut opts,
        ) {
            *any_changed = true;
        }
    }

    if let Some(i) = make_primary_idx {
        let craft = state.character.crafts.remove(i);
        state.character.crafts.insert(0, craft);
        *any_changed = true;
    }
    if let Some(i) = remove_idx {
        state.character.crafts.remove(i);
        *any_changed = true;
    }

    if ui.button("+ Add Craft").clicked() {
        state.character.crafts.push(Craft::new(String::new(), 0));
        *any_changed = true;
    }
}

//! Abilities section: 3×2 grid by caste with per-row favored checkbox and
//! inline specialties. Top row: Dawn / Zenith / Twilight. Bottom row:
//! Night / Eclipse / (empty).

use crate::character::{AbilityKind, Caste, DotSource, RatedTrait};
use crate::render::names::{ability_name, caste_name};
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::DotSourceKind;
use crate::ui::widgets::rated_trait::{RatedTraitOpts, Selectable, rated_trait_editor_with_prefix};

const ABILITY_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

const TOP_ROW: [Caste; 3] = [Caste::Dawn, Caste::Zenith, Caste::Twilight];
const BOTTOM_ROW: [Caste; 2] = [Caste::Night, Caste::Eclipse];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Abilities");
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
                    ui.label(egui::RichText::new(caste_name(c)).strong().underline());
                });
            }
            ui.end_row();
            for c in TOP_ROW {
                ui.vertical(|ui| {
                    for ab in c.caste_abilities() {
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
                });
            }
            ui.end_row();
            for c in BOTTOM_ROW {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(caste_name(c)).strong().underline());
                });
            }
            ui.vertical(|_| {});
            ui.end_row();
            for c in BOTTOM_ROW {
                ui.vertical(|ui| {
                    for ab in c.caste_abilities() {
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

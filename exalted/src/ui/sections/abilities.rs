//! Abilities section: all 25 abilities with a per-row favored checkbox and
//! specialties.

use crate::character::{AbilityKind, DotSource, RatedTrait};
use crate::render::names::ability_name;
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::DotSourceKind;
use crate::ui::widgets::rated_trait::{RatedTraitOpts, rated_trait_editor_with_prefix};

const ABILITY_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

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
    for ab in AbilityKind::ALL {
        let ab = *ab;
        let is_caste = state.character.is_caste_ability(ab);
        let is_favored = state.character.is_favored_ability(ab);
        let label = ability_name(ab).to_string();
        let entry = state
            .character
            .abilities
            .entry(ab)
            .or_insert_with(|| RatedTrait::with_base(0));
        let opts = RatedTraitOpts {
            label: &label,
            max_dots: 5,
            allowed_sources: ABILITY_SOURCES,
            default_add_source: default_source,
            show_specialties: true,
        };
        let mut favored_toggle: Option<bool> = None;
        let row_changed = rated_trait_editor_with_prefix(
            ui,
            ("ability", ab as usize),
            entry,
            &opts,
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
        if let Some(now_checked) = favored_toggle {
            if now_checked && !is_favored {
                state.character.favored_abilities.push(ab);
            } else if !now_checked && is_favored {
                state.character.favored_abilities.retain(|x| *x != ab);
            }
        }
        if row_changed {
            any_changed = true;
        }
    }
    if any_changed {
        state.mark_dirty();
    }
}

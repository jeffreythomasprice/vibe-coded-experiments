//! Abilities section: all 25 abilities with C/F markers and specialties.

use crate::character::{AbilityKind, DotSource, RatedTrait};
use crate::render::names::ability_name;
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::DotSourceKind;
use crate::ui::widgets::rated_trait::{rated_trait_editor, RatedTraitOpts};

const ABILITY_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Abilities");
    ui.small("C = Caste, F = Favored. Specialties are capped at 3 per ability except Linguistics.");

    let default_source = if state.character.is_in_chargen() {
        DotSource::ChargenPriority
    } else {
        DotSource::Xp { spent: 0 }
    };
    let mut any_changed = false;
    for ab in AbilityKind::ALL {
        let label_marker = match (
            state.character.is_caste_ability(*ab),
            state.character.is_favored_ability(*ab),
        ) {
            (true, _) => " (C)",
            (false, true) => " (F)",
            _ => "",
        };
        let label = format!("{}{}", ability_name(*ab), label_marker);
        let entry = state
            .character
            .abilities
            .entry(*ab)
            .or_insert_with(|| RatedTrait::with_base(0));
        let opts = RatedTraitOpts {
            label: &label,
            max_dots: 5,
            allowed_sources: ABILITY_SOURCES,
            default_add_source: default_source,
            show_specialties: true,
        };
        if rated_trait_editor(ui, ("ability", *ab as usize), entry, &opts) {
            any_changed = true;
        }
    }
    if any_changed {
        state.mark_dirty();
    }
}

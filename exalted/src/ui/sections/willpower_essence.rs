//! Willpower & Essence section.

use crate::character::DotSource;
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::DotSourceKind;
use crate::ui::widgets::rated_trait::{rated_trait_editor, RatedTraitOpts};

const WP_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::Base,
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

const ESSENCE_SOURCES: &[DotSourceKind] =
    &[DotSourceKind::Base, DotSourceKind::BonusPoints, DotSourceKind::Xp];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Willpower & Essence");
    ui.small(
        "Willpower's base normally equals the sum of the two highest Virtues. \
         Essence caps at 5 during chargen.",
    );

    let opts_wp = RatedTraitOpts {
        label: "Willpower",
        max_dots: 10,
        allowed_sources: WP_SOURCES,
        default_add_source: DotSource::BonusPoints { spent: 2 },
        show_specialties: false,
    };
    let mut any = false;
    if rated_trait_editor(ui, "willpower", &mut state.character.willpower, &opts_wp) {
        any = true;
    }

    let opts_essence = RatedTraitOpts {
        label: "Essence",
        max_dots: 10,
        allowed_sources: ESSENCE_SOURCES,
        default_add_source: DotSource::Xp { spent: 0 },
        show_specialties: false,
    };
    if rated_trait_editor(ui, "essence", &mut state.character.essence, &opts_essence) {
        any = true;
    }

    if any {
        state.mark_dirty();
    }
}

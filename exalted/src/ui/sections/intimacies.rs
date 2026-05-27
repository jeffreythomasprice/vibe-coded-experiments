//! Intimacies section: Vec<Intimacy> editor.

use crate::character::{DotSource, Intimacy, IntimacyKind};
use crate::ui::search::{self, MatchTarget, SectionId, TextEditOpts};
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::{DotSourceKind, dot_source_editor};
use crate::ui::widgets::icon_button::trash_button;

const INTIMACY_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::Base,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

const KINDS: &[(IntimacyKind, &str)] = &[
    (IntimacyKind::Person, "Person"),
    (IntimacyKind::Place, "Place"),
    (IntimacyKind::Cause, "Cause"),
    (IntimacyKind::Ideal, "Ideal"),
    (IntimacyKind::Other, "Other"),
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.intimacies.len();
    let heading_hl = state
        .search
        .highlight_for(MatchTarget::SectionHeading(SectionId::Intimacies));
    search::highlight_heading(
        ui,
        &format!("{} ({})", SectionId::Intimacies.label(), count),
        heading_hl,
        state.search.scroll_pending,
    );
    ui.small(
        "Compassion-baseline intimacies use source=Base. New ones acquired \
         in play cost 3 BP at chargen or 3 XP afterward.",
    );

    if ui.button("+ Add intimacy").clicked() {
        state.character.intimacies.push(Intimacy {
            description: String::new(),
            kind: IntimacyKind::Person,
            source: DotSource::Base,
            rating: 1,
        });
        state.mark_dirty_with("intimacies.add");
    }

    let mut any_changed = false;
    let mut delete_idx: Option<usize> = None;
    for (i, intimacy) in state.character.intimacies.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let highlight = state.search.highlight_for(MatchTarget::Intimacy(i));
            let resp = search::highlighted_singleline(
                ui,
                &mut intimacy.description,
                &state.search.query,
                highlight,
                TextEditOpts {
                    desired_width: 280.0,
                    hint: Some("description"),
                },
                state.search.scroll_pending,
            );
            if resp.changed() {
                any_changed = true;
            }
            let prev = intimacy.kind;
            egui::ComboBox::from_id_salt(("intimacy-kind", i))
                .selected_text(
                    KINDS
                        .iter()
                        .find(|(k, _)| *k == intimacy.kind)
                        .map(|(_, n)| *n)
                        .unwrap_or(""),
                )
                .show_ui(ui, |ui| {
                    for (k, n) in KINDS {
                        ui.selectable_value(&mut intimacy.kind, *k, *n);
                    }
                });
            if intimacy.kind != prev {
                any_changed = true;
            }
            let prev_rating = intimacy.rating;
            ui.add(
                egui::DragValue::new(&mut intimacy.rating)
                    .range(1u8..=10)
                    .prefix("rating "),
            );
            if intimacy.rating != prev_rating {
                any_changed = true;
            }
            if dot_source_editor(
                ui,
                ("intimacy-src", i),
                &mut intimacy.source,
                INTIMACY_SOURCES,
            ) {
                any_changed = true;
            }
            if trash_button(ui).clicked() {
                delete_idx = Some(i);
            }
        });
    }
    if let Some(i) = delete_idx {
        state.character.intimacies.remove(i);
        any_changed = true;
    }
    if any_changed {
        state.mark_dirty_with("intimacies.edit");
    }
}

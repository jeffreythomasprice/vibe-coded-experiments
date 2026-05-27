//! Character-level notes section.

use crate::ui::search::{self, MatchTarget, NoteParent, SectionId};
use crate::ui::state::AppState;
use crate::ui::widgets::notes_list::notes_editor;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.notes.len();
    let heading_hl = state
        .search
        .highlight_for(MatchTarget::SectionHeading(SectionId::Notes));
    search::highlight_heading(
        ui,
        &format!("{} ({})", SectionId::Notes.label(), count),
        heading_hl,
        state.search.scroll_pending,
    );
    if notes_editor(
        ui,
        "character-notes",
        &mut state.character.notes,
        NoteParent::Character,
        &state.search,
    ) {
        state.mark_dirty_with("notes");
    }
}

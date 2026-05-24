//! Character-level notes section.

use crate::ui::state::AppState;
use crate::ui::widgets::notes_list::notes_editor;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.notes.len();
    ui.heading(format!("Notes ({})", count));
    if notes_editor(ui, "character-notes", &mut state.character.notes) {
        state.mark_dirty();
    }
}

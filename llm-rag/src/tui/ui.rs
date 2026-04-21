use ratatui::Frame;

use super::app::App;
use super::screens::{self, Screen};

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    match &app.screen {
        Screen::Chat(state) => screens::chat::render(frame, area, app, state),
        Screen::ConversationList(state) => screens::conversation_list::render(frame, area, state),
        Screen::TagEditor(state) => screens::tag_editor::render(frame, area, app, state),
        Screen::Search(state) => screens::search::render(frame, area, app, state),
        Screen::ConfirmDelete(state) => {
            // Render the ConversationList "behind" the modal so the user sees
            // the context they're acting on. Since we don't preserve the
            // previous list state once we transition, just paint the modal.
            screens::confirm::render(frame, area, state);
        }
        Screen::DocumentList(state) => screens::document_list::render(frame, area, state),
        Screen::DocumentSearch(state) => screens::document_search::render(frame, area, app, state),
        Screen::FilePicker(state) => screens::file_picker::render(frame, area, app, state),
    }
}

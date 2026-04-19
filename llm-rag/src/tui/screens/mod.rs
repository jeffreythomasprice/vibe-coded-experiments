//! One module per screen. Each screen owns its state struct, a render
//! function, a key handler that returns a [`Transition`], and (optionally)
//! a reply handler invoked when a server response arrives for its outstanding
//! request.

pub mod chat;
pub mod confirm;
pub mod conversation_list;
pub mod search;
pub mod tag_editor;

use crate::protocol::Request;

pub use chat::ChatState;
pub use confirm::ConfirmDeleteState;
pub use conversation_list::ConversationListState;
pub use search::SearchState;
pub use tag_editor::TagEditorState;

use crate::tui::commands::ScreenIntent;

/// All TUI screens.
pub enum Screen {
    Chat(ChatState),
    ConversationList(ConversationListState),
    TagEditor(TagEditorState),
    Search(SearchState),
    ConfirmDelete(ConfirmDeleteState),
}

impl Screen {
    /// Requests to fire automatically when this screen becomes active. The
    /// event loop dispatches each, captures the resulting seqs, and hands
    /// them back via [`Self::record_seqs`] so replies can be routed.
    pub fn on_enter(&self) -> Vec<Request> {
        match self {
            Screen::Chat(state) => state.pending_resume_request.clone().into_iter().collect(),
            Screen::ConversationList(_) => vec![conversation_list::initial_request()],
            Screen::TagEditor(state) => {
                let (a, b) = tag_editor::initial_requests(&state.conversation_id);
                vec![a, b]
            }
            Screen::Search(_) => search::initial_requests(),
            Screen::ConfirmDelete(_) => Vec::new(),
        }
    }

    /// Stamp the seqs of the `on_enter` requests back onto the screen, in
    /// the same order they were produced by [`Self::on_enter`].
    pub fn record_seqs(&mut self, seqs: &[u64]) {
        match self {
            Screen::Chat(state) => {
                if let Some(&x) = seqs.first() {
                    state.request_seq = x;
                }
            }
            Screen::ConversationList(state) => {
                if let Some(&x) = seqs.first() {
                    state.request_seq = x;
                }
            }
            Screen::TagEditor(state) => {
                if let Some(&x) = seqs.first() {
                    state.tags_seq = x;
                }
                if let Some(&x) = seqs.get(1) {
                    state.all_tags_seq = x;
                }
            }
            Screen::Search(state) => {
                if let Some(&x) = seqs.first() {
                    state.tags_seq = x;
                }
            }
            Screen::ConfirmDelete(_) => {}
        }
    }
}

/// Result of a screen's key-handling step. Applied by the outer event loop.
pub enum Transition {
    /// No-op; stay on the current screen.
    None,
    /// Replace the current screen with a new one. The event loop will call
    /// [`Screen::on_enter`] on the new screen and fire any initial requests.
    To(Screen),
    /// Exit the TUI.
    Quit,
}

/// Turn a [`ScreenIntent`] (raised by a slash command) into a concrete
/// transition. Called by the event loop when it sees [`Action::OpenScreen`].
pub fn open(intent: ScreenIntent) -> Transition {
    match intent {
        ScreenIntent::ConversationList => Transition::To(Screen::ConversationList(
            ConversationListState::new_loading(),
        )),
    }
}

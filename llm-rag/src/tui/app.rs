use super::commands::SlashCommand;
use super::screens::{ChatState, Screen};

/// Cross-screen UI state. Per-screen concerns (input widget, transcript,
/// conversation id, etc.) live inside [`Screen`] variants.
pub struct App {
    pub screen: Screen,
    /// Outstanding server requests; drives the input-box spinner.
    pub pending: usize,
    pub should_quit: bool,
    /// Monotonic request sequence. Every outgoing request takes the next seq
    /// and records it on the issuing screen; replies with a non-matching seq
    /// are dropped (they belong to a screen the user has navigated away from).
    pub next_request_seq: u64,
}

pub struct Autocomplete {
    pub matches: Vec<&'static SlashCommand>,
    pub selected: usize,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Chat(ChatState::new()),
            pending: 0,
            should_quit: false,
            next_request_seq: 0,
        }
    }
}

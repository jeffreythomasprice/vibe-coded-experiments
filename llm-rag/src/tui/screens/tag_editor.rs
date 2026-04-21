//! Tag editor: shows the tags attached to a conversation plus a text input
//! that both adds new tags and fuzzy-matches existing ones.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

use crate::error::ClientError;
use crate::protocol::{Request, Response};
use crate::tui::app::App;
use crate::tui::input;
use crate::tui::spinner;

use super::{Screen, Transition};

/// Where the user came from; an Esc transition lands back here (with a
/// refresh).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnScreen {
    ConversationList,
}

pub struct TagEditorState {
    pub conversation_id: String,
    pub conversation_label: String,
    pub tags: Vec<String>,
    pub selected_tag: usize,
    pub input: String,
    pub cursor: usize,
    pub suggestions: Vec<String>,
    pub selected_suggestion: Option<usize>,
    pub all_tags_cache: Vec<String>,
    pub return_screen: ReturnScreen,
    /// Seqs of the two initial loads (ConversationTags + TagList). Zeroed
    /// after each reply lands.
    pub tags_seq: u64,
    pub all_tags_seq: u64,
}

impl TagEditorState {
    pub fn loading(
        conversation_id: String,
        conversation_label: String,
        return_screen: ReturnScreen,
    ) -> Self {
        Self {
            conversation_id,
            conversation_label,
            tags: Vec::new(),
            selected_tag: 0,
            input: String::new(),
            cursor: 0,
            suggestions: Vec::new(),
            selected_suggestion: None,
            all_tags_cache: Vec::new(),
            return_screen,
            tags_seq: 0,
            all_tags_seq: 0,
        }
    }

    fn refresh_suggestions(&mut self) {
        self.suggestions = suggest(&self.all_tags_cache, &self.input, &self.tags);
        self.selected_suggestion = if self.suggestions.is_empty() {
            None
        } else {
            Some(0)
        };
    }
}

/// Initial load pair: current tags for this conv + global tag list.
pub fn initial_requests(conv_id: &str) -> (Request, Request) {
    (
        Request::ConversationTags {
            id: conv_id.to_string(),
        },
        Request::TagList,
    )
}

fn suggest(all: &[String], query: &str, excluded: &[String]) -> Vec<String> {
    if query.is_empty() {
        return Vec::new();
    }
    let q = query.to_lowercase();
    let mut scored: Vec<(usize, String)> = all
        .iter()
        .filter(|t| !excluded.iter().any(|e| e == *t))
        .filter_map(|t| t.to_lowercase().find(&q).map(|i| (i, t.clone())))
        .collect();
    scored.sort_by_key(|(i, t)| (*i, t.len(), t.clone()));
    scored.into_iter().map(|(_, t)| t).collect()
}

pub fn render(frame: &mut Frame, area: Rect, app: &App, state: &TagEditorState) {
    let (banner_area, header_area, tags_area, input_area, sugg_area, help_area) = if app.pending > 0
    {
        let c = Layout::vertical([
            Constraint::Length(1), // banner
            Constraint::Length(3), // header
            Constraint::Min(1),    // tags list
            Constraint::Length(3), // input box
            Constraint::Length(6), // suggestions
            Constraint::Length(1), // help
        ])
        .split(area);
        (Some(c[0]), c[1], c[2], c[3], c[4], c[5])
    } else {
        let c = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Length(1),
        ])
        .split(area);
        (None, c[0], c[1], c[2], c[3], c[4])
    };

    if let Some(b) = banner_area {
        spinner::render_banner(frame, b, app.spinner_frame);
    }

    let header = Paragraph::new(format!("editing: {}", state.conversation_label))
        .block(Block::bordered().title(" tag editor "));
    frame.render_widget(header, header_area);

    let tag_items: Vec<ListItem> = state
        .tags
        .iter()
        .map(|t| ListItem::new(Line::raw(t.clone())))
        .collect();
    let tags_list = List::new(tag_items)
        .block(Block::bordered().title(" tags "))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
    let mut ls = ListState::default();
    if !state.tags.is_empty() {
        ls.select(Some(state.selected_tag.min(state.tags.len() - 1)));
    }
    frame.render_stateful_widget(tags_list, tags_area, &mut ls);

    let input_title = if app.pending > 0 {
        format!(" add tag {} ", spinner::frame(app.spinner_frame))
    } else {
        " add tag ".to_string()
    };
    let input = Paragraph::new(state.input.as_str()).block(Block::bordered().title(input_title));
    frame.render_widget(input, input_area);

    let sugg_items: Vec<ListItem> = state
        .suggestions
        .iter()
        .map(|s| ListItem::new(Line::raw(s.clone())))
        .collect();
    let sugg_list = List::new(sugg_items)
        .block(Block::bordered().title(" suggestions "))
        .highlight_style(Style::default().fg(Color::Yellow));
    let mut sl = ListState::default();
    if let Some(i) = state.selected_suggestion {
        sl.select(Some(i));
    }
    frame.render_stateful_widget(sugg_list, sugg_area, &mut sl);

    let help = Paragraph::new(
        "enter: add · tab: accept suggestion · ↑↓: navigate · d/del/backspace (empty input): remove · esc: back",
    )
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, help_area);
}

pub fn handle_key(state: &mut TagEditorState, key: KeyEvent, waiting: bool) -> Option<TagAction> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    // While a request is pending, swallow everything that types into the
    // input or fires an RPC. Esc/Tab/arrows still work so the user can back
    // out or navigate the existing tags.
    if waiting && (input::is_buffer_edit(&key) || key.code == KeyCode::Enter) {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(TagAction::Back),
        KeyCode::Enter => {
            // Prefer a highlighted suggestion when present; otherwise treat
            // the raw input as a new tag.
            let tag = match state.selected_suggestion {
                Some(i) if !state.suggestions.is_empty() => state.suggestions[i].clone(),
                _ => state.input.trim().to_string(),
            };
            if tag.is_empty() {
                return None;
            }
            state.input.clear();
            state.cursor = 0;
            state.suggestions.clear();
            state.selected_suggestion = None;
            Some(TagAction::AddTag(tag))
        }
        KeyCode::Tab => {
            if let Some(i) = state.selected_suggestion
                && let Some(s) = state.suggestions.get(i)
            {
                state.input = s.clone();
                state.cursor = state.input.len();
                state.refresh_suggestions();
            }
            None
        }
        KeyCode::Up => {
            if state.input.is_empty() {
                if state.selected_tag > 0 {
                    state.selected_tag -= 1;
                }
            } else if !state.suggestions.is_empty() {
                let i = state.selected_suggestion.unwrap_or(0);
                let n = state.suggestions.len();
                state.selected_suggestion = Some((i + n - 1) % n);
            }
            None
        }
        KeyCode::Down => {
            if state.input.is_empty() {
                if state.selected_tag + 1 < state.tags.len() {
                    state.selected_tag += 1;
                }
            } else if !state.suggestions.is_empty() {
                let i = state.selected_suggestion.unwrap_or(0);
                state.selected_suggestion = Some((i + 1) % state.suggestions.len());
            }
            None
        }
        KeyCode::Backspace if state.input.is_empty() => remove_highlighted(state),
        KeyCode::Delete if state.input.is_empty() => remove_highlighted(state),
        KeyCode::Char('d')
            if state.input.is_empty() && !key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            remove_highlighted(state)
        }
        KeyCode::Backspace => {
            if input::delete_char_backward(&mut state.input, &mut state.cursor) {
                state.refresh_suggestions();
            }
            None
        }
        KeyCode::Delete => {
            if input::delete_char_forward(&mut state.input, &mut state.cursor) {
                state.refresh_suggestions();
            }
            None
        }
        KeyCode::Left => {
            input::cursor_left(&state.input, &mut state.cursor);
            None
        }
        KeyCode::Right => {
            input::cursor_right(&state.input, &mut state.cursor);
            None
        }
        KeyCode::Home => {
            input::cursor_home(&mut state.cursor);
            None
        }
        KeyCode::End => {
            input::cursor_end(&state.input, &mut state.cursor);
            None
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let changed = match c {
                    'w' => input::delete_word_backward(&mut state.input, &mut state.cursor),
                    'u' => input::delete_to_start(&mut state.input, &mut state.cursor),
                    'k' => input::delete_to_end(&mut state.input, &mut state.cursor),
                    'a' => {
                        input::cursor_home(&mut state.cursor);
                        false
                    }
                    'e' => {
                        input::cursor_end(&state.input, &mut state.cursor);
                        false
                    }
                    _ => false,
                };
                if changed {
                    state.refresh_suggestions();
                }
            } else {
                input::insert_char(&mut state.input, &mut state.cursor, c);
                state.refresh_suggestions();
            }
            None
        }
        _ => None,
    }
}

fn remove_highlighted(state: &mut TagEditorState) -> Option<TagAction> {
    let tag = state.tags.get(state.selected_tag)?.clone();
    Some(TagAction::RemoveTag(tag))
}

/// Per-screen action produced by key handling. The outer event loop turns
/// these into Transitions (e.g., Back → a new ConversationList screen) or
/// RPC requests.
pub enum TagAction {
    AddTag(String),
    RemoveTag(String),
    Back,
}

/// Apply a reply addressed to this screen. Drops anything whose seq doesn't
/// match one of our outstanding loads.
pub fn handle_reply(state: &mut TagEditorState, seq: u64, result: Result<Response, ClientError>) {
    if seq == state.tags_seq && seq != 0 {
        state.tags_seq = 0;
        if let Ok(Response::ConversationTags { tags }) = result {
            state.tags = tags;
            if state.selected_tag >= state.tags.len() && !state.tags.is_empty() {
                state.selected_tag = state.tags.len() - 1;
            }
            state.refresh_suggestions();
        }
        return;
    }
    if seq == state.all_tags_seq && seq != 0 {
        state.all_tags_seq = 0;
        if let Ok(Response::TagList { tags }) = result {
            state.all_tags_cache = tags;
            state.refresh_suggestions();
        }
        return;
    }
    // Otherwise: an ack of an AddTag / RemoveTag we just sent. Current tag
    // list has been updated optimistically already; nothing to do here.
    // (If the server returned an Error, we swallow it for now — a future
    // enhancement could surface it in a status line.)
    let _ = result;
}

/// Helper: build a Back transition (return to whatever screen we came from).
/// The event loop's [`Screen::on_enter`] will re-fire the screen's initial
/// loads so data is fresh.
pub fn back_transition(state: &TagEditorState) -> Transition {
    match state.return_screen {
        ReturnScreen::ConversationList => Transition::To(Screen::ConversationList(
            super::ConversationListState::new_loading(),
        )),
    }
}

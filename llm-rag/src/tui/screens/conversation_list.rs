//! Conversation picker: scrollable list, Enter to resume, `t` for tags,
//! `d`/Backspace/Delete to delete (confirm dialog), `s` to search, Esc back.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

use crate::protocol::{ConversationSummary, Request, Response};

use super::chat::ChatState;
use super::{Screen, Transition};

pub struct ConversationListState {
    pub items: Vec<ConversationSummary>,
    pub selected: usize,
    pub loading: bool,
    /// The seq of the currently-outstanding load request (or 0 when idle).
    pub request_seq: u64,
}

impl Default for ConversationListState {
    fn default() -> Self {
        Self::new_loading()
    }
}

impl ConversationListState {
    /// Start in the loading state; the event loop will kick off a
    /// `Request::ConversationList` and record the seq via `set_request_seq`.
    pub fn new_loading() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            loading: true,
            request_seq: 0,
        }
    }
}

pub fn initial_request() -> Request {
    Request::ConversationList {
        tags: Vec::new(),
        text_query: None,
        limit: Some(200),
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &ConversationListState) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let list_area = chunks[0];
    let help_area = chunks[1];

    let block = Block::bordered().title(" conversations ");
    if state.loading && state.items.is_empty() {
        let msg = Paragraph::new("loading…")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(msg, list_area);
    } else if state.items.is_empty() {
        let msg = Paragraph::new("no conversations yet")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(msg, list_area);
    } else {
        let items: Vec<ListItem> = state
            .items
            .iter()
            .map(|item| {
                let title = item.title.as_deref().unwrap_or(&item.id);
                let tags = if item.tags.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", item.tags.join(", "))
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:<19} ", item.updated_at),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(title.to_string()),
                    Span::styled(tags, Style::default().fg(Color::Yellow)),
                ]))
            })
            .collect();
        let list = List::new(items).block(block).highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
        let mut ls = ListState::default();
        ls.select(Some(state.selected));
        frame.render_stateful_widget(list, list_area, &mut ls);
    }

    let help =
        Paragraph::new("enter: resume · t: tags · d/del/backspace: delete · s: search · esc: back")
            .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, help_area);
}

/// Key handler. Returns a [`Transition`]; initial loads for the next screen
/// are dispatched by the event loop via [`Screen::on_enter`].
pub fn handle_key(state: &mut ConversationListState, key: KeyEvent) -> Transition {
    if key.kind == KeyEventKind::Release {
        return Transition::None;
    }
    match key.code {
        KeyCode::Esc => Transition::To(Screen::Chat(ChatState::new())),
        KeyCode::Up => {
            if state.selected > 0 {
                state.selected -= 1;
            }
            Transition::None
        }
        KeyCode::Down => {
            if state.selected + 1 < state.items.len() {
                state.selected += 1;
            }
            Transition::None
        }
        KeyCode::Enter => {
            let Some(item) = state.items.get(state.selected) else {
                return Transition::None;
            };
            Transition::To(Screen::Chat(ChatState::loading_resume(item.id.clone())))
        }
        KeyCode::Char('t') => {
            let Some(item) = state.items.get(state.selected) else {
                return Transition::None;
            };
            let label = item.title.clone().unwrap_or_else(|| item.id.clone());
            Transition::To(Screen::TagEditor(super::TagEditorState::loading(
                item.id.clone(),
                label,
                super::tag_editor::ReturnScreen::ConversationList,
            )))
        }
        KeyCode::Char('s') => Transition::To(Screen::Search(super::SearchState::new())),
        KeyCode::Char('d') | KeyCode::Backspace | KeyCode::Delete => {
            let Some(item) = state.items.get(state.selected) else {
                return Transition::None;
            };
            let label = item.title.clone().unwrap_or_else(|| item.id.clone());
            Transition::To(Screen::ConfirmDelete(super::ConfirmDeleteState::new(
                item.id.clone(),
                label,
                super::confirm::ReturnTo::ConversationList,
            )))
        }
        _ => Transition::None,
    }
}

/// Apply a reply whose `seq` matches `state.request_seq`. Stale replies
/// (from a prior screen the user has navigated away from) are dropped by the
/// outer event loop before reaching us.
pub fn handle_reply(state: &mut ConversationListState, result: Response) {
    state.loading = false;
    match result {
        Response::ConversationList { items } => {
            state.items = items;
            if state.selected >= state.items.len() {
                state.selected = state.items.len().saturating_sub(1);
            }
        }
        Response::Error { message: _ } => {
            state.items.clear();
        }
        _ => {}
    }
}

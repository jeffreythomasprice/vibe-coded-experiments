//! Document picker: scrollable list; `n` to open the file picker, `t` for
//! tags, `d`/Backspace/Delete to delete (confirm dialog), `s` to search,
//! Esc back to chat.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

use crate::protocol::{DocumentSummary, Request, Response};

use super::chat::ChatState;
use super::targets::{DeleteTarget, TagTarget};
use super::{Screen, Transition};

pub struct DocumentListState {
    pub items: Vec<DocumentSummary>,
    pub selected: usize,
    pub loading: bool,
    pub request_seq: u64,
}

impl Default for DocumentListState {
    fn default() -> Self {
        Self::new_loading()
    }
}

impl DocumentListState {
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
    Request::DocumentList {
        tags: Vec::new(),
        limit: Some(200),
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &DocumentListState) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let list_area = chunks[0];
    let help_area = chunks[1];

    let block = Block::bordered().title(" documents ");
    if state.loading && state.items.is_empty() {
        let msg = Paragraph::new("loading…")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(msg, list_area);
    } else if state.items.is_empty() {
        let msg = Paragraph::new("no documents yet — press `n` to ingest one")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        frame.render_widget(msg, list_area);
    } else {
        let items: Vec<ListItem> = state
            .items
            .iter()
            .map(|item| {
                let tags = if item.tags.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", item.tags.join(", "))
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("{:<19} ", item.created_at),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(item.path.clone()),
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

    let help = Paragraph::new("n: new · t: tags · d/del/backspace: delete · s: search · esc: back")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, help_area);
}

pub fn handle_key(state: &mut DocumentListState, key: KeyEvent) -> Transition {
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
        KeyCode::Char('n') => Transition::To(Screen::FilePicker(Box::default())),
        KeyCode::Char('t') => {
            let Some(item) = state.items.get(state.selected) else {
                return Transition::None;
            };
            let label = item.path.clone();
            Transition::To(Screen::TagEditor(super::TagEditorState::loading(
                TagTarget::Document(item.id),
                label,
            )))
        }
        KeyCode::Char('s') => Transition::To(Screen::DocumentSearch(Box::default())),
        KeyCode::Char('d') | KeyCode::Backspace | KeyCode::Delete => {
            let Some(item) = state.items.get(state.selected) else {
                return Transition::None;
            };
            let label = item.path.clone();
            Transition::To(Screen::ConfirmDelete(super::ConfirmDeleteState::new(
                DeleteTarget::Document(item.id),
                label,
            )))
        }
        _ => Transition::None,
    }
}

pub fn handle_reply(state: &mut DocumentListState, result: Response) {
    state.loading = false;
    match result {
        Response::DocumentList { items } => {
            state.items = items;
            if state.selected >= state.items.len() {
                state.selected = state.items.len().saturating_sub(1);
            }
        }
        Response::Error { .. } => {
            state.items.clear();
        }
        _ => {}
    }
}

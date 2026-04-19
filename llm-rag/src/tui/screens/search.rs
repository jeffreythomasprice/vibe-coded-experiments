//! Search screen: substring query on message content + ALL-of tag filter.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

use crate::error::ClientError;
use crate::protocol::{ConversationSummary, Request, Response};

use super::chat::ChatState;
use super::tag_editor::ReturnScreen as TagReturn;
use super::{Screen, Transition};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Text,
    Tags,
    Results,
}

pub struct SearchState {
    pub text_input: String,
    pub text_cursor: usize,
    pub tag_selections: Vec<(String, bool)>,
    pub focus: Focus,
    pub results: Vec<ConversationSummary>,
    pub selected_result: usize,
    pub loading: bool,
    /// Seq of an outstanding `TagList` load (populates tag_selections).
    pub tags_seq: u64,
    /// Seq of an outstanding `ConversationList` search.
    pub results_seq: u64,
}

impl Default for SearchState {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            text_input: String::new(),
            text_cursor: 0,
            tag_selections: Vec::new(),
            focus: Focus::Text,
            results: Vec::new(),
            selected_result: 0,
            loading: false,
            tags_seq: 0,
            results_seq: 0,
        }
    }

    fn selected_tags(&self) -> Vec<String> {
        self.tag_selections
            .iter()
            .filter(|(_, on)| *on)
            .map(|(t, _)| t.clone())
            .collect()
    }

    fn search_request(&self) -> Request {
        let text = self.text_input.trim();
        Request::ConversationList {
            tags: self.selected_tags(),
            text_query: if text.is_empty() {
                None
            } else {
                Some(text.to_string())
            },
            limit: Some(200),
        }
    }
}

/// Initial load: fetch the global tag list so the user has something to
/// multi-select. The first search is triggered by Enter in the text box
/// (or by toggling a tag).
pub fn initial_requests() -> Vec<Request> {
    vec![Request::TagList]
}

pub fn render(frame: &mut Frame, area: Rect, state: &SearchState) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // text input
        Constraint::Length(6), // tag checklist
        Constraint::Min(1),    // results
        Constraint::Length(1), // help
    ])
    .split(area);

    let focused = |f: Focus| -> Style {
        if state.focus == f {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        }
    };

    let input = Paragraph::new(state.text_input.as_str())
        .block(Block::bordered().title(Span::styled(" search text ", focused(Focus::Text))));
    frame.render_widget(input, chunks[0]);

    let tag_items: Vec<ListItem> = state
        .tag_selections
        .iter()
        .map(|(name, on)| {
            let mark = if *on { "[x] " } else { "[ ] " };
            ListItem::new(Line::raw(format!("{mark}{name}")))
        })
        .collect();
    let tags_list = List::new(tag_items)
        .block(Block::bordered().title(Span::styled(
            " tags (space to toggle) ",
            focused(Focus::Tags),
        )))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
    let mut tl = ListState::default();
    if state.focus == Focus::Tags && !state.tag_selections.is_empty() {
        tl.select(Some(
            state.selected_result.min(state.tag_selections.len() - 1),
        ));
    }
    frame.render_stateful_widget(tags_list, chunks[1], &mut tl);

    let result_items: Vec<ListItem> = state
        .results
        .iter()
        .map(|item| {
            let title = item.title.as_deref().unwrap_or(&item.id);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:<19} ", item.updated_at),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(title.to_string()),
            ]))
        })
        .collect();
    let results_block = Block::bordered().title(Span::styled(" results ", focused(Focus::Results)));
    if state.loading && state.results.is_empty() {
        let msg = Paragraph::new("searching…")
            .style(Style::default().fg(Color::DarkGray))
            .block(results_block);
        frame.render_widget(msg, chunks[2]);
    } else {
        let list = List::new(result_items)
            .block(results_block)
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
        let mut ls = ListState::default();
        if state.focus == Focus::Results && !state.results.is_empty() {
            ls.select(Some(state.selected_result.min(state.results.len() - 1)));
        }
        frame.render_stateful_widget(list, chunks[2], &mut ls);
    }

    let help = Paragraph::new(
        "tab: cycle focus · enter: search / resume · t: tags · d/del/backspace: delete · esc: back",
    )
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[3]);
}

pub enum SearchAction {
    None,
    Transition(Transition),
    Search,
}

pub fn handle_key(state: &mut SearchState, key: KeyEvent) -> SearchAction {
    if key.kind == KeyEventKind::Release {
        return SearchAction::None;
    }
    match (state.focus, key.code) {
        (_, KeyCode::Esc) => SearchAction::Transition(Transition::To(Screen::ConversationList(
            super::ConversationListState::new_loading(),
        ))),
        (_, KeyCode::Tab) => {
            state.focus = match state.focus {
                Focus::Text => Focus::Tags,
                Focus::Tags => Focus::Results,
                Focus::Results => Focus::Text,
            };
            // Reset the list cursor across focus changes so we don't
            // accidentally index into the wrong list.
            state.selected_result = 0;
            SearchAction::None
        }

        (Focus::Text, KeyCode::Char(c)) => {
            state.text_input.insert(state.text_cursor, c);
            state.text_cursor += c.len_utf8();
            SearchAction::None
        }
        (Focus::Text, KeyCode::Backspace) => {
            if state.text_cursor > 0 {
                let prev = state.text_input[..state.text_cursor]
                    .chars()
                    .next_back()
                    .map(char::len_utf8)
                    .unwrap_or(0);
                if prev > 0 {
                    state
                        .text_input
                        .replace_range(state.text_cursor - prev..state.text_cursor, "");
                    state.text_cursor -= prev;
                }
            }
            SearchAction::None
        }
        (Focus::Text, KeyCode::Enter) => SearchAction::Search,

        (Focus::Tags, KeyCode::Up) => {
            if state.selected_result > 0 {
                state.selected_result -= 1;
            }
            SearchAction::None
        }
        (Focus::Tags, KeyCode::Down) => {
            if state.selected_result + 1 < state.tag_selections.len() {
                state.selected_result += 1;
            }
            SearchAction::None
        }
        (Focus::Tags, KeyCode::Char(' ')) => {
            if let Some((_, on)) = state.tag_selections.get_mut(state.selected_result) {
                *on = !*on;
                return SearchAction::Search;
            }
            SearchAction::None
        }

        (Focus::Results, KeyCode::Up) => {
            if state.selected_result > 0 {
                state.selected_result -= 1;
            }
            SearchAction::None
        }
        (Focus::Results, KeyCode::Down) => {
            if state.selected_result + 1 < state.results.len() {
                state.selected_result += 1;
            }
            SearchAction::None
        }
        (Focus::Results, KeyCode::Enter) => {
            let Some(item) = state.results.get(state.selected_result) else {
                return SearchAction::None;
            };
            SearchAction::Transition(Transition::To(Screen::Chat(ChatState::loading_resume(
                item.id.clone(),
            ))))
        }
        (Focus::Results, KeyCode::Char('t')) => {
            let Some(item) = state.results.get(state.selected_result) else {
                return SearchAction::None;
            };
            let label = item.title.clone().unwrap_or_else(|| item.id.clone());
            SearchAction::Transition(Transition::To(Screen::TagEditor(
                super::TagEditorState::loading(
                    item.id.clone(),
                    label,
                    // Return to search would require carrying the prefill
                    // forward; for now ConversationList is the return path.
                    TagReturn::ConversationList,
                ),
            )))
        }
        _ => SearchAction::None,
    }
}

pub fn search_request(state: &SearchState) -> Request {
    state.search_request()
}

pub fn handle_reply(state: &mut SearchState, seq: u64, result: Result<Response, ClientError>) {
    if seq == state.tags_seq && seq != 0 {
        state.tags_seq = 0;
        if let Ok(Response::TagList { tags }) = result {
            state.tag_selections = tags.into_iter().map(|t| (t, false)).collect();
        }
        return;
    }
    if seq == state.results_seq && seq != 0 {
        state.results_seq = 0;
        state.loading = false;
        match result {
            Ok(Response::ConversationList { items }) => {
                state.results = items;
                if state.selected_result >= state.results.len() {
                    state.selected_result = state.results.len().saturating_sub(1);
                }
            }
            _ => state.results.clear(),
        }
    }
}

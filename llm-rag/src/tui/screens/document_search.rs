//! Vector search over stored chunks. Mirrors the three-pane layout of
//! `search.rs` (text input + ALL-of tag checklist + result list).

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

use crate::error::ClientError;
use crate::protocol::{DocumentSearchHit, Request, Response};
use crate::tui::app::App;
use crate::tui::input;
use crate::tui::spinner;

use super::{Screen, Transition};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Text,
    Tags,
    Results,
}

pub struct DocumentSearchState {
    pub text_input: String,
    pub text_cursor: usize,
    pub tag_selections: Vec<(String, bool)>,
    pub focus: Focus,
    pub results: Vec<DocumentSearchHit>,
    pub selected: usize,
    pub loading: bool,
    pub tags_seq: u64,
    pub results_seq: u64,
}

impl Default for DocumentSearchState {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentSearchState {
    pub fn new() -> Self {
        Self {
            text_input: String::new(),
            text_cursor: 0,
            tag_selections: Vec::new(),
            focus: Focus::Text,
            results: Vec::new(),
            selected: 0,
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
}

pub fn initial_requests() -> Vec<Request> {
    vec![Request::TagList]
}

pub fn search_request(state: &DocumentSearchState) -> Request {
    let text = state.text_input.trim();
    Request::DocumentSearch {
        query: text.to_string(),
        tags: state.selected_tags(),
        limit: Some(20),
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App, state: &DocumentSearchState) {
    let (banner_area, input_area, tags_area, results_area, help_area) = if app.pending > 0 {
        let c = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
        (Some(c[0]), c[1], c[2], c[3], c[4])
    } else {
        let c = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
        (None, c[0], c[1], c[2], c[3])
    };

    if let Some(b) = banner_area {
        spinner::render_banner(frame, b, app.spinner_frame);
    }

    let focused = |f: Focus| -> Style {
        if state.focus == f {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        }
    };

    let input_title = if app.pending > 0 {
        format!(" query {} ", spinner::frame(app.spinner_frame))
    } else {
        " query ".to_string()
    };
    let input = Paragraph::new(state.text_input.as_str())
        .block(Block::bordered().title(Span::styled(input_title, focused(Focus::Text))));
    frame.render_widget(input, input_area);

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
        tl.select(Some(state.selected.min(state.tag_selections.len() - 1)));
    }
    frame.render_stateful_widget(tags_list, tags_area, &mut tl);

    let result_items: Vec<ListItem> = state
        .results
        .iter()
        .map(|hit| {
            let snippet: String = hit.content.chars().take(80).collect();
            let snippet = snippet.replace('\n', " ");
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{:>6.4}  ", hit.distance),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(hit.path.clone()),
                ]),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(snippet, Style::default().fg(Color::Gray)),
                ]),
            ])
        })
        .collect();
    let results_block = Block::bordered().title(Span::styled(" results ", focused(Focus::Results)));
    if state.loading && state.results.is_empty() {
        let msg = Paragraph::new(format!("{} searching…", spinner::frame(app.spinner_frame)))
            .style(Style::default().fg(Color::DarkGray))
            .block(results_block);
        frame.render_widget(msg, results_area);
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
            ls.select(Some(state.selected.min(state.results.len() - 1)));
        }
        frame.render_stateful_widget(list, results_area, &mut ls);
    }

    let help = Paragraph::new("tab: cycle focus · enter: search · space: toggle tag · esc: back")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, help_area);
}

#[allow(clippy::large_enum_variant)]
pub enum SearchAction {
    None,
    Transition(Transition),
    Search,
}

pub fn handle_key(state: &mut DocumentSearchState, key: KeyEvent, waiting: bool) -> SearchAction {
    if key.kind == KeyEventKind::Release {
        return SearchAction::None;
    }
    if waiting {
        match state.focus {
            Focus::Text if input::is_buffer_edit(&key) || key.code == KeyCode::Enter => {
                return SearchAction::None;
            }
            Focus::Tags if matches!(key.code, KeyCode::Char(' ')) => return SearchAction::None,
            _ => {}
        }
    }
    match (state.focus, key.code) {
        (_, KeyCode::Esc) => {
            SearchAction::Transition(Transition::To(Screen::DocumentList(Box::default())))
        }
        (_, KeyCode::Tab) => {
            state.focus = match state.focus {
                Focus::Text => Focus::Tags,
                Focus::Tags => Focus::Results,
                Focus::Results => Focus::Text,
            };
            state.selected = 0;
            SearchAction::None
        }

        (Focus::Text, KeyCode::Char(c)) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'w' => {
                        input::delete_word_backward(&mut state.text_input, &mut state.text_cursor);
                    }
                    'u' => {
                        input::delete_to_start(&mut state.text_input, &mut state.text_cursor);
                    }
                    'k' => {
                        input::delete_to_end(&mut state.text_input, &mut state.text_cursor);
                    }
                    'a' => input::cursor_home(&mut state.text_cursor),
                    'e' => input::cursor_end(&state.text_input, &mut state.text_cursor),
                    _ => {}
                }
            } else {
                input::insert_char(&mut state.text_input, &mut state.text_cursor, c);
            }
            SearchAction::None
        }
        (Focus::Text, KeyCode::Backspace) => {
            input::delete_char_backward(&mut state.text_input, &mut state.text_cursor);
            SearchAction::None
        }
        (Focus::Text, KeyCode::Delete) => {
            input::delete_char_forward(&mut state.text_input, &mut state.text_cursor);
            SearchAction::None
        }
        (Focus::Text, KeyCode::Left) => {
            input::cursor_left(&state.text_input, &mut state.text_cursor);
            SearchAction::None
        }
        (Focus::Text, KeyCode::Right) => {
            input::cursor_right(&state.text_input, &mut state.text_cursor);
            SearchAction::None
        }
        (Focus::Text, KeyCode::Home) => {
            input::cursor_home(&mut state.text_cursor);
            SearchAction::None
        }
        (Focus::Text, KeyCode::End) => {
            input::cursor_end(&state.text_input, &mut state.text_cursor);
            SearchAction::None
        }
        (Focus::Text, KeyCode::Enter) => {
            if state.text_input.trim().is_empty() {
                SearchAction::None
            } else {
                SearchAction::Search
            }
        }

        (Focus::Tags, KeyCode::Up) => {
            if state.selected > 0 {
                state.selected -= 1;
            }
            SearchAction::None
        }
        (Focus::Tags, KeyCode::Down) => {
            if state.selected + 1 < state.tag_selections.len() {
                state.selected += 1;
            }
            SearchAction::None
        }
        (Focus::Tags, KeyCode::Char(' ')) => {
            if let Some((_, on)) = state.tag_selections.get_mut(state.selected) {
                *on = !*on;
                if !state.text_input.trim().is_empty() {
                    return SearchAction::Search;
                }
            }
            SearchAction::None
        }

        (Focus::Results, KeyCode::Up) => {
            if state.selected > 0 {
                state.selected -= 1;
            }
            SearchAction::None
        }
        (Focus::Results, KeyCode::Down) => {
            if state.selected + 1 < state.results.len() {
                state.selected += 1;
            }
            SearchAction::None
        }
        _ => SearchAction::None,
    }
}

pub fn handle_reply(
    state: &mut DocumentSearchState,
    seq: u64,
    result: Result<Response, ClientError>,
) {
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
            Ok(Response::DocumentSearch { results }) => {
                state.results = results;
                if state.selected >= state.results.len() {
                    state.selected = state.results.len().saturating_sub(1);
                }
            }
            _ => state.results.clear(),
        }
    }
}

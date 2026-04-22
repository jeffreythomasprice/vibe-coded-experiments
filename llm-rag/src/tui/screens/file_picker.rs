//! Minimal file browser for document ingest.
//!
//! Runs in the TUI process (client side) and reads directories synchronously
//! via `std::fs::read_dir`. Once the user confirms a file and (optionally)
//! types some tags, the picker emits a `Request::DocumentCreate` carrying the
//! absolute path — the server is what actually opens the file.
//!
//! Two modes:
//! - [`PickerMode::Browsing`] — navigate with Up/Down, Enter descends into a
//!   dir or moves to the Confirm mode on a file, Backspace/Left goes up.
//! - [`PickerMode::Confirming`] — tag-editor-style input. Ctrl-Enter (or
//!   Enter on empty input) submits. Esc goes back to browsing.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

use crate::error::ClientError;
use crate::protocol::{Request, Response};
use crate::tui::app::App;
use crate::tui::input;
use crate::tui::spinner;

use super::{Screen, Transition};

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub enum PickerMode {
    Browsing,
    Confirming {
        file: PathBuf,
        tag_input: String,
        cursor: usize,
        staged_tags: Vec<String>,
    },
    Ingesting {
        file: PathBuf,
        total: Option<usize>,
        completed: usize,
        in_flight: usize,
        request_seq: u64,
    },
    Done {
        document_id: i64,
        chunks: usize,
    },
    Failed {
        message: String,
    },
}

pub struct FilePickerState {
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub mode: PickerMode,
    /// Cached global tag list for hinting (not autocompleted for simplicity).
    pub all_tags: Vec<String>,
    pub all_tags_seq: u64,
}

impl Default for FilePickerState {
    fn default() -> Self {
        Self::new()
    }
}

impl FilePickerState {
    pub fn new() -> Self {
        let cwd = start_dir();
        let entries = read_dir(&cwd);
        Self {
            cwd,
            entries,
            selected: 0,
            mode: PickerMode::Browsing,
            all_tags: Vec::new(),
            all_tags_seq: 0,
        }
    }
}

pub fn initial_requests() -> Vec<Request> {
    vec![Request::TagList]
}

fn start_dir() -> PathBuf {
    if let Some(dirs) = directories::UserDirs::new()
        && let Some(home) = dirs.home_dir().to_path_buf().into()
    {
        return home;
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
}

fn read_dir(cwd: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    if let Some(parent) = cwd.parent() {
        out.push(Entry {
            name: "..".to_string(),
            path: parent.to_path_buf(),
            is_dir: true,
        });
    }
    let Ok(iter) = std::fs::read_dir(cwd) else {
        return out;
    };
    let mut items: Vec<Entry> = iter
        .filter_map(|res| res.ok())
        .filter_map(|d| {
            let name = d.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = d.path();
            let is_dir = d.file_type().map(|t| t.is_dir()).unwrap_or(false);
            Some(Entry { name, path, is_dir })
        })
        .collect();
    items.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    out.extend(items);
    out
}

pub fn render(frame: &mut Frame, area: Rect, app: &App, state: &FilePickerState) {
    match &state.mode {
        PickerMode::Browsing => render_browsing(frame, area, state),
        PickerMode::Confirming {
            file,
            tag_input,
            staged_tags,
            ..
        } => render_confirming(frame, area, state, file, tag_input, staged_tags),
        PickerMode::Ingesting {
            file,
            total,
            completed,
            in_flight,
            ..
        } => render_ingesting(frame, area, app, file, *total, *completed, *in_flight),
        PickerMode::Done {
            document_id,
            chunks,
        } => render_done(frame, area, *document_id, *chunks),
        PickerMode::Failed { message } => render_failed(frame, area, message),
    }
}

fn render_browsing(frame: &mut Frame, area: Rect, state: &FilePickerState) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    let cwd_display = state.cwd.display().to_string();
    let header =
        Paragraph::new(cwd_display).block(Block::bordered().title(" pick a file to ingest "));
    frame.render_widget(header, chunks[0]);

    let items: Vec<ListItem> = state
        .entries
        .iter()
        .map(|e| {
            let marker = if e.is_dir { "▸ " } else { "  " };
            let style = if e.is_dir {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::styled(e.name.clone(), style),
            ]))
        })
        .collect();
    let list = List::new(items).block(Block::bordered()).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut ls = ListState::default();
    if !state.entries.is_empty() {
        ls.select(Some(state.selected.min(state.entries.len() - 1)));
    }
    frame.render_stateful_widget(list, chunks[1], &mut ls);

    let help = Paragraph::new("↑↓: move · enter: descend / pick · backspace: up · esc: cancel")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[2]);
}

fn render_confirming(
    frame: &mut Frame,
    area: Rect,
    _state: &FilePickerState,
    file: &Path,
    tag_input: &str,
    staged_tags: &[String],
) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    let header = Paragraph::new(file.display().to_string())
        .block(Block::bordered().title(" ingest this file? "));
    frame.render_widget(header, chunks[0]);

    let staged_line = if staged_tags.is_empty() {
        "(none)".to_string()
    } else {
        staged_tags.join(", ")
    };
    let staged = Paragraph::new(staged_line).block(Block::bordered().title(" tags "));
    frame.render_widget(staged, chunks[1]);

    let input =
        Paragraph::new(tag_input).block(Block::bordered().title(" add tag (enter to stage) "));
    frame.render_widget(input, chunks[2]);

    let blank = Paragraph::new("");
    frame.render_widget(blank, chunks[3]);

    let help = Paragraph::new(
        "enter (in tag box): stage · ctrl-s / enter (empty): ingest · esc: back to browse",
    )
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[4]);
}

fn render_ingesting(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    file: &Path,
    total: Option<usize>,
    completed: usize,
    in_flight: usize,
) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    let title = format!(" ingesting {} ", spinner::frame(app.spinner_frame));
    let header = Paragraph::new(file.display().to_string()).block(Block::bordered().title(title));
    frame.render_widget(header, chunks[0]);

    let body = match total {
        Some(total) => format!("{completed}/{total} complete · {in_flight} in flight"),
        None => "preparing…".to_string(),
    };
    let msg = Paragraph::new(body).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(msg, chunks[1]);
    let help = Paragraph::new("please wait — esc after completion returns to document list")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[2]);
}

fn render_done(frame: &mut Frame, area: Rect, document_id: i64, chunks_count: usize) {
    let vchunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let msg = Paragraph::new(format!(
        "ingested successfully — document id={document_id}, chunks={chunks_count}",
    ))
    .style(Style::default().fg(Color::Green))
    .block(Block::bordered().title(" done "));
    frame.render_widget(msg, vchunks[0]);
    let help = Paragraph::new("enter / esc: back to documents")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, vchunks[1]);
}

fn render_failed(frame: &mut Frame, area: Rect, message: &str) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let msg = Paragraph::new(message.to_string())
        .style(Style::default().fg(Color::Red))
        .block(Block::bordered().title(" ingest failed "));
    frame.render_widget(msg, chunks[0]);
    let help = Paragraph::new("enter / esc: back to documents")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[1]);
}

pub enum PickerAction {
    None,
    Transition(Transition),
    Submit { path: String, tags: Vec<String> },
}

pub fn handle_key(state: &mut FilePickerState, key: KeyEvent) -> PickerAction {
    if key.kind == KeyEventKind::Release {
        return PickerAction::None;
    }
    match &mut state.mode {
        PickerMode::Browsing => handle_browsing_key(state, key),
        PickerMode::Confirming { .. } => handle_confirming_key(state, key),
        PickerMode::Ingesting { .. } => PickerAction::None,
        PickerMode::Done { .. } | PickerMode::Failed { .. } => match key.code {
            KeyCode::Enter | KeyCode::Esc => {
                PickerAction::Transition(Transition::To(Screen::DocumentList(Box::default())))
            }
            _ => PickerAction::None,
        },
    }
}

fn handle_browsing_key(state: &mut FilePickerState, key: KeyEvent) -> PickerAction {
    match key.code {
        KeyCode::Esc => {
            PickerAction::Transition(Transition::To(Screen::DocumentList(Box::default())))
        }
        KeyCode::Up => {
            if state.selected > 0 {
                state.selected -= 1;
            }
            PickerAction::None
        }
        KeyCode::Down => {
            if state.selected + 1 < state.entries.len() {
                state.selected += 1;
            }
            PickerAction::None
        }
        KeyCode::Left | KeyCode::Backspace => {
            if let Some(parent) = state.cwd.parent() {
                state.cwd = parent.to_path_buf();
                state.entries = read_dir(&state.cwd);
                state.selected = 0;
            }
            PickerAction::None
        }
        KeyCode::Enter | KeyCode::Right => {
            let Some(entry) = state.entries.get(state.selected).cloned() else {
                return PickerAction::None;
            };
            if entry.is_dir {
                state.cwd = entry.path;
                state.entries = read_dir(&state.cwd);
                state.selected = 0;
            } else {
                state.mode = PickerMode::Confirming {
                    file: entry.path,
                    tag_input: String::new(),
                    cursor: 0,
                    staged_tags: Vec::new(),
                };
            }
            PickerAction::None
        }
        _ => PickerAction::None,
    }
}

fn handle_confirming_key(state: &mut FilePickerState, key: KeyEvent) -> PickerAction {
    let PickerMode::Confirming {
        file,
        tag_input,
        cursor,
        staged_tags,
    } = &mut state.mode
    else {
        return PickerAction::None;
    };

    match key.code {
        KeyCode::Esc => {
            state.mode = PickerMode::Browsing;
            PickerAction::None
        }
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => submit(state),
        KeyCode::Enter if tag_input.trim().is_empty() => submit(state),
        KeyCode::Enter => {
            let tag = tag_input.trim().to_string();
            if !tag.is_empty() && !staged_tags.contains(&tag) {
                staged_tags.push(tag);
                staged_tags.sort();
            }
            tag_input.clear();
            *cursor = 0;
            PickerAction::None
        }
        KeyCode::Backspace => {
            if tag_input.is_empty() {
                staged_tags.pop();
            } else {
                input::delete_char_backward(tag_input, cursor);
            }
            PickerAction::None
        }
        KeyCode::Delete => {
            input::delete_char_forward(tag_input, cursor);
            PickerAction::None
        }
        KeyCode::Left => {
            input::cursor_left(tag_input, cursor);
            PickerAction::None
        }
        KeyCode::Right => {
            input::cursor_right(tag_input, cursor);
            PickerAction::None
        }
        KeyCode::Home => {
            input::cursor_home(cursor);
            PickerAction::None
        }
        KeyCode::End => {
            input::cursor_end(tag_input, cursor);
            PickerAction::None
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'w' => {
                        input::delete_word_backward(tag_input, cursor);
                    }
                    'u' => {
                        input::delete_to_start(tag_input, cursor);
                    }
                    'k' => {
                        input::delete_to_end(tag_input, cursor);
                    }
                    'a' => input::cursor_home(cursor),
                    'e' => input::cursor_end(tag_input, cursor),
                    _ => {}
                }
            } else {
                input::insert_char(tag_input, cursor, c);
            }
            // Silence unused-var warning for `file` path.
            let _ = file;
            PickerAction::None
        }
        _ => PickerAction::None,
    }
}

fn submit(state: &mut FilePickerState) -> PickerAction {
    let PickerMode::Confirming {
        file,
        staged_tags,
        tag_input,
        ..
    } = &state.mode
    else {
        return PickerAction::None;
    };
    let mut tags = staged_tags.clone();
    // Any un-staged text in the input is treated as a final tag.
    let tail = tag_input.trim();
    if !tail.is_empty() && !tags.contains(&tail.to_string()) {
        tags.push(tail.to_string());
    }
    let path = match file.canonicalize() {
        Ok(p) => p,
        Err(_) => file.clone(),
    };
    let action = PickerAction::Submit {
        path: path.to_string_lossy().into_owned(),
        tags,
    };
    state.mode = PickerMode::Ingesting {
        file: path,
        total: None,
        completed: 0,
        in_flight: 0,
        request_seq: 0,
    };
    action
}

pub fn set_request_seq(state: &mut FilePickerState, seq: u64) {
    if let PickerMode::Ingesting { request_seq, .. } = &mut state.mode {
        *request_seq = seq;
    }
}

/// Route a streaming frame (or a terminal error) into the picker's state
/// machine. Returns `true` if the frame was consumed by this picker.
pub fn handle_reply(
    state: &mut FilePickerState,
    seq: u64,
    result: &Result<Response, ClientError>,
) -> bool {
    // Tag list populates the hint cache.
    if seq == state.all_tags_seq && seq != 0 {
        state.all_tags_seq = 0;
        if let Ok(Response::TagList { tags }) = result {
            state.all_tags = tags.clone();
        }
        return true;
    }

    let PickerMode::Ingesting { request_seq, .. } = &state.mode else {
        return false;
    };
    if *request_seq != seq {
        return false;
    }

    match result {
        Ok(Response::DocumentCreateStart { total_chunks, .. }) => {
            if let PickerMode::Ingesting {
                total,
                completed,
                in_flight,
                ..
            } = &mut state.mode
            {
                *total = Some(*total_chunks);
                *completed = 0;
                *in_flight = 0;
            }
            true
        }
        Ok(Response::DocumentCreateProgress {
            completed: c,
            in_flight: f,
            total: t,
            ..
        }) => {
            if let PickerMode::Ingesting {
                total,
                completed,
                in_flight,
                ..
            } = &mut state.mode
            {
                *total = Some(*t);
                *completed = *c;
                *in_flight = *f;
            }
            true
        }
        Ok(Response::DocumentCreateDone {
            document_id,
            chunks,
        }) => {
            state.mode = PickerMode::Done {
                document_id: *document_id,
                chunks: *chunks,
            };
            true
        }
        Ok(Response::Error { message }) => {
            state.mode = PickerMode::Failed {
                message: message.clone(),
            };
            true
        }
        Err(err) => {
            state.mode = PickerMode::Failed {
                message: err.to_string(),
            };
            true
        }
        _ => false,
    }
}

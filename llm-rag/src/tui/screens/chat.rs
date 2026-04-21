//! The default chat screen — transcript on top, input box below. Handles
//! `/slash` commands and normal chat turns, dispatching to the server over
//! the existing Unix-socket RPC.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::db::MessageRole;
use crate::protocol::{Request, Response, WireMessage};
use crate::tui::app::{App, Autocomplete};
use crate::tui::commands;
use crate::tui::format;
use crate::tui::input;
use crate::tui::spinner;

use super::Transition;

/// Per-screen state for the chat view.
pub struct ChatState {
    pub transcript: Vec<Line<'static>>,
    pub scroll_offset: u16,
    pub input: String,
    pub cursor: usize,
    pub autocomplete: Option<Autocomplete>,
    /// Set by the server on the first Chat reply and echoed back on each
    /// subsequent turn so the server can thread history.
    pub conversation_id: Option<String>,
    /// Seq of the outstanding `ConversationGet` request fired when resuming;
    /// once the reply lands and populates the transcript, this stays 0.
    pub request_seq: u64,
    /// When entering via `/resume`, [`Screen::on_enter`] fires this request
    /// so the transcript gets populated with history.
    pub pending_resume_request: Option<Request>,
    /// Populated while a streaming chat response is in flight. Cleared on
    /// `ChatDone` (or error). `transcript_mark` is the transcript length at
    /// `ChatStart` — on completion we truncate back to it and replay the
    /// authoritative rows from `ChatDone.messages_appended`, which ensures
    /// parity with the rendering path used for `/resume`'d history. Boxed so
    /// the idle (non-streaming) case keeps `ChatState` — and by extension
    /// `Screen` / `Transition` — small enough to stay under clippy's
    /// `large_enum_variant` threshold.
    pub streaming: Option<Box<StreamingTurn>>,
}

pub struct StreamingTurn {
    pub transcript_mark: usize,
    pub events: Vec<StreamEvent>,
}

/// One ordered piece of the in-flight streaming turn. The stream can include
/// multiple LLM passes with tool calls in between, so the renderer has to
/// walk these in first-seen order rather than grouping by kind.
pub enum StreamEvent {
    Text(String),
    ToolCall(StreamingToolCall),
    ToolResult { id: String, content: String },
}

pub struct StreamingToolCall {
    pub id: String,
    pub name: Option<String>,
    pub args: String,
}

impl Default for ChatState {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatState {
    pub fn new() -> Self {
        let mut state = Self::blank();
        state.push_system("welcome to llm-rag — type a message, or try /ping or /quit");
        state
    }

    fn blank() -> Self {
        Self {
            transcript: Vec::new(),
            scroll_offset: 0,
            input: String::new(),
            cursor: 0,
            autocomplete: None,
            conversation_id: None,
            request_seq: 0,
            pending_resume_request: None,
            streaming: None,
        }
    }

    /// Construct a Chat screen that is resuming an existing conversation. On
    /// entry the event loop fires the `ConversationGet` carried in
    /// `pending_resume_request`.
    pub fn loading_resume(conversation_id: String) -> Self {
        let mut state = Self::blank();
        state.conversation_id = Some(conversation_id.clone());
        state.pending_resume_request = Some(Request::ConversationGet {
            id: conversation_id,
        });
        state.push_system("loading conversation…");
        state
    }

    pub fn push_system(&mut self, text: impl Into<String>) {
        self.transcript.push(Line::from(Span::styled(
            text.into(),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    pub fn push_user(&mut self, text: impl Into<String>) {
        self.transcript.push(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::raw(text.into()),
        ]));
    }

    pub fn push_server(&mut self, text: impl Into<String>) {
        self.transcript.push(Line::from(vec![
            Span::styled("• ", Style::default().fg(Color::Green)),
            Span::raw(text.into()),
        ]));
    }

    pub fn push_tool_use(&mut self, name: &str, args: &serde_json::Value) {
        self.transcript
            .push(format::tool_call_line(name, &args.to_string()));
    }

    pub fn push_tool_result(&mut self, content: &str) {
        self.transcript.push(format::tool_result_line(content));
    }

    pub fn push_error(&mut self, text: impl Into<String>) {
        self.transcript.push(Line::from(Span::styled(
            text.into(),
            Style::default().fg(Color::Red),
        )));
    }

    pub fn display_response(&mut self, resp: Response) {
        match resp {
            Response::Pong => self.push_server("Pong"),
            Response::ChatStart { conversation_id } => {
                self.conversation_id = Some(conversation_id);
                self.streaming = Some(Box::new(StreamingTurn {
                    transcript_mark: self.transcript.len(),
                    events: Vec::new(),
                }));
            }
            Response::ChatDelta { text, .. } => {
                if let Some(stream) = self.streaming.as_mut() {
                    // Append to the last Text event if one is already the
                    // latest — otherwise start a new one. This keeps turns
                    // that come after a tool result visually distinct.
                    if let Some(StreamEvent::Text(buf)) = stream.events.last_mut() {
                        buf.push_str(&text);
                    } else {
                        stream.events.push(StreamEvent::Text(text));
                    }
                    rerender_streaming(&mut self.transcript, stream);
                }
            }
            Response::ChatToolCallDelta {
                id,
                name,
                arguments_delta,
                ..
            } => {
                if let Some(stream) = self.streaming.as_mut() {
                    let existing = stream.events.iter_mut().find_map(|ev| match ev {
                        StreamEvent::ToolCall(tc) if tc.id == id => Some(tc),
                        _ => None,
                    });
                    match existing {
                        Some(tc) => {
                            if tc.name.is_none() {
                                tc.name = name;
                            }
                            tc.args.push_str(&arguments_delta);
                        }
                        None => stream.events.push(StreamEvent::ToolCall(StreamingToolCall {
                            id,
                            name,
                            args: arguments_delta,
                        })),
                    }
                    rerender_streaming(&mut self.transcript, stream);
                }
            }
            Response::ChatToolResult { id, content, .. } => {
                if let Some(stream) = self.streaming.as_mut() {
                    stream.events.push(StreamEvent::ToolResult { id, content });
                    rerender_streaming(&mut self.transcript, stream);
                }
            }
            Response::ChatDone {
                conversation_id,
                messages_appended,
            } => {
                self.conversation_id = Some(conversation_id);
                // Truncate the live placeholders and replay the authoritative
                // rows so role styling and metadata match the `/resume` path.
                if let Some(stream) = self.streaming.take() {
                    self.transcript.truncate(stream.transcript_mark);
                }
                for msg in messages_appended {
                    self.append_history(msg);
                }
            }
            Response::ConversationGet {
                conversation,
                messages,
            } => {
                self.conversation_id = Some(conversation.id.clone());
                // Replace the "loading…" placeholder with the real history.
                self.transcript.clear();
                let header = match conversation.title {
                    Some(t) if !t.is_empty() => format!("resumed: {t}"),
                    _ => format!("resumed: {}", conversation.id),
                };
                self.push_system(header);
                for msg in messages {
                    self.append_history(msg);
                }
            }
            Response::Error { message } => {
                // If the error arrived mid-stream, leave the partial render in
                // place and append the error line — preserves context.
                self.streaming = None;
                self.push_error(format!("server error: {message}"));
            }
            other => self.push_error(format!("unexpected response: {other:?}")),
        }
    }

    /// Render one persisted row with role-appropriate styling.
    fn append_history(&mut self, msg: WireMessage) {
        match msg.role {
            MessageRole::System => self.push_system(msg.content),
            MessageRole::User => self.push_user(msg.content),
            MessageRole::Assistant => self.push_server(msg.content),
            MessageRole::ToolUse => {
                if let Some(crate::db::MessageMetadata::ToolUse {
                    name, arguments, ..
                }) = msg.metadata.as_ref()
                {
                    self.push_tool_use(name, arguments);
                } else {
                    self.push_tool_use("?", &serde_json::Value::Null);
                }
            }
            MessageRole::Tool => self.push_tool_result(&msg.content),
        }
    }

    fn input_insert(&mut self, ch: char) {
        if input::insert_char(&mut self.input, &mut self.cursor, ch) {
            self.refresh_autocomplete();
        }
    }

    fn input_backspace(&mut self) {
        if input::delete_char_backward(&mut self.input, &mut self.cursor) {
            self.refresh_autocomplete();
        }
    }

    fn input_delete_forward(&mut self) {
        if input::delete_char_forward(&mut self.input, &mut self.cursor) {
            self.refresh_autocomplete();
        }
    }

    fn input_delete_word_backward(&mut self) {
        if input::delete_word_backward(&mut self.input, &mut self.cursor) {
            self.refresh_autocomplete();
        }
    }

    fn input_delete_to_start(&mut self) {
        if input::delete_to_start(&mut self.input, &mut self.cursor) {
            self.refresh_autocomplete();
        }
    }

    fn input_delete_to_end(&mut self) {
        if input::delete_to_end(&mut self.input, &mut self.cursor) {
            self.refresh_autocomplete();
        }
    }

    fn cursor_left(&mut self) {
        input::cursor_left(&self.input, &mut self.cursor);
    }

    fn cursor_right(&mut self) {
        input::cursor_right(&self.input, &mut self.cursor);
    }

    fn cursor_home(&mut self) {
        input::cursor_home(&mut self.cursor);
    }

    fn cursor_end(&mut self) {
        input::cursor_end(&self.input, &mut self.cursor);
    }

    fn take_input(&mut self) -> String {
        let taken = std::mem::take(&mut self.input);
        self.cursor = 0;
        self.autocomplete = None;
        taken
    }

    fn cycle_autocomplete(&mut self, forward: bool) {
        let Some(ac) = self.autocomplete.as_mut() else {
            return;
        };
        let n = ac.matches.len();
        if n == 0 {
            return;
        }
        if forward {
            ac.selected = (ac.selected + 1) % n;
        } else {
            ac.selected = (ac.selected + n - 1) % n;
        }
    }

    fn refresh_autocomplete(&mut self) {
        let buf = self.input.as_str();
        if !buf.starts_with('/') || buf.contains(char::is_whitespace) {
            self.autocomplete = None;
            return;
        }
        let prefix = &buf[1..];
        let matches = commands::filter(prefix);
        if matches.is_empty() {
            self.autocomplete = None;
            return;
        }
        let selected = self
            .autocomplete
            .as_ref()
            .map(|a| a.selected.min(matches.len() - 1))
            .unwrap_or(0);
        self.autocomplete = Some(Autocomplete { matches, selected });
    }
}

/// Rebuild the in-flight portion of the transcript from `stream`'s ordered
/// events. Truncates back to the stream's `transcript_mark` and re-pushes one
/// line per event, in first-seen order. Tool-call and tool-result lines route
/// through `tui::format` so they look identical to the post-`ChatDone` replay.
fn rerender_streaming(transcript: &mut Vec<Line<'static>>, stream: &StreamingTurn) {
    transcript.truncate(stream.transcript_mark);
    for event in &stream.events {
        match event {
            StreamEvent::Text(text) => {
                if !text.is_empty() {
                    transcript.push(Line::from(vec![
                        Span::styled("• ", Style::default().fg(Color::Green)),
                        Span::raw(text.clone()),
                    ]));
                }
            }
            StreamEvent::ToolCall(tc) => {
                let name = tc.name.as_deref().unwrap_or("?");
                transcript.push(format::tool_call_line(name, &tc.args));
            }
            StreamEvent::ToolResult { content, .. } => {
                transcript.push(format::tool_result_line(content));
            }
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App, state: &ChatState) {
    let (transcript_area, banner_area, input_area) = if app.pending > 0 {
        let c = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(area);
        (c[0], Some(c[1]), c[2])
    } else {
        let c = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(area);
        (c[0], None, c[1])
    };

    render_transcript(frame, transcript_area, state);
    if let Some(b) = banner_area {
        spinner::render_banner(frame, b, app.spinner_frame);
    }
    render_input(frame, input_area, app, state);
    render_autocomplete(frame, input_area, state);
}

fn render_transcript(frame: &mut Frame, area: Rect, state: &ChatState) {
    let block = Block::bordered().title(" llm-rag ");
    let paragraph = Paragraph::new(state.transcript.clone())
        .wrap(Wrap { trim: false })
        .scroll((state.scroll_offset, 0))
        .block(block);
    frame.render_widget(paragraph, area);
}

fn render_input(frame: &mut Frame, area: Rect, app: &App, state: &ChatState) {
    let block = Block::bordered().title(" > ");
    let inner = block.inner(area);
    let paragraph = Paragraph::new(state.input.as_str()).block(block);
    frame.render_widget(paragraph, area);

    if app.pending > 0 {
        return;
    }
    let col = state.input[..state.cursor].chars().count() as u16;
    let x = inner.x + col;
    let y = inner.y;
    if x < inner.x.saturating_add(inner.width) && inner.height > 0 {
        frame.set_cursor_position(Position { x, y });
    }
}

fn render_autocomplete(frame: &mut Frame, input_area: Rect, state: &ChatState) {
    let Some(ac) = state.autocomplete.as_ref() else {
        return;
    };
    let visible = ac.matches.len().min(8) as u16;
    let popup_height = visible.saturating_add(2);
    if input_area.y < popup_height {
        return;
    }
    let desired_width = ac
        .matches
        .iter()
        .map(|c| c.name.len() + c.description.len() + 6)
        .max()
        .unwrap_or(20) as u16;
    let width = desired_width.min(input_area.width).max(10);
    let popup = Rect {
        x: input_area.x,
        y: input_area.y - popup_height,
        width,
        height: popup_height,
    };
    let items: Vec<ListItem> = ac
        .matches
        .iter()
        .map(|c| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("/{}", c.name), Style::default().fg(Color::Yellow)),
                Span::raw("  "),
                Span::styled(c.description, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();
    let list = List::new(items).block(Block::bordered()).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut list_state = ListState::default();
    list_state.select(Some(ac.selected));
    frame.render_widget(Clear, popup);
    frame.render_stateful_widget(list, popup, &mut list_state);
}

/// Per-screen key handler. `issue_request` is called to enqueue an outgoing
/// RPC; the caller (event loop) is responsible for bumping `pending` and
/// actually spawning the request task. When `waiting` is true, the input is
/// disabled: keys that type into the prompt or fire a new request are
/// swallowed; navigation, scroll, and quit keys still work.
pub fn handle_key(
    state: &mut ChatState,
    key: KeyEvent,
    waiting: bool,
    mut issue_request: impl FnMut(Request),
) -> Transition {
    if key.kind == KeyEventKind::Release {
        return Transition::None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        return Transition::Quit;
    }
    if waiting
        && (input::is_buffer_edit(&key)
            || matches!(key.code, KeyCode::Enter | KeyCode::Tab | KeyCode::BackTab))
    {
        return Transition::None;
    }
    match key.code {
        KeyCode::Esc => {
            if state.autocomplete.is_some() {
                state.autocomplete = None;
                Transition::None
            } else {
                Transition::Quit
            }
        }
        KeyCode::Tab => {
            state.cycle_autocomplete(true);
            Transition::None
        }
        KeyCode::BackTab => {
            state.cycle_autocomplete(false);
            Transition::None
        }
        KeyCode::Backspace => {
            state.input_backspace();
            Transition::None
        }
        KeyCode::Delete => {
            state.input_delete_forward();
            Transition::None
        }
        KeyCode::Left => {
            state.cursor_left();
            Transition::None
        }
        KeyCode::Right => {
            state.cursor_right();
            Transition::None
        }
        KeyCode::Home => {
            state.cursor_home();
            Transition::None
        }
        KeyCode::End => {
            state.cursor_end();
            Transition::None
        }
        KeyCode::Up => {
            state.scroll_offset = state.scroll_offset.saturating_sub(1);
            Transition::None
        }
        KeyCode::Down => {
            state.scroll_offset = state.scroll_offset.saturating_add(1);
            Transition::None
        }
        KeyCode::Enter => submit(state, &mut issue_request),
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'w' => state.input_delete_word_backward(),
                    'u' => state.input_delete_to_start(),
                    'k' => state.input_delete_to_end(),
                    'a' => state.cursor_home(),
                    'e' => state.cursor_end(),
                    _ => {}
                }
            } else {
                state.input_insert(c);
            }
            Transition::None
        }
        _ => Transition::None,
    }
}

fn submit(state: &mut ChatState, issue_request: &mut dyn FnMut(Request)) -> Transition {
    let input = state.take_input();
    if input.is_empty() {
        return Transition::None;
    }
    state.push_user(input.clone());

    if let Some(rest) = input.strip_prefix('/') {
        let (name, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        if name.is_empty() {
            state.push_error("empty command");
            return Transition::None;
        }
        let cmd = commands::find(name).or_else(|| commands::filter(name).into_iter().next());
        match cmd {
            Some(cmd) => match cmd.action {
                commands::Action::Quit => Transition::Quit,
                commands::Action::Send(builder) => {
                    issue_request(builder(args));
                    Transition::None
                }
                commands::Action::OpenScreen(intent) => super::open(intent),
                commands::Action::ClearChat => {
                    let mut fresh = ChatState::blank();
                    fresh.push_system("new conversation — type a message to begin");
                    *state = fresh;
                    Transition::None
                }
            },
            None => {
                state.push_error(format!("unknown command: /{name}"));
                Transition::None
            }
        }
    } else {
        issue_request(Request::Chat {
            conversation_id: state.conversation_id.clone(),
            message: input,
        });
        Transition::None
    }
}

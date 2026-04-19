use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::protocol::Response;

use super::commands::{self, SlashCommand};

pub struct App {
    pub transcript: Vec<Line<'static>>,
    pub input: String,
    pub cursor: usize,
    pub scroll_offset: u16,
    pub autocomplete: Option<Autocomplete>,
    pub pending: usize,
    pub should_quit: bool,
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
        let mut app = Self {
            transcript: Vec::new(),
            input: String::new(),
            cursor: 0,
            scroll_offset: 0,
            autocomplete: None,
            pending: 0,
            should_quit: false,
        };
        app.push_system("welcome to llm-rag — type a message, or try /ping or /quit");
        app
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

    pub fn push_error(&mut self, text: impl Into<String>) {
        self.transcript.push(Line::from(Span::styled(
            text.into(),
            Style::default().fg(Color::Red),
        )));
    }

    pub fn display_response(&mut self, resp: Response) {
        match resp {
            Response::Pong => self.push_server("Pong"),
            Response::Chat { reply } => self.push_server(reply),
            Response::Error { message } => self.push_error(format!("server error: {message}")),
        }
    }

    pub fn input_insert(&mut self, ch: char) {
        self.input.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.refresh_autocomplete();
    }

    pub fn input_backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.input[..self.cursor]
            .chars()
            .next_back()
            .map(char::len_utf8)
            .unwrap_or(0);
        if prev == 0 {
            return;
        }
        let new_cursor = self.cursor - prev;
        self.input.replace_range(new_cursor..self.cursor, "");
        self.cursor = new_cursor;
        self.refresh_autocomplete();
    }

    pub fn cursor_left(&mut self) {
        let prev = self.input[..self.cursor]
            .chars()
            .next_back()
            .map(char::len_utf8)
            .unwrap_or(0);
        self.cursor -= prev;
    }

    pub fn cursor_right(&mut self) {
        let next = self.input[self.cursor..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(0);
        self.cursor += next;
    }

    pub fn take_input(&mut self) -> String {
        let taken = std::mem::take(&mut self.input);
        self.cursor = 0;
        self.autocomplete = None;
        taken
    }

    pub fn cycle_autocomplete(&mut self, forward: bool) {
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

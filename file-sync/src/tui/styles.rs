use ratatui::style::{Color, Modifier, Style};

pub const EQUAL: Style = Style::new().bg(Color::Reset).fg(Color::Reset);
pub const INSERT: Style = Style::new().bg(Color::Green).fg(Color::Black);
pub const DELETE: Style = Style::new().bg(Color::Red).fg(Color::Black);
pub const LINE_NUMBER: Style = Style::new()
    .fg(Color::DarkGray)
    .bg(Color::Reset)
    .add_modifier(Modifier::DIM);
pub const HEADER: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);
pub const FOOTER: Style = Style::new().fg(Color::DarkGray);

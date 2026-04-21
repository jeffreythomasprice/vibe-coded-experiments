//! Shared spinner/banner helpers. The frame counter lives on `App`; every
//! renderer that wants animation reads it and indexes into `FRAMES` here.

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;

pub const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const TICK: Duration = Duration::from_millis(100);
pub const WAIT_TEXT: &str = "waiting on a response…";

pub fn frame(idx: usize) -> &'static str {
    FRAMES[idx % FRAMES.len()]
}

pub fn render_banner(f: &mut Frame, area: Rect, spinner_idx: usize) {
    let text = format!(" {} {}", frame(spinner_idx), WAIT_TEXT);
    let p = Paragraph::new(text).style(Style::default().fg(Color::Yellow));
    f.render_widget(p, area);
}

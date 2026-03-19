use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::app::App;
use super::styles;
use super::widgets::render_pane;

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(3),   // diff panes
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    draw_header(frame, chunks[0], &app.entity_name);
    draw_panes(frame, chunks[1], app);
    draw_footer(frame, chunks[2], app);
}

fn draw_header(frame: &mut Frame, area: Rect, entity_name: &str) {
    let header = Paragraph::new(Line::from(Span::styled(
        format!(" file-sync: {} ", entity_name),
        styles::HEADER,
    )));
    frame.render_widget(header, area);
}

fn draw_panes(frame: &mut Frame, area: Rect, app: &App) {
    let pane_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    if app.is_binary {
        draw_binary_pane(frame, pane_chunks[0], app, 0);
        draw_binary_pane(frame, pane_chunks[1], app, 1);
    } else {
        let left_title = app.pane_title(0);
        let left_block = Block::default().borders(Borders::ALL).title(left_title);
        let left_para = render_pane(&app.rows, app.scroll, true).block(left_block);
        frame.render_widget(left_para, pane_chunks[0]);

        let right_title = app.pane_title(1);
        let right_block = Block::default().borders(Borders::ALL).title(right_title);
        let right_para = render_pane(&app.rows, app.scroll, false).block(right_block);
        frame.render_widget(right_para, pane_chunks[1]);
    }
}

fn draw_binary_pane(frame: &mut Frame, area: Rect, app: &App, index: usize) {
    let title = app.pane_title(index);
    let info = if index < app.copy_info.len() {
        let (size, mtime) = &app.copy_info[index];
        format!("Size: {} bytes\nModified: {}", size, mtime)
    } else {
        "No info".to_string()
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let para = Paragraph::new(info).block(block);
    frame.render_widget(para, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let keys = if app.is_binary {
        format!(
            " [1-{}] Keep copy  [s] Skip  [q] Quit",
            app.copy_info.len()
        )
    } else {
        format!(
            " [1-{}] Keep copy  [s] Skip  [q] Quit  [↑↓/jk] Scroll",
            app.copy_info.len()
        )
    };
    let footer = Paragraph::new(Line::from(Span::styled(keys, styles::FOOTER)));
    frame.render_widget(footer, area);
}

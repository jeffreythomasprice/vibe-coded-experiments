//! Rendering for the tui. Pure: [`draw`] reads [`App`] and paints a frame, with
//! no side effects and no I/O of its own.
//!
//! The view is a placeholder status dashboard: a connection panel (link state,
//! server identity, time since last ping) on top, the connected-client list
//! below, and a one-line key hint at the bottom.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::App;

/// Paint one frame of the UI.
pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // connection panel
            Constraint::Min(3),    // clients panel
            Constraint::Length(1), // footer hint
        ])
        .split(frame.area());

    draw_connection(frame, app, chunks[0]);
    draw_clients(frame, app, chunks[1]);
    draw_footer(frame, chunks[2]);
}

fn draw_connection(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let (status_text, status_color) = if app.connected {
        ("● connected", Color::Green)
    } else {
        ("● disconnected", Color::Red)
    };

    let last_ping = match app.last_ping {
        Some(at) => format!("{}s ago", at.elapsed().as_secs()),
        None => "never".to_owned(),
    };

    let mut lines = vec![
        Line::from(vec![
            Span::raw("status:  "),
            Span::styled(
                status_text,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(format!("server:  {}", app.server.name)),
        Line::from(format!(
            "version: {}   id: {}",
            app.server.version, app.server.assigned_id
        )),
        Line::from(format!("last ping: {last_ping}")),
    ];

    if let Some(err) = &app.error {
        lines.push(Line::from(vec![
            Span::raw("error:   "),
            Span::styled(err.clone(), Style::default().fg(Color::Red)),
        ]));
    }

    let panel = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Connection "));
    frame.render_widget(panel, area);
}

fn draw_clients(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let title = format!(" Clients ({}) ", app.clients.len());

    let items: Vec<ListItem> = if app.clients.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "(waiting for stats…)",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        app.clients
            .iter()
            .map(|c| {
                let name = if c.name.is_empty() { "—" } else { &c.name };
                ListItem::new(Line::from(format!(
                    "#{:<4} {:<16} idle {}s",
                    c.id,
                    name,
                    c.idle_ms / 1000
                )))
            })
            .collect()
    };

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(list, area);
}

fn draw_footer(frame: &mut Frame, area: ratatui::layout::Rect) {
    let hint = Paragraph::new(Line::from(Span::styled(
        " Press Esc to quit ",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(hint, area);
}

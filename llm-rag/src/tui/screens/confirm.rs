//! Generic two-button confirmation dialog. Drives delete flows for any
//! `DeleteTarget` — the target's `back_transition()` + `delete_request()`
//! handle the routing, so new entity types don't touch this file.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::Transition;
use super::targets::DeleteTarget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Delete,
    Cancel,
}

pub struct ConfirmDeleteState {
    pub target: DeleteTarget,
    pub label: String,
    pub choice: Choice,
}

impl ConfirmDeleteState {
    pub fn new(target: DeleteTarget, label: String) -> Self {
        Self {
            target,
            label,
            choice: Choice::Cancel,
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &ConfirmDeleteState) {
    // Center a modal roughly in the middle of the available area.
    let modal = centered_rect(60, 30, area);
    frame.render_widget(Clear, modal);

    let block = Block::bordered().title(" confirm delete ");
    frame.render_widget(&block, modal);
    let inner = block.inner(modal);

    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(inner);

    let msg = Paragraph::new(format!(
        "Delete {}:\n\n  {}\n\nThis cannot be undone.",
        state.target.kind(),
        state.label
    ));
    frame.render_widget(msg, chunks[0]);

    let buttons = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let mk_button = |label: &str, active: bool| {
        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        Paragraph::new(Line::from(vec![Span::styled(format!(" {label} "), style)]))
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::bordered())
    };

    frame.render_widget(
        mk_button("Delete", matches!(state.choice, Choice::Delete)),
        buttons[0],
    );
    frame.render_widget(
        mk_button("Cancel", matches!(state.choice, Choice::Cancel)),
        buttons[1],
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let v_margin = (area.height.saturating_sub(area.height * percent_y / 100)) / 2;
    let h_margin = (area.width.saturating_sub(area.width * percent_x / 100)) / 2;
    Rect {
        x: area.x + h_margin,
        y: area.y + v_margin,
        width: area.width * percent_x / 100,
        height: area.height * percent_y / 100,
    }
}

/// Action bubbled up to the event loop. `Confirm` means "fire the delete
/// RPC, then transition back"; `Back` means cancel without deleting.
pub enum ConfirmAction {
    None,
    Confirm,
    Back,
}

pub fn handle_key(state: &mut ConfirmDeleteState, key: KeyEvent) -> ConfirmAction {
    if key.kind == KeyEventKind::Release {
        return ConfirmAction::None;
    }
    match key.code {
        KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
            state.choice = match state.choice {
                Choice::Delete => Choice::Cancel,
                Choice::Cancel => Choice::Delete,
            };
            ConfirmAction::None
        }
        KeyCode::Esc => ConfirmAction::Back,
        KeyCode::Enter => match state.choice {
            Choice::Delete => ConfirmAction::Confirm,
            Choice::Cancel => ConfirmAction::Back,
        },
        _ => ConfirmAction::None,
    }
}

pub fn back_transition(state: &ConfirmDeleteState) -> Transition {
    state.target.back_transition()
}

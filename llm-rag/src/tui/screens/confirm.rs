//! Generic two-button confirmation dialog. Currently only used for conversation
//! deletion; generalize the variants in this file when another confirm flow
//! lands.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use super::{ConversationListState, Screen, Transition};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Delete,
    Cancel,
}

/// Where to return after the confirmation resolves. The state machine stays
/// narrow in this phase; wire more variants as more call sites show up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnTo {
    ConversationList,
}

pub struct ConfirmDeleteState {
    pub conv_id: String,
    pub label: String,
    pub choice: Choice,
    pub return_to: ReturnTo,
}

impl ConfirmDeleteState {
    pub fn new(conv_id: String, label: String, return_to: ReturnTo) -> Self {
        Self {
            conv_id,
            label,
            choice: Choice::Cancel,
            return_to,
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
        "Delete conversation:\n\n  {}\n\nThis cannot be undone.",
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
    match state.return_to {
        ReturnTo::ConversationList => Transition::To(Screen::ConversationList(
            ConversationListState::new_loading(),
        )),
    }
}

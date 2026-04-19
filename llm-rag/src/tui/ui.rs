use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use super::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(frame.area());
    let transcript_area = chunks[0];
    let input_area = chunks[1];

    render_transcript(frame, transcript_area, app);
    render_input(frame, input_area, app);
    render_autocomplete(frame, input_area, app);
}

fn render_transcript(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered().title(" llm-rag ");
    let paragraph = Paragraph::new(app.transcript.clone())
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0))
        .block(block);
    frame.render_widget(paragraph, area);
}

fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    let title = if app.pending > 0 { " … " } else { " > " };
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    let paragraph = Paragraph::new(app.input.as_str()).block(block);
    frame.render_widget(paragraph, area);

    let col = app.input[..app.cursor].chars().count() as u16;
    let x = inner.x + col;
    let y = inner.y;
    if x < inner.x.saturating_add(inner.width) && inner.height > 0 {
        frame.set_cursor_position(Position { x, y });
    }
}

fn render_autocomplete(frame: &mut Frame, input_area: Rect, app: &App) {
    let Some(ac) = app.autocomplete.as_ref() else {
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
    let mut state = ListState::default();
    state.select(Some(ac.selected));
    frame.render_widget(Clear, popup);
    frame.render_stateful_widget(list, popup, &mut state);
}

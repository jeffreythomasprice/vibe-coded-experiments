pub mod app;
pub mod diff_model;
pub mod layout;
pub mod styles;
pub mod widgets;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, UserChoice};
use crate::diff::ConflictInfo;

pub fn run_diff_dialog(info: &ConflictInfo) -> Result<UserChoice> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(info);

    let result = loop {
        terminal.draw(|frame| layout::draw(frame, &app))?;

        if app.handle_event()? {
            break app.choice.unwrap_or(UserChoice::Skip);
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(result)
}

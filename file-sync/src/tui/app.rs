use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use super::diff_model::{build_side_by_side, DiffRow};
use crate::diff::ConflictInfo;

#[derive(Debug, Clone, PartialEq)]
pub enum UserChoice {
    Keep(usize), // index of copy to keep
    Skip,
    Quit,
}

pub struct App {
    pub entity_name: String,
    pub rows: Vec<DiffRow>,
    pub scroll: usize,
    pub is_binary: bool,
    pub copy_info: Vec<(u64, String)>, // (size, mtime_string) per copy
    pub paths: Vec<String>,
    pub choice: Option<UserChoice>,
    pub num_copies: usize,
}

impl App {
    pub fn new(info: &ConflictInfo) -> Self {
        let rows = if let (Some(left), Some(right)) = (&info.left_text, &info.right_text) {
            build_side_by_side(left, right)
        } else {
            Vec::new()
        };

        let copy_info: Vec<(u64, String)> = info
            .copies
            .iter()
            .map(|c| (c.size, c.mtime_string()))
            .collect();

        let paths: Vec<String> = info
            .copies
            .iter()
            .map(|c| c.path.display().to_string())
            .collect();

        App {
            entity_name: info.entity_name.clone(),
            rows,
            scroll: 0,
            is_binary: info.is_binary,
            copy_info,
            num_copies: info.copies.len(),
            paths,
            choice: None,
        }
    }

    pub fn pane_title(&self, index: usize) -> String {
        if index < self.paths.len() && index < self.copy_info.len() {
            format!(
                " {} ({}) ",
                self.paths[index], self.copy_info[index].1
            )
        } else {
            String::new()
        }
    }

    pub fn scroll_down(&mut self, amount: usize) {
        let max = self.rows.len().saturating_sub(1);
        self.scroll = (self.scroll + amount).min(max);
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll = self.rows.len().saturating_sub(1);
    }

    pub fn handle_event(&mut self) -> anyhow::Result<bool> {
        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                return Ok(false);
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.choice = Some(UserChoice::Quit);
                    return Ok(true);
                }
                KeyCode::Char('s') => {
                    self.choice = Some(UserChoice::Skip);
                    return Ok(true);
                }
                KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                    let n = (c as usize) - ('0' as usize);
                    if n <= self.num_copies {
                        self.choice = Some(UserChoice::Keep(n - 1));
                        return Ok(true);
                    }
                }
                KeyCode::Char('j') | KeyCode::Down => self.scroll_down(1),
                KeyCode::Char('k') | KeyCode::Up => self.scroll_up(1),
                KeyCode::Char('d') | KeyCode::PageDown => self.scroll_down(20),
                KeyCode::Char('u') | KeyCode::PageUp => self.scroll_up(20),
                KeyCode::Char('g') | KeyCode::Home => self.scroll_to_top(),
                KeyCode::Char('G') | KeyCode::End => self.scroll_to_bottom(),
                _ => {}
            }
        }
        Ok(false)
    }
}

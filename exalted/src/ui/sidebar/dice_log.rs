//! Dice Log panel body and its in-memory log structure.
//!
//! Session-only: the log lives on `AppState` and is wiped on app restart.
//! Not part of the saved character file.

use chrono::{DateTime, Local};

use crate::character::MotePool;
use crate::ui::state::AppState;

#[derive(Debug, Clone)]
pub struct DiceLogEntry {
    /// Short headline, e.g. "Attack: Daiklave" or "Join Battle".
    pub label: String,
    /// Math shown to the user, e.g. "Dex 4 + Martial Arts 3 + acc 2 (−1 wound) = 8d".
    pub formula: String,
    pub rolls: Vec<u8>,
    /// Successes from the dice themselves (before bonus successes).
    pub dice_successes: u8,
    /// Bonus successes from charms (Second Excellency).
    pub bonus_successes: u8,
    pub botch: bool,
    /// Motes spent for this roll (0 if none).
    pub motes_spent: u16,
    pub mote_pool: Option<MotePool>,
    /// Local time the roll happened. Not displayed in v1, but kept so that
    /// future features (sort, group-by-session) have it for free.
    #[allow(dead_code)]
    pub when: DateTime<Local>,
}

impl DiceLogEntry {
    pub fn total_successes(&self) -> u8 {
        self.dice_successes.saturating_add(self.bonus_successes)
    }

    /// One-line plain-text format used in the panel and by Copy. Matches the
    /// example in the user spec:
    ///   "Strength + Martial Arts = 5d → 1, 4, 4, 3, 2 → 2 successes (−1m personal)"
    pub fn formatted(&self) -> String {
        let rolls_str = if self.rolls.is_empty() {
            "(no dice)".to_string()
        } else {
            self.rolls
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let result_str = if self.botch {
            "BOTCH".to_string()
        } else {
            let total = self.total_successes();
            let suffix = if total == 1 { "success" } else { "successes" };
            if self.bonus_successes > 0 {
                format!(
                    "{} {} (+{} from charms)",
                    total, suffix, self.bonus_successes
                )
            } else {
                format!("{} {}", total, suffix)
            }
        };
        let cost = match (self.motes_spent, self.mote_pool) {
            (0, _) | (_, None) => String::new(),
            (n, Some(MotePool::Personal)) => format!(" (−{}m personal)", n),
            (n, Some(MotePool::Peripheral)) => format!(" (−{}m peripheral)", n),
        };
        format!(
            "{}: {} → {} → {}{}",
            self.label, self.formula, rolls_str, result_str, cost
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiceLog {
    pub entries: Vec<DiceLogEntry>,
}

impl DiceLog {
    pub fn push(&mut self, entry: DiceLogEntry) {
        // Cap at 500 to keep the panel responsive in long sessions.
        const MAX: usize = 500;
        if self.entries.len() >= MAX {
            self.entries.remove(0);
        }
        self.entries.push(entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn to_clipboard_text(&self) -> String {
        self.entries
            .iter()
            .map(DiceLogEntry::formatted)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

pub fn render_body(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        let empty = state.dice_log.entries.is_empty();
        if ui.add_enabled(!empty, egui::Button::new("Clear")).clicked() {
            state.dice_log.clear();
        }
        if ui.add_enabled(!empty, egui::Button::new("Copy")).clicked() {
            let text = state.dice_log.to_clipboard_text();
            ui.ctx().copy_text(text);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.small(format!("{} entries", state.dice_log.entries.len()));
        });
    });
    ui.separator();

    egui::ScrollArea::vertical()
        .id_salt("dice-log-entries")
        .auto_shrink([false; 2])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            if state.dice_log.entries.is_empty() {
                ui.label(egui::RichText::new("No rolls yet. Use the Actions panel.").weak());
                return;
            }
            for entry in &state.dice_log.entries {
                if entry.botch {
                    ui.colored_label(egui::Color32::from_rgb(220, 90, 90), entry.formatted());
                } else {
                    ui.label(entry.formatted());
                }
            }
        });
}

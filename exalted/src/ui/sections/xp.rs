//! XP section: xp_earned / xp_banked + the xp_awards timestamped history.
//! The derived "spent" total is shown read-only as a sanity check.

use chrono::Utc;

use crate::character::XpAward;
use crate::ui::state::AppState;
use crate::ui::widgets::icon_button::trash_button;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Experience");

    let mut any = false;

    let earned = state.character.xp_earned;
    let banked = state.character.xp_banked;
    let spent = crate::character::xp::total_xp_spent(&state.character);

    ui.horizontal(|ui| {
        ui.label("Earned");
        let resp = ui.add(
            egui::DragValue::new(&mut state.character.xp_earned)
                .range(0u32..=100_000)
                .speed(0.25),
        );
        if resp.changed() {
            any = true;
        }
        ui.label("Banked");
        let resp = ui.add(
            egui::DragValue::new(&mut state.character.xp_banked)
                .range(0u32..=100_000)
                .speed(0.25),
        );
        if resp.changed() {
            any = true;
        }
        ui.small(format!(
            "(spent: {} | earned − spent = {})",
            spent,
            (earned as i64) - (spent as i64),
        ));
        if (earned as i64) - (spent as i64) != banked as i64 {
            ui.colored_label(
                egui::Color32::from_rgb(220, 180, 80),
                "earned − spent ≠ banked",
            );
        }
    });

    ui.add_space(6.0);
    ui.label("XP awards (must sum to Earned when non-empty)");

    if ui.button("+ Add award").clicked() {
        state.character.xp_awards.push(XpAward::new(0, ""));
        any = true;
    }

    let mut delete_idx: Option<usize> = None;
    for (i, award) in state.character.xp_awards.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let prev_amount = award.amount;
            let resp = ui.add(
                egui::DragValue::new(&mut award.amount)
                    .range(0u32..=1_000)
                    .suffix(" xp"),
            );
            if resp.changed() && award.amount != prev_amount {
                award.updated_at = Utc::now();
                any = true;
            }
            ui.small(award.created_at.format("%Y-%m-%d").to_string());
            let prev_body = award.body.clone();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut award.body)
                    .desired_width(f32::INFINITY)
                    .hint_text("session / award note"),
            );
            if resp.changed() && award.body != prev_body {
                award.updated_at = Utc::now();
                any = true;
            }
            if trash_button(ui).clicked() {
                delete_idx = Some(i);
            }
        });
    }
    if let Some(i) = delete_idx {
        state.character.xp_awards.remove(i);
        any = true;
    }

    let awards_sum: u32 = state.character.xp_awards.iter().map(|a| a.amount).sum();
    if !state.character.xp_awards.is_empty() && awards_sum != state.character.xp_earned {
        ui.colored_label(
            egui::Color32::from_rgb(220, 180, 80),
            format!(
                "awards sum to {} but Earned = {}",
                awards_sum, state.character.xp_earned,
            ),
        );
    }

    if any {
        state.mark_dirty_with("xp");
    }
}

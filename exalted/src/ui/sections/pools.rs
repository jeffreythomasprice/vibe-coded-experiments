//! Pools section: mote spend / committed motes / WP temp+spent / health
//! damage / Limit / per-virtue channel counters.

use crate::character::{MoteCommitment, MotePool, VirtueKind};
use crate::render::names::virtue_name;
use crate::ui::state::AppState;
use crate::ui::widgets::icon_button::trash_button;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Pools & In-Play State");

    let mut any = false;

    // Mote spend.
    ui.horizontal(|ui| {
        ui.label("Motes spent");
        let resp = ui.add(
            egui::DragValue::new(&mut state.character.pool_state.personal_motes_spent)
                .range(0u16..=99)
                .prefix("personal "),
        );
        if resp.changed() {
            any = true;
        }
        let resp = ui.add(
            egui::DragValue::new(&mut state.character.pool_state.peripheral_motes_spent)
                .range(0u16..=999)
                .prefix("peripheral "),
        );
        if resp.changed() {
            any = true;
        }
    });

    // Committed motes.
    ui.label("Committed motes");
    let mut delete_idx: Option<usize> = None;
    for (i, c) in state
        .character
        .pool_state
        .committed_motes
        .iter_mut()
        .enumerate()
    {
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut c.name)
                    .desired_width(200.0)
                    .hint_text("name"),
            );
            if resp.changed() {
                any = true;
            }
            let prev_pool = c.pool;
            egui::ComboBox::from_id_salt(("commit-pool", i))
                .selected_text(match c.pool {
                    MotePool::Personal => "personal",
                    MotePool::Peripheral => "peripheral",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut c.pool, MotePool::Personal, "personal");
                    ui.selectable_value(&mut c.pool, MotePool::Peripheral, "peripheral");
                });
            if c.pool != prev_pool {
                any = true;
            }
            let resp = ui.add(
                egui::DragValue::new(&mut c.motes)
                    .range(0u16..=999)
                    .suffix("m"),
            );
            if resp.changed() {
                any = true;
            }
            if trash_button(ui).clicked() {
                delete_idx = Some(i);
            }
        });
    }
    if let Some(i) = delete_idx {
        state.character.pool_state.committed_motes.remove(i);
        any = true;
    }
    if ui.button("+ Add commitment").clicked() {
        state
            .character
            .pool_state
            .committed_motes
            .push(MoteCommitment {
                name: String::new(),
                pool: MotePool::Personal,
                motes: 0,
            });
        any = true;
    }

    ui.add_space(6.0);

    // Willpower.
    ui.horizontal(|ui| {
        ui.label("Willpower");
        let resp = ui.add(
            egui::DragValue::new(&mut state.character.pool_state.willpower_temporary)
                .range(0u8..=10)
                .prefix("temp "),
        );
        if resp.changed() {
            any = true;
        }
        let resp = ui.add(
            egui::DragValue::new(&mut state.character.pool_state.willpower_permanent_spent)
                .range(0u8..=10)
                .prefix("perm spent "),
        );
        if resp.changed() {
            any = true;
        }
    });

    // Health damage.
    ui.horizontal(|ui| {
        ui.label("Damage");
        let resp = ui.add(
            egui::DragValue::new(&mut state.character.pool_state.health_damage.bashing)
                .range(0u8..=30)
                .prefix("B "),
        );
        if resp.changed() {
            any = true;
        }
        let resp = ui.add(
            egui::DragValue::new(&mut state.character.pool_state.health_damage.lethal)
                .range(0u8..=30)
                .prefix("L "),
        );
        if resp.changed() {
            any = true;
        }
        let resp = ui.add(
            egui::DragValue::new(&mut state.character.pool_state.health_damage.aggravated)
                .range(0u8..=30)
                .prefix("A "),
        );
        if resp.changed() {
            any = true;
        }
    });

    // Limit.
    ui.horizontal(|ui| {
        ui.label("Limit");
        let resp = ui.add(
            egui::DragValue::new(&mut state.character.pool_state.limit)
                .range(0u8..=10)
                .suffix("/10"),
        );
        if resp.changed() {
            any = true;
        }
        if state.character.pool_state.is_at_limit_break() {
            ui.colored_label(egui::Color32::from_rgb(255, 90, 90), "LIMIT BREAK");
        }
    });

    // Virtue channels.
    ui.label("Virtue channels used this story");
    ui.horizontal(|ui| {
        for v in VirtueKind::ALL {
            let count = state
                .character
                .pool_state
                .virtue_channels_used
                .entry(*v)
                .or_insert(0);
            let resp = ui.add(
                egui::DragValue::new(count)
                    .range(0u8..=10)
                    .prefix(format!("{} ", virtue_name(*v))),
            );
            if resp.changed() {
                any = true;
            }
        }
    });

    if any {
        state.mark_dirty();
    }
}

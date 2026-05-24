//! Familiar section: Option<Familiar> editor. Add/remove button + name +
//! a damage track row.

use crate::character::Familiar;
use crate::ui::state::AppState;
use crate::ui::widgets::icon_button::trash_button_with_label;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Familiar");
    let mut any_changed = false;
    let mut remove = false;

    if state.character.familiar.is_none() {
        if ui.button("+ Add familiar").clicked() {
            state.character.familiar = Some(Familiar::default());
            any_changed = true;
        }
    } else if let Some(familiar) = state.character.familiar.as_mut() {
        ui.horizontal(|ui| {
            ui.label("Name");
            let resp = ui.add(egui::TextEdit::singleline(&mut familiar.name).desired_width(240.0));
            if resp.changed() {
                any_changed = true;
            }
            if trash_button_with_label(ui, "remove").clicked() {
                remove = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Damage (B / L / A)");
            if ui
                .add(
                    egui::DragValue::new(&mut familiar.health_damage.bashing)
                        .range(0u8..=30)
                        .prefix("B "),
                )
                .changed()
            {
                any_changed = true;
            }
            if ui
                .add(
                    egui::DragValue::new(&mut familiar.health_damage.lethal)
                        .range(0u8..=30)
                        .prefix("L "),
                )
                .changed()
            {
                any_changed = true;
            }
            if ui
                .add(
                    egui::DragValue::new(&mut familiar.health_damage.aggravated)
                        .range(0u8..=30)
                        .prefix("A "),
                )
                .changed()
            {
                any_changed = true;
            }
        });
    }

    if remove {
        state.character.familiar = None;
        any_changed = true;
    }
    if any_changed {
        state.mark_dirty();
    }
}

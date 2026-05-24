//! Hearthstones section: Vec<Hearthstone> editor.

use crate::character::Hearthstone;
use crate::ui::state::AppState;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.hearthstones.len();
    ui.heading(format!("Hearthstones ({})", count));

    if ui.button("+ Add hearthstone").clicked() {
        state.character.hearthstones.push(Hearthstone {
            name: String::new(),
            level: 1,
            aspect: String::new(),
            description: String::new(),
        });
        state.mark_dirty();
    }

    let mut any = false;
    let mut delete_idx: Option<usize> = None;
    for (i, hs) in state.character.hearthstones.iter_mut().enumerate() {
        egui::CollapsingHeader::new(if hs.name.is_empty() {
            format!("(unnamed hearthstone #{})", i + 1)
        } else {
            format!("{} ({}★)", hs.name, hs.level)
        })
        .id_salt(("hs", i))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                let resp = ui.add(egui::TextEdit::singleline(&mut hs.name).desired_width(240.0));
                if resp.changed() {
                    any = true;
                }
                ui.label("Level");
                let resp = ui.add(egui::DragValue::new(&mut hs.level).range(1u8..=5));
                if resp.changed() {
                    any = true;
                }
                ui.label("Aspect");
                let resp = ui.add(egui::TextEdit::singleline(&mut hs.aspect).desired_width(140.0));
                if resp.changed() {
                    any = true;
                }
                if ui.small_button("✕").clicked() {
                    delete_idx = Some(i);
                }
            });
            ui.label("Description");
            let resp = ui.add(
                egui::TextEdit::multiline(&mut hs.description)
                    .desired_width(f32::INFINITY)
                    .desired_rows(2),
            );
            if resp.changed() {
                any = true;
            }
        });
    }
    if let Some(i) = delete_idx {
        state.character.hearthstones.remove(i);
        any = true;
    }
    if any {
        state.mark_dirty();
    }
}

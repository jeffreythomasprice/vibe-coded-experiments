//! Top menu bar. Renders into the TopBottomPanel set up by `App::update`,
//! and signals user intent via a `MenuAction` enum that the App dispatches.

use crate::ui::persisted::ThemePreference;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MenuAction {
    New,
    Open,
    Save,
    SaveAs,
    ExportPdf,
    Quit,
    ToggleSidebar,
    ToggleValidationPanel,
    ToggleActionsPanel,
    ToggleDiceLogPanel,
    SetTheme(ThemePreference),
}

pub fn render(ui: &mut egui::Ui, current_theme: ThemePreference) -> Option<MenuAction> {
    let mut action = None;
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("New").clicked() {
                action = Some(MenuAction::New);
                ui.close();
            }
            if ui.button("Open…").clicked() {
                action = Some(MenuAction::Open);
                ui.close();
            }
            ui.separator();
            if ui.button("Save").clicked() {
                action = Some(MenuAction::Save);
                ui.close();
            }
            if ui.button("Save As…").clicked() {
                action = Some(MenuAction::SaveAs);
                ui.close();
            }
            ui.separator();
            if ui.button("Export PDF…").clicked() {
                action = Some(MenuAction::ExportPdf);
                ui.close();
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                action = Some(MenuAction::Quit);
                ui.close();
            }
        });
        ui.menu_button("View", |ui| {
            if ui.button("Toggle derived sidebar").clicked() {
                action = Some(MenuAction::ToggleSidebar);
                ui.close();
            }
            if ui.button("Toggle validation panel").clicked() {
                action = Some(MenuAction::ToggleValidationPanel);
                ui.close();
            }
            if ui.button("Toggle actions panel").clicked() {
                action = Some(MenuAction::ToggleActionsPanel);
                ui.close();
            }
            if ui.button("Toggle dice log panel").clicked() {
                action = Some(MenuAction::ToggleDiceLogPanel);
                ui.close();
            }
            ui.separator();
            ui.menu_button("Theme", |ui| {
                for pref in ThemePreference::ALL {
                    if ui
                        .selectable_label(current_theme == pref, pref.label())
                        .clicked()
                    {
                        action = Some(MenuAction::SetTheme(pref));
                        ui.close();
                    }
                }
            });
        });
    });
    action
}

/// Translate keyboard shortcuts into menu actions. Called from
/// `App::update` once per frame before menu rendering.
pub fn shortcut_action(ctx: &egui::Context) -> Option<MenuAction> {
    use egui::{Key, Modifiers};
    let mut hit = None;
    ctx.input_mut(|i| {
        if i.consume_key(Modifiers::COMMAND, Key::N) {
            hit = Some(MenuAction::New);
        } else if i.consume_key(Modifiers::COMMAND, Key::O) {
            hit = Some(MenuAction::Open);
        } else if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::S) {
            hit = Some(MenuAction::SaveAs);
        } else if i.consume_key(Modifiers::COMMAND, Key::S) {
            hit = Some(MenuAction::Save);
        } else if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::P) {
            hit = Some(MenuAction::ExportPdf);
        } else if i.consume_key(Modifiers::COMMAND, Key::Q) {
            hit = Some(MenuAction::Quit);
        }
    });
    hit
}

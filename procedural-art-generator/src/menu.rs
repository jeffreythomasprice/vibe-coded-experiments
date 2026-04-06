use crate::parameterized_grid::ParamDef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuPanel {
    ArtInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayState {
    Hidden,
    Visible(MenuPanel),
}

impl OverlayState {
    pub fn is_visible(&self) -> bool {
        matches!(self, Self::Visible(_))
    }

    pub fn toggle(&mut self) {
        *self = match self {
            Self::Hidden => Self::Visible(MenuPanel::ArtInfo),
            Self::Visible(_) => Self::Hidden,
        };
    }
}

pub fn draw_parameterized_graphics_info_panel(
    ctx: &egui::Context,
    description: &str,
    param_defs: &[ParamDef],
) {
    egui::Window::new("Parameterized Graphics Info")
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(description);
            ui.separator();
            ui.heading("Parameters");
            for def in param_defs {
                ui.horizontal(|ui| {
                    ui.strong(&def.description);
                    ui.label(format!("{}", def.range));
                });
            }
        });
}

use crate::parameterized_grid::{ParamDef, ParamRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPanel {
    Main,
    Grid,
    SelectGenerator,
    Parameters,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayState {
    Hidden,
    Visible,
}

impl OverlayState {
    pub fn is_visible(&self) -> bool {
        matches!(self, Self::Visible)
    }

    pub fn toggle(&mut self) {
        *self = match self {
            Self::Hidden => Self::Visible,
            Self::Visible => Self::Hidden,
        };
    }
}

pub const GENERATOR_NAMES: &[&str] = &["HueCircle", "PerlinNoise"];

pub struct MenuState {
    pub panel: MenuPanel,
    pub pending_rows: usize,
    pub pending_cols: usize,
    pub pending_ranges: Vec<ParamRange>,
    pub needs_regenerate: bool,
    pub needs_generator_switch: bool,
    pub selected_generator: usize,
}

impl MenuState {
    pub fn new(rows: usize, cols: usize, param_ranges: &[ParamRange]) -> Self {
        Self {
            panel: MenuPanel::Main,
            pending_rows: rows,
            pending_cols: cols,
            pending_ranges: param_ranges.to_vec(),
            needs_regenerate: false,
            needs_generator_switch: false,
            selected_generator: 0,
        }
    }

}

pub fn draw_menu(ctx: &egui::Context, state: &mut MenuState, param_defs: &[ParamDef]) {
    egui::Window::new("Menu")
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .min_width(350.0)
        .show(ctx, |ui| match state.panel {
            MenuPanel::Main => draw_main_panel(ui, state),
            MenuPanel::Grid => draw_grid_panel(ui, state),
            MenuPanel::SelectGenerator => draw_select_generator_panel(ui, state),
            MenuPanel::Parameters => draw_parameters_panel(ui, state, param_defs),
        });
}

fn draw_main_panel(ui: &mut egui::Ui, state: &mut MenuState) {
    ui.heading("Menu");
    ui.add_space(8.0);

    if ui.button("Grid").clicked() {
        state.panel = MenuPanel::Grid;
    }
    if ui.button("Select Generator").clicked() {
        state.panel = MenuPanel::SelectGenerator;
    }
    if ui.button("Parameters").clicked() {
        state.panel = MenuPanel::Parameters;
    }
}

fn draw_grid_panel(ui: &mut egui::Ui, state: &mut MenuState) {
    if ui.button("← Back").clicked() {
        state.panel = MenuPanel::Main;
    }
    ui.separator();
    ui.heading("Grid Size");
    ui.add_space(8.0);

    let mut rows = state.pending_rows as u32;
    let mut cols = state.pending_cols as u32;

    ui.horizontal(|ui| {
        ui.label("Rows:");
        ui.add(egui::DragValue::new(&mut rows).range(1..=100));
    });
    ui.horizontal(|ui| {
        ui.label("Columns:");
        ui.add(egui::DragValue::new(&mut cols).range(1..=100));
    });

    state.pending_rows = rows as usize;
    state.pending_cols = cols as usize;

    ui.add_space(12.0);
    if ui.button("Confirm").clicked() {
        state.needs_regenerate = true;
    }
}

fn draw_select_generator_panel(ui: &mut egui::Ui, state: &mut MenuState) {
    if ui.button("← Back").clicked() {
        state.panel = MenuPanel::Main;
    }
    ui.separator();
    ui.heading("Select Generator");
    ui.add_space(8.0);

    for (i, name) in GENERATOR_NAMES.iter().enumerate() {
        let selected = state.selected_generator == i;
        let label = if selected {
            format!("> {name}")
        } else {
            name.to_string()
        };
        if ui.button(label).clicked() {
            if state.selected_generator != i {
                state.selected_generator = i;
                state.needs_generator_switch = true;
            }
            state.panel = MenuPanel::Main;
        }
    }
}

fn draw_parameters_panel(ui: &mut egui::Ui, state: &mut MenuState, param_defs: &[ParamDef]) {
    if ui.button("← Back").clicked() {
        state.panel = MenuPanel::Main;
    }
    ui.separator();
    ui.heading("Parameters");
    ui.add_space(8.0);

    for (i, def) in param_defs.iter().enumerate() {
        if i >= state.pending_ranges.len() {
            break;
        }

        let full_range = &def.range;
        let full_min = full_range.min_f32();
        let full_max = full_range.max_f32();
        let is_int = full_range.is_integer();

        let mut current_min = state.pending_ranges[i].min_f32();
        let mut current_max = state.pending_ranges[i].max_f32();

        ui.strong(&def.description);

        ui.horizontal(|ui| {
            ui.label("Min:");
            let mut slider = egui::Slider::new(&mut current_min, full_min..=full_max);
            if is_int {
                slider = slider.step_by(1.0);
            }
            ui.add(slider);
        });

        ui.horizontal(|ui| {
            ui.label("Max:");
            let mut slider = egui::Slider::new(&mut current_max, full_min..=full_max);
            if is_int {
                slider = slider.step_by(1.0);
            }
            ui.add(slider);
        });

        if current_min > current_max {
            current_min = current_max;
        }
        if current_max < current_min {
            current_max = current_min;
        }

        state.pending_ranges[i] = full_range.with_min_f32(current_min);
        state.pending_ranges[i] = state.pending_ranges[i].with_max_f32(current_max);

        if i < param_defs.len() - 1 {
            ui.separator();
        }
    }

    ui.add_space(12.0);
    if ui.button("Confirm").clicked() {
        state.needs_regenerate = true;
    }
}

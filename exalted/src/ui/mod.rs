//! egui-based desktop editor for `Character`.
//!
//! `launch` opens a native window backed by `eframe::run_native`. The CLI
//! invokes this from `main` when no subcommand is given.

mod app;
mod dialogs;
pub mod io;
mod menu;
mod pickers;
mod sections;
mod sidebar;
mod state;
mod widgets;

pub use state::StartupAction;

use app::App;

pub fn launch(start: StartupAction) -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Exalted")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Exalted",
        native_options,
        Box::new(move |_cc| Ok(Box::new(App::new(start)))),
    )
}

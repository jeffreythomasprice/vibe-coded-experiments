//! Validation panel body. Live validation report; errors in red, notes in
//! yellow. Recomputes from `Character::validate_chargen()` + `validate_xp()`
//! only when `state.validation_dirty` is set (after a mutating edit), or on
//! demand via the "Re-validate" button.
//!
//! The tab strip and close button are owned by `sidebar::panel`; this
//! function only renders the body content.

use crate::error::{ValidationError, ValidationReport};
use crate::ui::state::AppState;

/// Renders the validation body into the current `Ui` (no header).
pub fn render_body(ui: &mut egui::Ui, state: &mut AppState) {
    if state.validation_dirty {
        let mut report = ValidationReport::new();
        report.extend(state.character.validate_chargen());
        report.extend(state.character.validate_xp());
        state.last_validation = report;
        state.validation_dirty = false;
    }

    ui.horizontal(|ui| {
        ui.colored_label(
            error_color(),
            format!("errors: {}", state.last_validation.errors.len()),
        );
        ui.colored_label(
            note_color(),
            format!("notes: {}", state.last_validation.notes.len()),
        );
        if ui.button("Re-validate").clicked() {
            state.validation_dirty = true;
        }
    });

    list_section(
        ui,
        "Errors",
        error_color(),
        &state.last_validation.errors,
        true,
    );
    list_section(
        ui,
        "Notes",
        note_color(),
        &state.last_validation.notes,
        false,
    );
}

fn list_section(
    ui: &mut egui::Ui,
    title: &str,
    color: egui::Color32,
    items: &[ValidationError],
    default_open: bool,
) {
    let header = format!("{} ({})", title, items.len());
    egui::CollapsingHeader::new(header)
        .id_salt(("validation", title))
        .default_open(default_open && !items.is_empty())
        .show(ui, |ui| {
            if items.is_empty() {
                ui.label("(none)");
            }
            for err in items {
                ui.colored_label(color, format!("• {}", err));
            }
        });
}

fn error_color() -> egui::Color32 {
    egui::Color32::from_rgb(230, 110, 110)
}

fn note_color() -> egui::Color32 {
    egui::Color32::from_rgb(220, 180, 80)
}

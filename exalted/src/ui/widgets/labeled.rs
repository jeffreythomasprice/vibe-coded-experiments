//! Tiny convenience widgets for "label: <input>" rows.

/// A single-line text edit with a left-side label. Returns true if the user
/// changed the contents this frame.
pub fn labeled_text_edit(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([120.0, 0.0], egui::Label::new(label));
        let resp = ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
        if resp.changed() {
            changed = true;
        }
    });
    changed
}

/// A multi-line text edit with a label above it.
pub fn labeled_text_area(ui: &mut egui::Ui, label: &str, value: &mut String, rows: usize) -> bool {
    let mut changed = false;
    ui.label(label);
    let resp = ui.add(
        egui::TextEdit::multiline(value)
            .desired_width(f32::INFINITY)
            .desired_rows(rows),
    );
    if resp.changed() {
        changed = true;
    }
    changed
}

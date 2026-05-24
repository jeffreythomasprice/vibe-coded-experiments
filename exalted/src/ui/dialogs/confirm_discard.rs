//! Confirm-discard modal. Shown when the user tries to leave a dirty
//! document (File → New, File → Open, Quit) so they can save first or
//! deliberately throw the changes away.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DiscardChoice {
    Save,
    Discard,
    Cancel,
}

/// Render the modal. Returns `Some(choice)` once the user picks a button;
/// `None` while still showing.
pub fn show(ctx: &egui::Context, file_label: &str) -> Option<DiscardChoice> {
    let mut choice = None;
    egui::Window::new("Unsaved changes")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(format!(
                "{} has unsaved changes. Save before closing?",
                file_label
            ));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    choice = Some(DiscardChoice::Save);
                }
                if ui.button("Discard").clicked() {
                    choice = Some(DiscardChoice::Discard);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(DiscardChoice::Cancel);
                }
            });
        });
    choice
}

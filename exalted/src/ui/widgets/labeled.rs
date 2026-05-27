//! Tiny convenience widgets for "label: <input>" rows.

use crate::ui::search::{
    self, HighlightKind, MatchTarget, SearchState, TextAreaOpts, TextEditOpts,
};

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

/// Search-aware variant of [`labeled_text_edit`]. The label is shown
/// unchanged; the TextEdit highlights matches when `target` is set on the
/// active search.
pub fn labeled_text_edit_search(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    target: MatchTarget,
    search: &SearchState,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([120.0, 0.0], egui::Label::new(label));
        let highlight: Option<HighlightKind> = search.highlight_for(target);
        let resp = search::highlighted_singleline(
            ui,
            value,
            &search.query,
            highlight,
            TextEditOpts {
                desired_width: f32::INFINITY,
                hint: None,
            },
            search.scroll_pending,
        );
        if resp.changed() {
            changed = true;
        }
    });
    changed
}

/// Search-aware variant of [`labeled_text_area`].
pub fn labeled_text_area_search(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    rows: usize,
    target: MatchTarget,
    search: &SearchState,
) -> bool {
    ui.label(label);
    let highlight: Option<HighlightKind> = search.highlight_for(target);
    let resp = search::highlighted_multiline(
        ui,
        value,
        &search.query,
        highlight,
        TextAreaOpts {
            desired_width: f32::INFINITY,
            desired_rows: rows,
            hint: None,
        },
        search.scroll_pending,
    );
    resp.changed()
}

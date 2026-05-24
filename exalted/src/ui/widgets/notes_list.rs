//! Edit a `Vec<Note>`. Used by character-level notes, charm notes, spell
//! notes, background notes, and combo notes — same widget everywhere so the
//! interaction model is consistent.

use chrono::Utc;

use crate::character::Note;

/// Render an editable list of notes. Returns true if anything changed
/// (added, edited body, deleted). Adding a note also returns true.
pub fn notes_editor(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash,
    notes: &mut Vec<Note>,
) -> bool {
    let mut changed = false;
    let mut delete_index: Option<usize> = None;

    let salt = egui::Id::new(id_source);

    for (idx, note) in notes.iter_mut().enumerate() {
        let header_label = format!(
            "{} — {}",
            note.created_at.format("%Y-%m-%d %H:%M"),
            preview(&note.body),
        );
        egui::CollapsingHeader::new(header_label)
            .id_salt(salt.with(idx))
            .show(ui, |ui| {
                let prev_body = note.body.clone();
                let resp = ui.add(
                    egui::TextEdit::multiline(&mut note.body)
                        .desired_width(f32::INFINITY)
                        .desired_rows(3),
                );
                if resp.changed() && note.body != prev_body {
                    note.updated_at = Utc::now();
                    changed = true;
                }
                ui.horizontal(|ui| {
                    ui.small(format!(
                        "updated {}",
                        note.updated_at.format("%Y-%m-%d %H:%M")
                    ));
                    if ui.button("Delete").clicked() {
                        delete_index = Some(idx);
                    }
                });
            });
    }

    if let Some(idx) = delete_index {
        notes.remove(idx);
        changed = true;
    }

    ui.horizontal(|ui| {
        if ui.button("+ Add note").clicked() {
            notes.push(Note::new(""));
            changed = true;
        }
    });

    changed
}

fn preview(body: &str) -> String {
    let first_line = body.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        "(empty)".to_string()
    } else if first_line.chars().count() > 60 {
        let truncated: String = first_line.chars().take(57).collect();
        format!("{}…", truncated)
    } else {
        first_line.to_string()
    }
}

//! Generic ComboBox over a small enumeration. The caller supplies the
//! `(value, label)` pairs and an id source.

pub fn enum_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash,
    current: &mut T,
    options: &[(T, &str)],
) -> bool {
    let mut changed = false;
    let selected_label = options
        .iter()
        .find(|(v, _)| *v == *current)
        .map(|(_, l)| *l)
        .unwrap_or("?");
    egui::ComboBox::from_id_salt(id_source)
        .selected_text(selected_label)
        .show_ui(ui, |ui| {
            for (value, label) in options {
                if ui
                    .selectable_label(*value == *current, *label)
                    .clicked()
                    && *value != *current
                {
                    *current = *value;
                    changed = true;
                }
            }
        });
    changed
}

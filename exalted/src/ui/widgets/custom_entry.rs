//! Editors for the `BackgroundEntry`, `CharmEntry`, and `SpellEntry`
//! structs from the rules database. Shared between the "Custom…" path in
//! the pickers and the "Edit details…" modal opened from each section.
//!
//! Each `*_entry_form` function renders one tall form mutating `entry` in
//! place; the caller wraps it in a window and/or scroll area as desired.
//! `id_salt` namespaces all internal ComboBox / DragValue ids so the same
//! form can render twice in one frame (picker + edit modal) without colliding.

use std::collections::BTreeMap;

use crate::character::{AttributeKind, BackgroundKind, SpellCircle};
use crate::render::names::{ability_name, attr_name, spell_circle_label};
use crate::rules::database::{BackgroundEntry, CharmEntry, CharmType, SpellEntry};
use crate::ui::widgets::icon_button::trash_button;

const EXALT_TYPES: &[&str] = &[
    "solar",
    "lunar",
    "sidereal",
    "terrestrial",
    "abyssal",
    "infernal",
    "alchemical",
    "(any)",
];

const CHARM_TYPES: &[CharmType] = &[
    CharmType::Reflexive,
    CharmType::Supplemental,
    CharmType::Simple,
    CharmType::ExtraAction,
    CharmType::Permanent,
];

const ABILITY_ANY: &str = "(any)";

pub fn background_entry_form(
    ui: &mut egui::Ui,
    id_salt: &str,
    entry: &mut BackgroundEntry,
) -> bool {
    let mut changed = false;
    egui::Grid::new((id_salt, "bg-form"))
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Id");
            if ui
                .add(egui::TextEdit::singleline(&mut entry.id).desired_width(220.0))
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Name");
            if ui
                .add(egui::TextEdit::singleline(&mut entry.name).desired_width(260.0))
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Kind");
            egui::ComboBox::from_id_salt((id_salt, "bg-kind"))
                .selected_text(format!("{:?}", entry.kind))
                .show_ui(ui, |ui| {
                    for k in BackgroundKind::ALL {
                        if ui
                            .selectable_label(entry.kind == *k, format!("{:?}", k))
                            .clicked()
                            && entry.kind != *k
                        {
                            entry.kind = *k;
                            changed = true;
                        }
                    }
                });
            ui.end_row();

            ui.label("Source");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.source)
                        .desired_width(220.0)
                        .hint_text("e.g. Homebrew, Exalted 2E"),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Pages");
            if ui
                .add(egui::TextEdit::singleline(&mut entry.pages).desired_width(100.0))
                .changed()
            {
                changed = true;
            }
            ui.end_row();
        });

    ui.label("Description");
    if ui
        .add(
            egui::TextEdit::multiline(&mut entry.description)
                .desired_width(f32::INFINITY)
                .desired_rows(4),
        )
        .changed()
    {
        changed = true;
    }

    changed
}

pub fn charm_entry_form(ui: &mut egui::Ui, id_salt: &str, entry: &mut CharmEntry) -> bool {
    let mut changed = false;
    egui::Grid::new((id_salt, "charm-form"))
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Id");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.id)
                        .desired_width(260.0)
                        .hint_text("kebab-case-id"),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Name");
            if ui
                .add(egui::TextEdit::singleline(&mut entry.name).desired_width(320.0))
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Exalt type");
            if exalt_type_combo(ui, id_salt, &mut entry.exalt_type) {
                changed = true;
            }
            ui.end_row();

            ui.label("Ability");
            if ability_combo(ui, id_salt, &mut entry.ability) {
                changed = true;
            }
            ui.end_row();

            ui.label("Charm type");
            egui::ComboBox::from_id_salt((id_salt, "charm-ct"))
                .selected_text(entry.charm_type.display())
                .show_ui(ui, |ui| {
                    for ct in CHARM_TYPES {
                        if ui
                            .selectable_label(entry.charm_type == *ct, ct.display())
                            .clicked()
                            && entry.charm_type != *ct
                        {
                            entry.charm_type = *ct;
                            changed = true;
                        }
                    }
                });
            ui.end_row();

            ui.label("Type detail");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.type_detail)
                        .desired_width(320.0)
                        .hint_text("e.g. \"Step 2, defense\""),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Cost");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.cost)
                        .desired_width(220.0)
                        .hint_text("e.g. \"5m, 1wp\""),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Min ability");
            if ui
                .add(egui::DragValue::new(&mut entry.mins_ability).range(0u8..=5))
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Min essence");
            if ui
                .add(egui::DragValue::new(&mut entry.mins_essence).range(1u8..=10))
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Duration");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.duration)
                        .desired_width(220.0)
                        .hint_text("e.g. Instant, One scene"),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Source");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.source)
                        .desired_width(220.0)
                        .hint_text("e.g. Homebrew, Exalted 2E"),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Pages");
            if ui
                .add(egui::TextEdit::singleline(&mut entry.pages).desired_width(100.0))
                .changed()
            {
                changed = true;
            }
            ui.end_row();
        });

    ui.label("Keywords");
    if string_list_editor(ui, (id_salt, "kw"), &mut entry.keywords, "keyword") {
        changed = true;
    }

    ui.label("Prerequisites");
    if string_list_editor(
        ui,
        (id_salt, "prereq"),
        &mut entry.prerequisites,
        "prereq id (kebab-case)",
    ) {
        changed = true;
    }

    ui.label("Minimum attributes");
    if attribute_map_editor(ui, (id_salt, "minattr"), &mut entry.mins_attribute) {
        changed = true;
    }

    ui.label("Effect (one-line summary for the sheet)");
    if ui
        .add(
            egui::TextEdit::multiline(&mut entry.effect)
                .desired_width(f32::INFINITY)
                .desired_rows(2),
        )
        .changed()
    {
        changed = true;
    }

    ui.label("Description");
    if ui
        .add(
            egui::TextEdit::multiline(&mut entry.description)
                .desired_width(f32::INFINITY)
                .desired_rows(5),
        )
        .changed()
    {
        changed = true;
    }

    changed
}

pub fn spell_entry_form(ui: &mut egui::Ui, id_salt: &str, entry: &mut SpellEntry) -> bool {
    let mut changed = false;
    egui::Grid::new((id_salt, "spell-form"))
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label("Id");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.id)
                        .desired_width(260.0)
                        .hint_text("kebab-case-id"),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Name");
            if ui
                .add(egui::TextEdit::singleline(&mut entry.name).desired_width(320.0))
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Circle");
            egui::ComboBox::from_id_salt((id_salt, "spell-circle"))
                .selected_text(spell_circle_label(entry.circle))
                .show_ui(ui, |ui| {
                    for c in SpellCircle::ALL {
                        if ui
                            .selectable_label(entry.circle == *c, spell_circle_label(*c))
                            .clicked()
                            && entry.circle != *c
                        {
                            entry.circle = *c;
                            changed = true;
                        }
                    }
                });
            ui.end_row();

            ui.label("Cost");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.cost)
                        .desired_width(220.0)
                        .hint_text("e.g. 15m, 1wp"),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Duration");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.duration)
                        .desired_width(220.0)
                        .hint_text("e.g. Instant, One scene"),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Target");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.target)
                        .desired_width(320.0)
                        .hint_text("e.g. One creature"),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Source");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut entry.source)
                        .desired_width(220.0)
                        .hint_text("e.g. Homebrew, Books of Sorcery"),
                )
                .changed()
            {
                changed = true;
            }
            ui.end_row();

            ui.label("Pages");
            if ui
                .add(egui::TextEdit::singleline(&mut entry.pages).desired_width(100.0))
                .changed()
            {
                changed = true;
            }
            ui.end_row();
        });

    ui.label("Keywords");
    if string_list_editor(ui, (id_salt, "kw"), &mut entry.keywords, "keyword") {
        changed = true;
    }

    ui.label("Effect (one-line summary for the sheet)");
    if ui
        .add(
            egui::TextEdit::multiline(&mut entry.effect)
                .desired_width(f32::INFINITY)
                .desired_rows(2),
        )
        .changed()
    {
        changed = true;
    }

    ui.label("Description");
    if ui
        .add(
            egui::TextEdit::multiline(&mut entry.description)
                .desired_width(f32::INFINITY)
                .desired_rows(5),
        )
        .changed()
    {
        changed = true;
    }

    changed
}

fn exalt_type_combo(ui: &mut egui::Ui, id_salt: &str, value: &mut String) -> bool {
    let mut changed = false;
    let label = if value.is_empty() {
        "(unset)".to_string()
    } else {
        value.clone()
    };
    egui::ComboBox::from_id_salt((id_salt, "exalt-type"))
        .selected_text(label)
        .show_ui(ui, |ui| {
            for opt in EXALT_TYPES {
                if ui
                    .selectable_label(value == opt, *opt)
                    .clicked()
                    && value != opt
                {
                    *value = (*opt).to_string();
                    changed = true;
                }
            }
        });
    changed
}

fn ability_combo(ui: &mut egui::Ui, id_salt: &str, value: &mut String) -> bool {
    let mut changed = false;
    use crate::character::AbilityKind;
    let label = if value.is_empty() {
        "(unset)".to_string()
    } else {
        value.clone()
    };
    egui::ComboBox::from_id_salt((id_salt, "ability"))
        .selected_text(label)
        .show_ui(ui, |ui| {
            if ui.selectable_label(value == ABILITY_ANY, ABILITY_ANY).clicked()
                && value != ABILITY_ANY
            {
                *value = ABILITY_ANY.to_string();
                changed = true;
            }
            for a in AbilityKind::ALL {
                let name = ability_name(*a);
                if ui.selectable_label(value == name, name).clicked() && value != name {
                    *value = name.to_string();
                    changed = true;
                }
            }
        });
    changed
}

fn string_list_editor(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + Copy,
    items: &mut Vec<String>,
    placeholder: &str,
) -> bool {
    let mut changed = false;
    let mut remove: Option<usize> = None;
    for (i, item) in items.iter_mut().enumerate() {
        ui.push_id((id_salt, i), |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::TextEdit::singleline(item)
                            .desired_width(280.0)
                            .hint_text(placeholder),
                    )
                    .changed()
                {
                    changed = true;
                }
                if trash_button(ui).clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        items.remove(i);
        changed = true;
    }
    if ui.small_button("+ add").clicked() {
        items.push(String::new());
        changed = true;
    }
    changed
}

fn attribute_map_editor(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + Copy,
    map: &mut BTreeMap<AttributeKind, u8>,
) -> bool {
    let mut changed = false;
    let mut remove: Option<AttributeKind> = None;
    for (k, v) in map.iter_mut() {
        ui.push_id((id_salt, *k), |ui| {
            ui.horizontal(|ui| {
                ui.label(attr_name(*k));
                if ui
                    .add(egui::DragValue::new(v).range(1u8..=5))
                    .changed()
                {
                    changed = true;
                }
                if trash_button(ui).clicked() {
                    remove = Some(*k);
                }
            });
        });
    }
    if let Some(k) = remove {
        map.remove(&k);
        changed = true;
    }
    let unused: Vec<AttributeKind> = AttributeKind::ALL
        .iter()
        .copied()
        .filter(|a| !map.contains_key(a))
        .collect();
    if !unused.is_empty() {
        ui.horizontal(|ui| {
            ui.label("add");
            egui::ComboBox::from_id_salt((id_salt, "add-attr"))
                .selected_text("(attribute)")
                .show_ui(ui, |ui| {
                    for a in &unused {
                        if ui.selectable_label(false, attr_name(*a)).clicked() {
                            map.insert(*a, 1);
                            changed = true;
                        }
                    }
                });
        });
    }
    changed
}

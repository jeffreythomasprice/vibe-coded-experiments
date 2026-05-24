//! Equipment section: weapons, armor, possessions, artifacts. Each is a
//! flat editor over the underlying struct; no rules-database lookups.

use crate::character::equipment::DamageType;
use crate::character::{AbilityKind, Armor, Artifact, Possession, Weapon};
use crate::render::names::ability_name;
use crate::ui::state::AppState;
use crate::ui::widgets::icon_button::{trash_button, trash_button_with_label};

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Equipment");

    let mut any = false;

    weapons(ui, state, &mut any);
    ui.add_space(8.0);
    armor(ui, state, &mut any);
    ui.add_space(8.0);
    possessions(ui, state, &mut any);
    ui.add_space(8.0);
    artifacts(ui, state, &mut any);

    if any {
        state.mark_dirty();
    }
}

fn weapons(ui: &mut egui::Ui, state: &mut AppState, any: &mut bool) {
    ui.label(egui::RichText::new("Weapons").strong());
    if ui.button("+ Add weapon").clicked() {
        state.character.equipment.weapons.push(Weapon {
            name: String::new(),
            ability: AbilityKind::Melee,
            speed: 5,
            accuracy: 0,
            damage: 0,
            damage_type: DamageType::Lethal,
            defense: 0,
            rate: 2,
            range_yards: None,
            tags: Vec::new(),
            attunement_motes: None,
            artifact_name: None,
        });
        *any = true;
    }
    let mut delete: Option<usize> = None;
    for (i, w) in state.character.equipment.weapons.iter_mut().enumerate() {
        egui::CollapsingHeader::new(if w.name.is_empty() {
            format!("(unnamed weapon #{})", i + 1)
        } else {
            w.name.clone()
        })
        .id_salt(("weapon", i))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                if ui
                    .add(egui::TextEdit::singleline(&mut w.name).desired_width(200.0))
                    .changed()
                {
                    *any = true;
                }
                ui.label("Ability");
                let prev = w.ability;
                egui::ComboBox::from_id_salt(("weapon-ab", i))
                    .selected_text(ability_name(w.ability))
                    .show_ui(ui, |ui| {
                        for a in AbilityKind::ALL {
                            ui.selectable_value(&mut w.ability, *a, ability_name(*a));
                        }
                    });
                if w.ability != prev {
                    *any = true;
                }
                if trash_button_with_label(ui, "remove").clicked() {
                    delete = Some(i);
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::DragValue::new(&mut w.speed)
                            .range(-5i8..=20)
                            .prefix("spd "),
                    )
                    .changed()
                {
                    *any = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut w.accuracy)
                            .range(-5i8..=20)
                            .prefix("acc "),
                    )
                    .changed()
                {
                    *any = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut w.damage)
                            .range(-5i8..=20)
                            .prefix("dmg "),
                    )
                    .changed()
                {
                    *any = true;
                }
                let prev_dt = w.damage_type;
                egui::ComboBox::from_id_salt(("weapon-dt", i))
                    .selected_text(damage_label(w.damage_type))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut w.damage_type, DamageType::Bashing, "Bashing");
                        ui.selectable_value(&mut w.damage_type, DamageType::Lethal, "Lethal");
                        ui.selectable_value(
                            &mut w.damage_type,
                            DamageType::Aggravated,
                            "Aggravated",
                        );
                    });
                if w.damage_type != prev_dt {
                    *any = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut w.defense)
                            .range(-5i8..=20)
                            .prefix("def "),
                    )
                    .changed()
                {
                    *any = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut w.rate)
                            .range(0u8..=20)
                            .prefix("rate "),
                    )
                    .changed()
                {
                    *any = true;
                }
            });
            ui.horizontal(|ui| {
                let mut has_range = w.range_yards.is_some();
                if ui.checkbox(&mut has_range, "ranged").changed() {
                    w.range_yards = if has_range { Some(0) } else { None };
                    *any = true;
                }
                if let Some(r) = w.range_yards.as_mut() {
                    if ui
                        .add(egui::DragValue::new(r).range(0u32..=10_000).suffix("y"))
                        .changed()
                    {
                        *any = true;
                    }
                }
                let mut tags = w.tags.join(", ");
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut tags)
                        .desired_width(240.0)
                        .hint_text("tags (comma-separated)"),
                );
                if resp.changed() {
                    w.tags = tags
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    *any = true;
                }
            });
        });
    }
    if let Some(i) = delete {
        state.character.equipment.weapons.remove(i);
        *any = true;
    }
}

fn armor(ui: &mut egui::Ui, state: &mut AppState, any: &mut bool) {
    ui.label(egui::RichText::new("Armor").strong());
    if state.character.equipment.armor.is_none() {
        if ui.button("+ Set armor").clicked() {
            state.character.equipment.armor = Some(Armor {
                name: String::new(),
                soak_bashing: 0,
                soak_lethal: 0,
                soak_aggravated: 0,
                hardness_bashing: 0,
                hardness_lethal: 0,
                mobility_penalty: 0,
                fatigue: 0,
                attunement_motes: None,
                artifact_name: None,
            });
            *any = true;
        }
        return;
    }
    let mut remove_armor = false;
    if let Some(a) = state.character.equipment.armor.as_mut() {
        ui.horizontal(|ui| {
            ui.label("Name");
            if ui
                .add(egui::TextEdit::singleline(&mut a.name).desired_width(220.0))
                .changed()
            {
                *any = true;
            }
            if trash_button_with_label(ui, "remove armor").clicked() {
                remove_armor = true;
            }
        });
    }
    if remove_armor {
        state.character.equipment.armor = None;
        *any = true;
        return;
    }
    if let Some(a) = state.character.equipment.armor.as_mut() {
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::DragValue::new(&mut a.soak_bashing)
                        .range(0u8..=20)
                        .prefix("soakB "),
                )
                .changed()
            {
                *any = true;
            }
            if ui
                .add(
                    egui::DragValue::new(&mut a.soak_lethal)
                        .range(0u8..=20)
                        .prefix("soakL "),
                )
                .changed()
            {
                *any = true;
            }
            if ui
                .add(
                    egui::DragValue::new(&mut a.soak_aggravated)
                        .range(0u8..=20)
                        .prefix("soakA "),
                )
                .changed()
            {
                *any = true;
            }
            if ui
                .add(
                    egui::DragValue::new(&mut a.mobility_penalty)
                        .range(0u8..=10)
                        .prefix("mob -"),
                )
                .changed()
            {
                *any = true;
            }
            if ui
                .add(
                    egui::DragValue::new(&mut a.fatigue)
                        .range(0u8..=10)
                        .prefix("fat "),
                )
                .changed()
            {
                *any = true;
            }
        });
    }
}

fn possessions(ui: &mut egui::Ui, state: &mut AppState, any: &mut bool) {
    ui.label(egui::RichText::new("Other possessions").strong());
    if ui.button("+ Add possession").clicked() {
        state.character.equipment.other.push(Possession {
            name: String::new(),
            primary_location: String::new(),
            secondary_location: None,
        });
        *any = true;
    }
    let mut delete: Option<usize> = None;
    for (i, p) in state.character.equipment.other.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            if ui
                .add(
                    egui::TextEdit::singleline(&mut p.name)
                        .desired_width(220.0)
                        .hint_text("name"),
                )
                .changed()
            {
                *any = true;
            }
            if ui
                .add(
                    egui::TextEdit::singleline(&mut p.primary_location)
                        .desired_width(160.0)
                        .hint_text("primary location"),
                )
                .changed()
            {
                *any = true;
            }
            let mut has_second = p.secondary_location.is_some();
            if ui.checkbox(&mut has_second, "alt").changed() {
                p.secondary_location = if has_second {
                    Some(String::new())
                } else {
                    None
                };
                *any = true;
            }
            if let Some(s) = p.secondary_location.as_mut() {
                if ui
                    .add(
                        egui::TextEdit::singleline(s)
                            .desired_width(120.0)
                            .hint_text("alt loc"),
                    )
                    .changed()
                {
                    *any = true;
                }
            }
            if trash_button(ui).clicked() {
                delete = Some(i);
            }
        });
    }
    if let Some(i) = delete {
        state.character.equipment.other.remove(i);
        *any = true;
    }
}

fn artifacts(ui: &mut egui::Ui, state: &mut AppState, any: &mut bool) {
    ui.label(egui::RichText::new("Artifacts").strong());
    if ui.button("+ Add artifact").clicked() {
        state.character.equipment.artifacts.push(Artifact {
            name: String::new(),
            rating: 1,
            attunement_motes: 0,
            hearthstone_sockets: 0,
            socketed_hearthstones: Vec::new(),
            description: String::new(),
        });
        *any = true;
    }
    let mut delete: Option<usize> = None;
    for (i, a) in state.character.equipment.artifacts.iter_mut().enumerate() {
        egui::CollapsingHeader::new(if a.name.is_empty() {
            format!("(unnamed artifact #{})", i + 1)
        } else {
            format!("{} (★{})", a.name, a.rating)
        })
        .id_salt(("artifact", i))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .add(egui::TextEdit::singleline(&mut a.name).desired_width(240.0))
                    .changed()
                {
                    *any = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut a.rating)
                            .range(1u8..=5)
                            .prefix("★"),
                    )
                    .changed()
                {
                    *any = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut a.attunement_motes)
                            .range(0u8..=30)
                            .suffix("m"),
                    )
                    .changed()
                {
                    *any = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut a.hearthstone_sockets)
                            .range(0u8..=8)
                            .prefix("sockets "),
                    )
                    .changed()
                {
                    *any = true;
                }
                if trash_button(ui).clicked() {
                    delete = Some(i);
                }
            });
            let mut sockets = a.socketed_hearthstones.join(", ");
            if ui
                .add(
                    egui::TextEdit::singleline(&mut sockets)
                        .desired_width(f32::INFINITY)
                        .hint_text("socketed hearthstones (comma-separated names)"),
                )
                .changed()
            {
                a.socketed_hearthstones = sockets
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                *any = true;
            }
            ui.label("Description");
            if ui
                .add(
                    egui::TextEdit::multiline(&mut a.description)
                        .desired_width(f32::INFINITY)
                        .desired_rows(2),
                )
                .changed()
            {
                *any = true;
            }
        });
    }
    if let Some(i) = delete {
        state.character.equipment.artifacts.remove(i);
        *any = true;
    }
}

fn damage_label(d: DamageType) -> &'static str {
    match d {
        DamageType::Bashing => "B",
        DamageType::Lethal => "L",
        DamageType::Aggravated => "A",
    }
}

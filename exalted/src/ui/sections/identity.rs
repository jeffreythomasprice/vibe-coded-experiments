//! Identity section: name/concept/motivation/etc., Caste, Anima totem,
//! Appearance, Favored Abilities, Virtue Flaw.

use crate::character::{AbilityKind, Caste, VirtueFlaw, VirtueKind};
use crate::render::names::ability_name;
use crate::ui::state::AppState;
use crate::ui::widgets::enum_combo::enum_combo;
use crate::ui::widgets::labeled::{labeled_text_area, labeled_text_edit};

const CASTES: &[(Caste, &str)] = &[
    (Caste::Dawn, "Dawn"),
    (Caste::Zenith, "Zenith"),
    (Caste::Twilight, "Twilight"),
    (Caste::Night, "Night"),
    (Caste::Eclipse, "Eclipse"),
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("Identity");

    if labeled_text_edit(ui, "Name", &mut state.character.identity.name) {
        state.mark_dirty();
    }
    if labeled_text_edit(ui, "Player", &mut state.character.identity.player) {
        state.mark_dirty();
    }
    if labeled_text_edit(ui, "Chronicle", &mut state.character.identity.chronicle) {
        state.mark_dirty();
    }
    if labeled_text_edit(ui, "Concept", &mut state.character.identity.concept) {
        state.mark_dirty();
    }
    if labeled_text_edit(
        ui,
        "Motivation",
        &mut state.character.identity.motivation,
    ) {
        state.mark_dirty();
    }
    if labeled_text_edit(
        ui,
        "Personality",
        &mut state.character.identity.personality,
    ) {
        state.mark_dirty();
    }

    ui.add_space(6.0);

    // Caste.
    ui.horizontal(|ui| {
        ui.add_sized([120.0, 0.0], egui::Label::new("Caste"));
        if enum_combo(ui, "caste-combo", &mut state.character.caste, CASTES) {
            state.mark_dirty();
        }
    });

    // Anima totem.
    if labeled_text_edit(
        ui,
        "Anima totem",
        &mut state.character.identity.anima.totem,
    ) {
        state.mark_dirty();
    }

    // Favored Abilities.
    favored_abilities_editor(ui, state);

    // VirtueFlaw.
    virtue_flaw_editor(ui, state);

    // Appearance.
    appearance_editor(ui, state);
}

fn favored_abilities_editor(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(6.0);
    let count = state.character.favored_abilities.len();
    ui.label(format!("Favored Abilities ({}/5)", count));
    egui::CollapsingHeader::new("Pick favored abilities")
        .id_salt("favored-abilities-grid")
        .default_open(count != 5)
        .show(ui, |ui| {
            egui::Grid::new("favored-grid")
                .num_columns(5)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    for (i, ab) in AbilityKind::ALL.iter().enumerate() {
                        let mut checked = state.character.favored_abilities.contains(ab);
                        let resp = ui.checkbox(&mut checked, ability_name(*ab));
                        if resp.changed() {
                            let already = state.character.favored_abilities.contains(ab);
                            if checked && !already {
                                state.character.favored_abilities.push(*ab);
                                state.mark_dirty();
                            } else if !checked && already {
                                state.character.favored_abilities.retain(|x| x != ab);
                                state.mark_dirty();
                            }
                        }
                        if (i + 1) % 5 == 0 {
                            ui.end_row();
                        }
                    }
                });
        });
}

fn virtue_flaw_editor(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(6.0);
    ui.label("Virtue Flaw");

    // Variant labels and a synthetic "<unset>" + "Custom…" choice.
    #[derive(Copy, Clone, PartialEq, Eq)]
    enum Pick {
        Unset,
        Named(usize), // index into NAMED below
        Custom,
    }
    const NAMED: &[(&str, fn() -> VirtueFlaw)] = &[
        ("Compassionate Martyrdom", || VirtueFlaw::CompassionateMartyrdom),
        ("Heart of Tears", || VirtueFlaw::HeartOfTears),
        ("Red Rage of Compassion", || VirtueFlaw::RedRageOfCompassion),
        ("Deliberate Cruelty", || VirtueFlaw::DeliberateCruelty),
        ("Heart of Flint", || VirtueFlaw::HeartOfFlint),
        ("Ascetic Drive", || VirtueFlaw::AsceticDrive),
        ("Contempt of the Virtuous", || VirtueFlaw::ContemptOfTheVirtuous),
        ("Overindulgence", || VirtueFlaw::Overindulgence),
        ("Berserk Anger", || VirtueFlaw::BerserkAnger),
        ("Foolhardy Contempt", || VirtueFlaw::FoolhardyContempt),
    ];

    let current_pick = match &state.character.virtue_flaw {
        None => Pick::Unset,
        Some(VirtueFlaw::Custom { .. }) => Pick::Custom,
        Some(other) => {
            let mut found = Pick::Unset;
            for (i, (_, ctor)) in NAMED.iter().enumerate() {
                if std::mem::discriminant(&ctor()) == std::mem::discriminant(other) {
                    found = Pick::Named(i);
                    break;
                }
            }
            found
        }
    };
    let mut pick = current_pick;
    egui::ComboBox::from_id_salt("virtue-flaw-combo")
        .selected_text(match pick {
            Pick::Unset => "<unset>".to_string(),
            Pick::Named(i) => NAMED[i].0.to_string(),
            Pick::Custom => "Custom…".to_string(),
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut pick, Pick::Unset, "<unset>");
            for (i, (label, _)) in NAMED.iter().enumerate() {
                ui.selectable_value(&mut pick, Pick::Named(i), *label);
            }
            ui.selectable_value(&mut pick, Pick::Custom, "Custom…");
        });
    if pick != current_pick {
        state.character.virtue_flaw = match pick {
            Pick::Unset => None,
            Pick::Named(i) => Some((NAMED[i].1)()),
            Pick::Custom => Some(VirtueFlaw::Custom {
                name: String::new(),
                virtue: VirtueKind::Compassion,
            }),
        };
        state.mark_dirty();
    }

    if let Some(VirtueFlaw::Custom { name, virtue }) = &mut state.character.virtue_flaw {
        ui.horizontal(|ui| {
            ui.label("Name");
            let resp = ui.add(egui::TextEdit::singleline(name).desired_width(240.0));
            if resp.changed() {
                state.dirty = true;
                state.validation_dirty = true;
            }
            ui.label("Virtue");
            let mut v = *virtue;
            egui::ComboBox::from_id_salt("custom-flaw-virtue")
                .selected_text(virtue_label(v))
                .show_ui(ui, |ui| {
                    for &vk in VirtueKind::ALL {
                        ui.selectable_value(&mut v, vk, virtue_label(vk));
                    }
                });
            if v != *virtue {
                *virtue = v;
                state.dirty = true;
                state.validation_dirty = true;
            }
        });
    }
}

fn virtue_label(v: VirtueKind) -> &'static str {
    match v {
        VirtueKind::Compassion => "Compassion",
        VirtueKind::Conviction => "Conviction",
        VirtueKind::Temperance => "Temperance",
        VirtueKind::Valor => "Valor",
    }
}

fn appearance_editor(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(6.0);
    egui::CollapsingHeader::new("Appearance").default_open(false).show(ui, |ui| {
        let app = &mut state.character.identity.appearance;
        let mut any = false;
        any |= labeled_text_edit(ui, "Sex", &mut app.sex);
        ui.horizontal(|ui| {
            ui.add_sized([120.0, 0.0], egui::Label::new("Age"));
            let mut has_age = app.age.is_some();
            if ui.checkbox(&mut has_age, "").changed() {
                if has_age {
                    app.age = Some(app.age.unwrap_or(0));
                } else {
                    app.age = None;
                }
                any = true;
            }
            if let Some(age) = app.age.as_mut() {
                let resp = ui.add(egui::DragValue::new(age).range(0u32..=10_000));
                if resp.changed() {
                    any = true;
                }
            }
        });
        any |= labeled_text_edit(ui, "Hair", &mut app.hair);
        any |= labeled_text_edit(ui, "Eyes", &mut app.eyes);
        any |= labeled_text_edit(ui, "Skin", &mut app.skin);
        any |= labeled_text_edit(ui, "Homeland", &mut app.homeland);
        any |= labeled_text_area(
            ui,
            "Distinguishing features",
            &mut app.distinguishing_features,
            2,
        );
        if any {
            state.mark_dirty();
        }
    });
}

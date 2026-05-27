//! Identity section: name/concept/motivation/etc., Caste, Anima totem,
//! Appearance, Virtue Flaw.

use crate::character::{Caste, VirtueFlaw, VirtueKind};
use crate::ui::search::{self, IdentityField, MatchTarget, SectionId, TextEditOpts};
use crate::ui::state::AppState;
use crate::ui::widgets::enum_combo::enum_combo;
use crate::ui::widgets::labeled::{labeled_text_area_search, labeled_text_edit_search};

const CASTES: &[(Caste, &str)] = &[
    (Caste::Dawn, "Dawn"),
    (Caste::Zenith, "Zenith"),
    (Caste::Twilight, "Twilight"),
    (Caste::Night, "Night"),
    (Caste::Eclipse, "Eclipse"),
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let heading_hl = state
        .search
        .highlight_for(MatchTarget::SectionHeading(SectionId::Identity));
    search::highlight_heading(
        ui,
        SectionId::Identity.label(),
        heading_hl,
        state.search.scroll_pending,
    );

    if labeled_text_edit_search(
        ui,
        "Name",
        &mut state.character.identity.name,
        MatchTarget::Identity(IdentityField::Name),
        &state.search,
    ) {
        state.mark_dirty_with("identity.name");
    }
    if labeled_text_edit_search(
        ui,
        "Player",
        &mut state.character.identity.player,
        MatchTarget::Identity(IdentityField::Player),
        &state.search,
    ) {
        state.mark_dirty_with("identity.player");
    }
    if labeled_text_edit_search(
        ui,
        "Chronicle",
        &mut state.character.identity.chronicle,
        MatchTarget::Identity(IdentityField::Chronicle),
        &state.search,
    ) {
        state.mark_dirty_with("identity.chronicle");
    }
    if labeled_text_edit_search(
        ui,
        "Concept",
        &mut state.character.identity.concept,
        MatchTarget::Identity(IdentityField::Concept),
        &state.search,
    ) {
        state.mark_dirty_with("identity.concept");
    }
    if labeled_text_edit_search(
        ui,
        "Motivation",
        &mut state.character.identity.motivation,
        MatchTarget::Identity(IdentityField::Motivation),
        &state.search,
    ) {
        state.mark_dirty_with("identity.motivation");
    }
    if labeled_text_edit_search(
        ui,
        "Personality",
        &mut state.character.identity.personality,
        MatchTarget::Identity(IdentityField::Personality),
        &state.search,
    ) {
        state.mark_dirty_with("identity.personality");
    }

    ui.add_space(6.0);

    // Caste.
    ui.horizontal(|ui| {
        ui.add_sized([120.0, 0.0], egui::Label::new("Caste"));
        if enum_combo(ui, "caste-combo", &mut state.character.caste, CASTES) {
            state.mark_dirty_with("identity.caste");
        }
    });

    // Anima totem.
    if labeled_text_edit_search(
        ui,
        "Anima totem",
        &mut state.character.identity.anima.totem,
        MatchTarget::Identity(IdentityField::AnimaTotem),
        &state.search,
    ) {
        state.mark_dirty_with("identity.anima_totem");
    }

    // VirtueFlaw.
    virtue_flaw_editor(ui, state);

    // Appearance.
    appearance_editor(ui, state);
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
        ("Compassionate Martyrdom", || {
            VirtueFlaw::CompassionateMartyrdom
        }),
        ("Heart of Tears", || VirtueFlaw::HeartOfTears),
        ("Red Rage of Compassion", || VirtueFlaw::RedRageOfCompassion),
        ("Deliberate Cruelty", || VirtueFlaw::DeliberateCruelty),
        ("Heart of Flint", || VirtueFlaw::HeartOfFlint),
        ("Ascetic Drive", || VirtueFlaw::AsceticDrive),
        ("Contempt of the Virtuous", || {
            VirtueFlaw::ContemptOfTheVirtuous
        }),
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
        state.mark_dirty_with("identity.virtue_flaw.kind");
    }

    if let Some(VirtueFlaw::Custom { name, virtue }) = &mut state.character.virtue_flaw {
        ui.horizontal(|ui| {
            ui.label("Name");
            let hl = state
                .search
                .highlight_for(MatchTarget::Identity(IdentityField::VirtueFlawName));
            let resp = search::highlighted_singleline(
                ui,
                name,
                &state.search.query,
                hl,
                TextEditOpts {
                    desired_width: 240.0,
                    hint: None,
                },
                state.search.scroll_pending,
            );
            if resp.changed() {
                tracing::trace!(
                    field = "identity.virtue_flaw.name",
                    "document field updated"
                );
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
                tracing::trace!(
                    field = "identity.virtue_flaw.virtue",
                    "document field updated"
                );
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
    egui::CollapsingHeader::new("Appearance")
        .default_open(false)
        .show(ui, |ui| {
            let search = &state.search;
            let app = &mut state.character.identity.appearance;
            let mut any = false;
            any |= labeled_text_edit_search(
                ui,
                "Sex",
                &mut app.sex,
                MatchTarget::Identity(IdentityField::AppearanceSex),
                search,
            );
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
            any |= labeled_text_edit_search(
                ui,
                "Hair",
                &mut app.hair,
                MatchTarget::Identity(IdentityField::AppearanceHair),
                search,
            );
            any |= labeled_text_edit_search(
                ui,
                "Eyes",
                &mut app.eyes,
                MatchTarget::Identity(IdentityField::AppearanceEyes),
                search,
            );
            any |= labeled_text_edit_search(
                ui,
                "Skin",
                &mut app.skin,
                MatchTarget::Identity(IdentityField::AppearanceSkin),
                search,
            );
            any |= labeled_text_edit_search(
                ui,
                "Homeland",
                &mut app.homeland,
                MatchTarget::Identity(IdentityField::AppearanceHomeland),
                search,
            );
            any |= labeled_text_area_search(
                ui,
                "Distinguishing features",
                &mut app.distinguishing_features,
                2,
                MatchTarget::Identity(IdentityField::AppearanceFeatures),
                search,
            );
            if any {
                state.mark_dirty_with("identity.appearance");
            }
        });
}

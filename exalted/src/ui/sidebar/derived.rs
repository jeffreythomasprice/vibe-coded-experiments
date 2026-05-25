//! Derived-values panel body. Read-only summary of the things the rules
//! engine computes from the current `Character`: DVs, soak, essence pools,
//! health track, movement, and a few combat thresholds. Recomputed every
//! frame (cheap; nothing here allocates significantly).
//!
//! The tab strip and close button are owned by `sidebar::panel`; this
//! function only renders the body content.

use crate::character::Character;
use crate::rules::{defense, derived, essence, health};

/// Renders the derived-values body into the current `Ui` (no header).
pub fn render_body(ui: &mut egui::Ui, character: &Character) {
    section(ui, "Essence", |ui| {
        let pmax = essence::personal_essence_max(character);
        let pavail = essence::essence_personal_available(character);
        let pmax2 = essence::peripheral_essence_max(character);
        let pavail2 = essence::essence_peripheral_available(character);
        ui.label(format!("Personal:    {:>3} / {:>3}", pavail.max(0), pmax));
        ui.label(format!("Peripheral:  {:>3} / {:>3}", pavail2.max(0), pmax2));
    });

    section(ui, "Willpower", |ui| {
        let perm = character.willpower.dots();
        let temp = character.pool_state.willpower_temporary;
        let spent = character.pool_state.willpower_permanent_spent;
        ui.label(format!("Permanent:   {}", perm));
        ui.label(format!("Spent:       {}", spent));
        ui.label(format!("Temporary:   {}", temp));
        let wp_from_v = defense::willpower_from_virtues(character);
        if wp_from_v != perm {
            ui.colored_label(
                egui::Color32::from_rgb(220, 180, 80),
                format!("(virtues sum to {})", wp_from_v),
            );
        }
    });

    section(ui, "Defense values", |ui| {
        ui.label(format!("Dodge DV:    {}", defense::dodge_dv(character)));
        if let Some(weapon) = character.equipment.weapons.first() {
            ui.label(format!(
                "Parry DV ({}): {}",
                weapon.name,
                defense::parry_dv(character, weapon),
            ));
        }
        ui.label(format!("MDV (dodge): {}", defense::mdv_dodge(character)));
    });

    section(ui, "Soak", |ui| {
        ui.label(format!("Bashing:     {}", defense::soak_bashing(character)));
        ui.label(format!("Lethal:      {}", defense::soak_lethal(character)));
        ui.label(format!(
            "Aggravated:  {}",
            defense::soak_aggravated(character)
        ));
    });

    section(ui, "Combat misc", |ui| {
        ui.label(format!("Join Battle: {}", defense::join_battle(character)));
        ui.label(format!("Wound pen.:  {}", health::wound_penalty(character)));
        if health::is_incapacitated(character) {
            ui.colored_label(egui::Color32::from_rgb(255, 90, 90), "INCAPACITATED");
        }
    });

    section(ui, "Health track", |ui| {
        let track = health::health_track(character);
        let damage = &character.pool_state.health_damage;
        let total_damage = damage.total() as usize;
        for (i, level) in track.iter().enumerate() {
            let filled = i < total_damage;
            let marker = if filled { "✕" } else { "·" };
            ui.label(format!(
                "[{}] {:<6} ({:+})",
                marker, level.label, level.penalty
            ));
        }
    });

    section(ui, "Movement", |ui| {
        let m = derived::movement(character);
        ui.label(format!("Move: {}y/tick", m.move_));
        ui.label(format!("Dash: {}y/tick", m.dash));
        ui.label(format!(
            "Jump: {}y vert / {}y horiz",
            m.jump_vertical, m.jump_horizontal
        ));
    });

    section(ui, "Knockdown / Stunning", |ui| {
        let k = derived::knockdown(character);
        let s = derived::stunning(character);
        ui.label(format!(
            "Knockdown ≥ {} → resist {}d",
            k.threshold, k.resist_pool
        ));
        ui.label(format!(
            "Stunning ≥ {} → resist {}d",
            s.threshold, s.resist_pool
        ));
    });

    section(ui, "Feats", |ui| {
        ui.label(format!("Lift: {} lbs", derived::lift_lbs(character)));
    });
}

fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title)
        .id_salt(("derived", title))
        .default_open(true)
        .show(ui, body);
}

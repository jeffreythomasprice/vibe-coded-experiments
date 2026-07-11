//! Per-section render functions called in order from `App::ui`. Each
//! section corresponds to one anchor in the long vertical scroll; the order
//! mirrors `render::markdown::character_to_markdown` so the editor and the
//! canonical sheet view agree on layout.

use crate::ui::state::AppState;

pub mod abilities;
pub mod arts;
pub mod attributes;
pub mod backgrounds;
pub mod charms;
pub mod combos;
pub mod equipment;
pub mod familiar;
pub mod hearthstones;
pub mod identity;
pub mod intimacies;
pub mod languages;
pub mod notes;
pub mod pools;
pub mod spells;
pub mod virtues;
pub mod willpower_essence;
pub mod xp;

pub fn render_all(ui: &mut egui::Ui, state: &mut AppState) {
    identity::render(ui, state);
    section_break(ui);
    attributes::render(ui, state);
    section_break(ui);
    abilities::render(ui, state);
    section_break(ui);
    virtues::render(ui, state);
    section_break(ui);
    willpower_essence::render(ui, state);
    section_break(ui);
    charms::render(ui, state);
    section_break(ui);
    combos::render(ui, state);
    section_break(ui);
    spells::render(ui, state);
    section_break(ui);
    arts::render(ui, state);
    section_break(ui);
    backgrounds::render(ui, state);
    section_break(ui);
    intimacies::render(ui, state);
    section_break(ui);
    familiar::render(ui, state);
    section_break(ui);
    equipment::render(ui, state);
    section_break(ui);
    hearthstones::render(ui, state);
    section_break(ui);
    languages::render(ui, state);
    section_break(ui);
    pools::render(ui, state);
    section_break(ui);
    xp::render(ui, state);
    section_break(ui);
    notes::render(ui, state);
}

fn section_break(ui: &mut egui::Ui) {
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(12.0);
}

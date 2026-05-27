//! Combos section: named bundles of owned Charms with a source (BP or XP).

use crate::character::{Combo, DotSource};
use crate::rules::database::database;
use crate::ui::search::{self, MatchTarget, NoteParent, SectionId, TextEditOpts};
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::{DotSourceKind, dot_source_editor};
use crate::ui::widgets::icon_button::trash_button_with_label;
use crate::ui::widgets::notes_list::notes_editor;

const COMBO_SOURCES: &[DotSourceKind] = &[DotSourceKind::BonusPoints, DotSourceKind::Xp];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.combos.len();
    let heading_hl = state
        .search
        .highlight_for(MatchTarget::SectionHeading(SectionId::Combos));
    search::highlight_heading(
        ui,
        &format!("{} ({})", SectionId::Combos.label(), count),
        heading_hl,
        state.search.scroll_pending,
    );

    if ui.button("+ Add combo").clicked() {
        state.character.combos.push(Combo {
            name: String::new(),
            charm_ids: Vec::new(),
            source: DotSource::BonusPoints { spent: 0 },
            notes: Vec::new(),
        });
        state.mark_dirty_with("combos.add");
    }

    let owned_charm_ids: Vec<(String, String)> = state
        .character
        .charms
        .iter()
        .map(|c| {
            let id = c.id().to_string();
            let name = c.display_name(database()).to_string();
            (id, name)
        })
        .collect();

    let mut any_changed = false;
    let mut delete_idx: Option<usize> = None;
    for (i, combo) in state.character.combos.iter_mut().enumerate() {
        let header = if combo.name.is_empty() {
            format!("(unnamed combo #{})", i + 1)
        } else {
            combo.name.clone()
        };
        let force_open = state.search.focused_within(|t| match t {
            MatchTarget::Combo { idx } => *idx == i,
            MatchTarget::Note {
                parent: NoteParent::Combo(p),
                ..
            } => *p == i,
            _ => false,
        });
        let mut header_widget = egui::CollapsingHeader::new(header)
            .id_salt(("combo", i))
            .default_open(false);
        if force_open {
            header_widget = header_widget.open(Some(true));
        }
        header_widget.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                let target = MatchTarget::Combo { idx: i };
                let highlight = state.search.highlight_for(target);
                let resp = search::highlighted_singleline(
                    ui,
                    &mut combo.name,
                    &state.search.query,
                    highlight,
                    TextEditOpts {
                        desired_width: 240.0,
                        hint: None,
                    },
                    state.search.scroll_pending,
                );
                if resp.changed() {
                    any_changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Source");
                if dot_source_editor(ui, ("combo-src", i), &mut combo.source, COMBO_SOURCES) {
                    any_changed = true;
                }
                if trash_button_with_label(ui, "remove combo").clicked() {
                    delete_idx = Some(i);
                }
            });
            ui.label("Member charms");
            let mut remove_member: Option<usize> = None;
            for (mi, id) in combo.charm_ids.iter().enumerate() {
                let label = owned_charm_ids
                    .iter()
                    .find(|(oid, _)| oid == id)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_else(|| format!("{} (not owned)", id));
                ui.horizontal(|ui| {
                    ui.label(format!("• {}", label));
                    if ui.small_button("remove").clicked() {
                        remove_member = Some(mi);
                    }
                });
            }
            if let Some(mi) = remove_member {
                combo.charm_ids.remove(mi);
                any_changed = true;
            }
            ui.horizontal(|ui| {
                ui.label("Add charm");
                egui::ComboBox::from_id_salt(("combo-add", i))
                    .selected_text("(pick one)")
                    .show_ui(ui, |ui| {
                        for (id, name) in &owned_charm_ids {
                            if combo.charm_ids.contains(id) {
                                continue;
                            }
                            if ui.selectable_label(false, name).clicked() {
                                combo.charm_ids.push(id.clone());
                                any_changed = true;
                            }
                        }
                    });
            });

            ui.add_space(4.0);
            ui.label("Notes");
            if notes_editor(
                ui,
                ("combo-notes", i),
                &mut combo.notes,
                NoteParent::Combo(i),
                &state.search,
            ) {
                any_changed = true;
            }
        });
    }
    if let Some(i) = delete_idx {
        state.character.combos.remove(i);
        any_changed = true;
    }
    if any_changed {
        state.mark_dirty_with("combos.edit");
    }
}

//! Hearthstones section: Vec<Hearthstone> editor.

use crate::character::Hearthstone;
use crate::ui::search::{self, HsField, MatchTarget, SectionId, TextAreaOpts, TextEditOpts};
use crate::ui::state::AppState;
use crate::ui::widgets::icon_button::trash_button;

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.hearthstones.len();
    let heading_hl = state
        .search
        .highlight_for(MatchTarget::SectionHeading(SectionId::Hearthstones));
    search::highlight_heading(
        ui,
        &format!("{} ({})", SectionId::Hearthstones.label(), count),
        heading_hl,
        state.search.scroll_pending,
    );

    if ui.button("+ Add hearthstone").clicked() {
        state.character.hearthstones.push(Hearthstone {
            name: String::new(),
            level: 1,
            aspect: String::new(),
            description: String::new(),
        });
        state.mark_dirty_with("hearthstones.add");
    }

    let mut any = false;
    let mut delete_idx: Option<usize> = None;
    for (i, hs) in state.character.hearthstones.iter_mut().enumerate() {
        let force_open = state.search.focused_within(|t| match t {
            MatchTarget::Hearthstone { idx, .. } => *idx == i,
            _ => false,
        });
        let mut header_widget = egui::CollapsingHeader::new(if hs.name.is_empty() {
            format!("(unnamed hearthstone #{})", i + 1)
        } else {
            format!("{} ({}★)", hs.name, hs.level)
        })
        .id_salt(("hs", i));
        if force_open {
            header_widget = header_widget.open(Some(true));
        }
        header_widget.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Name");
                let hl = state.search.highlight_for(MatchTarget::Hearthstone {
                    idx: i,
                    field: HsField::Name,
                });
                let resp = search::highlighted_singleline(
                    ui,
                    &mut hs.name,
                    &state.search.query,
                    hl,
                    TextEditOpts {
                        desired_width: 240.0,
                        hint: None,
                    },
                    state.search.scroll_pending,
                );
                if resp.changed() {
                    any = true;
                }
                ui.label("Level");
                let resp = ui.add(egui::DragValue::new(&mut hs.level).range(1u8..=5));
                if resp.changed() {
                    any = true;
                }
                ui.label("Aspect");
                let hl = state.search.highlight_for(MatchTarget::Hearthstone {
                    idx: i,
                    field: HsField::Aspect,
                });
                let resp = search::highlighted_singleline(
                    ui,
                    &mut hs.aspect,
                    &state.search.query,
                    hl,
                    TextEditOpts {
                        desired_width: 140.0,
                        hint: None,
                    },
                    state.search.scroll_pending,
                );
                if resp.changed() {
                    any = true;
                }
                if trash_button(ui).clicked() {
                    delete_idx = Some(i);
                }
            });
            ui.label("Description");
            let hl = state.search.highlight_for(MatchTarget::Hearthstone {
                idx: i,
                field: HsField::Description,
            });
            let resp = search::highlighted_multiline(
                ui,
                &mut hs.description,
                &state.search.query,
                hl,
                TextAreaOpts {
                    desired_width: f32::INFINITY,
                    desired_rows: 2,
                    hint: None,
                },
                state.search.scroll_pending,
            );
            if resp.changed() {
                any = true;
            }
        });
    }
    if let Some(i) = delete_idx {
        state.character.hearthstones.remove(i);
        any = true;
    }
    if any {
        state.mark_dirty_with("hearthstones.edit");
    }
}

//! Backgrounds section: list each `BackgroundRef`, edit its rated trait,
//! label (for disambiguation), and notes; add new entries via the picker.
//! `Custom` entries also get a read-only summary of their inline entry data
//! and an "Edit details…" button that opens the entry editor.

use crate::character::{BackgroundRef, DotSource};
use crate::rules::database::{BackgroundEntry, database};
use crate::ui::pickers::background_picker::BackgroundPickerState;
use crate::ui::search::{self, BgField, MatchTarget, NoteParent, SectionId, TextEditOpts};
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::DotSourceKind;
use crate::ui::widgets::icon_button::trash_button_with_label;
use crate::ui::widgets::notes_list::notes_editor;
use crate::ui::widgets::rated_trait::{RatedTraitOpts, rated_trait_editor};

const BG_TRAIT_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.backgrounds.len();
    let heading_hl = state
        .search
        .highlight_for(MatchTarget::SectionHeading(SectionId::Backgrounds));
    search::highlight_heading(
        ui,
        &format!("{} ({})", SectionId::Backgrounds.label(), count),
        heading_hl,
        state.search.scroll_pending,
    );

    if ui.button("+ Add background…").clicked() {
        tracing::debug!(picker = "background", "opened picker");
        state.background_picker = Some(BackgroundPickerState::new());
    }

    let db = database();
    let default_source = if state.character.is_in_chargen() {
        DotSource::ChargenPriority
    } else {
        DotSource::Xp { spent: 0 }
    };
    let mut any_changed = false;
    let mut delete_idx: Option<usize> = None;
    let mut start_edit: Option<(usize, BackgroundEntry)> = None;
    for (i, bg) in state.character.backgrounds.iter_mut().enumerate() {
        let name = bg.display_name(db).to_string();
        let label = bg.label().to_string();
        let header = if label.is_empty() {
            name.clone()
        } else {
            format!("{} — {}", name, label)
        };
        let custom_snapshot: Option<BackgroundEntry> = match bg {
            BackgroundRef::Custom { entry, .. } => Some(entry.clone()),
            _ => None,
        };
        let force_open = state.search.focused_within(|t| match t {
            MatchTarget::Background { idx, .. } => *idx == i,
            MatchTarget::Note {
                parent: NoteParent::Background(p),
                ..
            } => *p == i,
            _ => false,
        });
        let mut header_widget = egui::CollapsingHeader::new(header)
            .id_salt(("bg", i))
            .default_open(false);
        if force_open {
            header_widget = header_widget.open(Some(true));
        }
        header_widget.show(ui, |ui| {
            let (label, trait_, notes) = match bg {
                BackgroundRef::Lookup {
                    label,
                    trait_,
                    notes,
                    ..
                } => (label, trait_, notes),
                BackgroundRef::Custom {
                    label,
                    trait_,
                    notes,
                    ..
                } => (label, trait_, notes),
            };
            {
                ui.horizontal(|ui| {
                    ui.label("Label");
                    let label_target = MatchTarget::Background {
                        idx: i,
                        field: BgField::Label,
                    };
                    let highlight = state.search.highlight_for(label_target);
                    let resp = search::highlighted_singleline(
                        ui,
                        label,
                        &state.search.query,
                        highlight,
                        TextEditOpts {
                            desired_width: 220.0,
                            hint: Some("e.g. \"Realm\" or specific artifact name"),
                        },
                        state.search.scroll_pending,
                    );
                    if resp.changed() {
                        any_changed = true;
                    }
                    if trash_button_with_label(ui, "remove").clicked() {
                        delete_idx = Some(i);
                    }
                });

                let mut opts = RatedTraitOpts {
                    label: &name,
                    max_dots: 5,
                    allowed_sources: BG_TRAIT_SOURCES,
                    default_add_source: default_source,
                    show_specialties: false,
                    selectable: None,
                    search: Some(&state.search),
                    label_target: Some(MatchTarget::Background {
                        idx: i,
                        field: BgField::CustomName,
                    }),
                    specialty_ability: None,
                };
                if rated_trait_editor(ui, ("bg-trait", i), trait_, &mut opts) {
                    any_changed = true;
                }

                ui.add_space(4.0);
                ui.label("Notes");
                if notes_editor(
                    ui,
                    ("bg-notes", i),
                    notes,
                    NoteParent::Background(i),
                    &state.search,
                ) {
                    any_changed = true;
                }
            }

            if let Some(entry) = &custom_snapshot {
                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Custom background").strong());
                    if ui.small_button("Edit details…").clicked() {
                        start_edit = Some((i, entry.clone()));
                    }
                });
                ui.small(format!("kind: {:?}", entry.kind));
                if !entry.source.is_empty() || !entry.pages.is_empty() {
                    ui.small(format!("source: {}   pages: {}", entry.source, entry.pages));
                }
                if !entry.description.is_empty() {
                    ui.small(&entry.description);
                }
            }
        });
    }
    if let Some(i) = delete_idx {
        state.character.backgrounds.remove(i);
        any_changed = true;
    }
    if let Some((idx, _)) = &start_edit {
        tracing::debug!(
            kind = "background",
            index = idx,
            "started custom entry edit"
        );
    }
    if let Some(payload) = start_edit {
        state.editing_custom_background = Some(payload);
    }
    if any_changed {
        state.mark_dirty_with("backgrounds.edit");
    }
}

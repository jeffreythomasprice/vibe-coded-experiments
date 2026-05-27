//! Spells section: list known spells (grouped by circle), edit source / notes,
//! add new spells via the picker. `Custom` spells also get a read-only
//! summary of their inline entry data and an "Edit details…" button.

use crate::character::SpellRef;
use crate::render::names::spell_circle_label;
use crate::rules::database::{SpellEntry, database};
use crate::ui::pickers::spell_picker::SpellPickerState;
use crate::ui::search::{self, MatchTarget, NoteParent, SectionId};
use crate::ui::state::AppState;
use crate::ui::widgets::dot_source::{DotSourceKind, dot_source_editor};
use crate::ui::widgets::icon_button::trash_button_with_label;
use crate::ui::widgets::notes_list::notes_editor;

const SPELL_SOURCES: &[DotSourceKind] = &[
    DotSourceKind::ChargenPriority,
    DotSourceKind::BonusPoints,
    DotSourceKind::Xp,
];

pub fn render(ui: &mut egui::Ui, state: &mut AppState) {
    let count = state.character.spells.len();
    let heading_hl = state
        .search
        .highlight_for(MatchTarget::SectionHeading(SectionId::Spells));
    search::highlight_heading(
        ui,
        &format!("{} ({})", SectionId::Spells.label(), count),
        heading_hl,
        state.search.scroll_pending,
    );

    if ui.button("+ Add spell…").clicked() {
        tracing::debug!(picker = "spell", "opened picker");
        state.spell_picker = Some(SpellPickerState::new_for_character(&state.character));
    }

    let db = database();
    let mut any_changed = false;
    let mut delete_idx: Option<usize> = None;
    let mut start_edit: Option<(usize, SpellEntry)> = None;
    for (i, spell) in state.character.spells.iter_mut().enumerate() {
        let id_opt = spell.id().map(|s| s.to_string());
        let name = spell.display_name(db).to_string();
        let circle_label = spell.circle(db).map(spell_circle_label).unwrap_or("—");
        let header = format!("[{}] {}", circle_label, name);
        let entry_detail = spell.entry(db).map(|e| {
            (
                e.effect.clone(),
                format!("cost: {}   duration: {}", e.cost, e.duration),
            )
        });
        let custom_snapshot: Option<SpellEntry> = match spell {
            SpellRef::Custom { entry, .. } => Some(entry.clone()),
            _ => None,
        };
        let force_open = state.search.focused_within(|t| match t {
            MatchTarget::Spell { idx, .. } => *idx == i,
            MatchTarget::Note {
                parent: NoteParent::Spell(p),
                ..
            } => *p == i,
            _ => false,
        });
        let mut header_widget = egui::CollapsingHeader::new(header)
            .id_salt(("spell", i))
            .default_open(false);
        if force_open {
            header_widget = header_widget.open(Some(true));
        }
        header_widget.show(ui, |ui| {
            let (source, notes) = match spell {
                SpellRef::Lookup { source, notes, .. } => (source, notes),
                SpellRef::Custom { source, notes, .. } => (source, notes),
            };

            ui.horizontal(|ui| {
                ui.label("source");
                if dot_source_editor(ui, ("spell-src", i), source, SPELL_SOURCES) {
                    any_changed = true;
                }
                if trash_button_with_label(ui, "remove").clicked() {
                    delete_idx = Some(i);
                }
            });
            match &entry_detail {
                Some((effect, detail)) => {
                    if !effect.is_empty() {
                        ui.small(effect);
                    }
                    ui.small(detail);
                }
                None => {
                    if let Some(id) = &id_opt {
                        ui.small(format!("id {} not in rules database", id));
                    }
                }
            }
            ui.add_space(4.0);
            ui.label("Notes");
            if notes_editor(
                ui,
                ("spell-notes", i),
                notes,
                NoteParent::Spell(i),
                &state.search,
            ) {
                any_changed = true;
            }

            if let Some(entry) = &custom_snapshot {
                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Custom spell").strong());
                    if ui.small_button("Edit details…").clicked() {
                        start_edit = Some((i, entry.clone()));
                    }
                });
                ui.small(format!(
                    "keywords: {}   target: {}",
                    if entry.keywords.is_empty() {
                        "—".to_string()
                    } else {
                        entry.keywords.join(", ")
                    },
                    if entry.target.is_empty() {
                        "—"
                    } else {
                        entry.target.as_str()
                    }
                ));
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
        state.character.spells.remove(i);
        any_changed = true;
    }
    if let Some((idx, _)) = &start_edit {
        tracing::debug!(kind = "spell", index = idx, "started custom entry edit");
    }
    if let Some(payload) = start_edit {
        state.editing_custom_spell = Some(payload);
    }
    if any_changed {
        state.mark_dirty_with("spells.edit");
    }
}

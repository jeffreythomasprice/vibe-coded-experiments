//! The workhorse: edit a `RatedTrait` (base_dots + purchases + specialties).
//! Used by every attribute, ability, virtue, and per-background trait.

use std::cmp::Ordering;

use crate::character::{AbilityKind, DotPurchase, DotSource, RatedTrait, Specialty};
use crate::ui::search::{self, HighlightKind, MatchTarget, SearchState, TextEditOpts};
use crate::ui::widgets::dot_source::{DotSourceKind, dot_source_editor};
use crate::ui::widgets::dots_grid::DotsGrid;
use crate::ui::widgets::icon_button::trash_button;

pub struct RatedTraitOpts<'a> {
    /// Label shown to the left of the dot row (e.g. "Strength").
    pub label: &'a str,
    /// Largest legal value for `dots()` (typically 5; Willpower goes to 10).
    pub max_dots: u8,
    /// Source variants the user can pick from when adding a new dot or
    /// editing an existing purchase.
    pub allowed_sources: &'a [DotSourceKind],
    /// Default source for the "+1 dot" button — i.e. what gets pushed when
    /// the user clicks the plus without first picking a source. Pick the
    /// most common case for the surrounding panel (ChargenPriority for
    /// chargen sections, Xp for in-play sections).
    pub default_add_source: DotSource,
    /// Show the specialties sub-editor below the trait. Attributes don't
    /// have specialties; Abilities do.
    pub show_specialties: bool,
    /// When `Some`, the label becomes a clickable widget that renders with a
    /// filled background when selected. The widget toggles `clicked` to true
    /// if the user clicked the label this frame; the caller is responsible
    /// for translating that into a change to its own selection state.
    pub selectable: Option<Selectable<'a>>,
    /// Optional search integration. When `search` is set, the label and any
    /// specialty rows compare themselves against the active query and apply
    /// highlight visuals. `label_target` identifies what kind of label this
    /// is (AttributeLabel / AbilityLabel / VirtueLabel / Background name).
    /// `specialty_ability` identifies which `AbilityKind` owns these
    /// specialties so individual specialty matches can be addressed.
    pub search: Option<&'a SearchState>,
    pub label_target: Option<MatchTarget>,
    pub specialty_ability: Option<AbilityKind>,
}

/// Click-to-select handle passed in via `RatedTraitOpts`. The widget reads
/// `is_selected` to render the label with a filled background; on click, it
/// sets `*clicked = true`.
pub struct Selectable<'a> {
    pub is_selected: bool,
    pub clicked: &'a mut bool,
}

/// Render the trait editor. Returns true if the user changed anything.
pub fn rated_trait_editor(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash + Copy,
    trait_: &mut RatedTrait,
    opts: &mut RatedTraitOpts,
) -> bool {
    rated_trait_editor_with_prefix(ui, id_source, trait_, opts, |_| false)
}

/// Like [`rated_trait_editor`] but renders `prefix` inside the head
/// `ui.horizontal` row, immediately before the label. Used by the Abilities
/// section to fold the favored-ability checkbox into each row.
pub fn rated_trait_editor_with_prefix(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash + Copy,
    trait_: &mut RatedTrait,
    opts: &mut RatedTraitOpts,
    prefix: impl FnOnce(&mut egui::Ui) -> bool,
) -> bool {
    let mut changed = false;
    let salt = egui::Id::new(id_source);

    let label_highlight: Option<HighlightKind> = match (opts.search, opts.label_target) {
        (Some(s), Some(t)) => s.highlight_for(t),
        _ => None,
    };
    let scroll_pending = opts.search.is_some_and(|s| s.scroll_pending);

    ui.horizontal(|ui| {
        if prefix(ui) {
            changed = true;
        }
        match opts.selectable.as_mut() {
            None => {
                if label_highlight.is_some() {
                    search::highlight_label(ui, opts.label, label_highlight, scroll_pending);
                } else {
                    ui.add_sized([140.0, 0.0], egui::Label::new(opts.label));
                }
            }
            Some(sel) => {
                let resp = ui.add_sized(
                    [140.0, 0.0],
                    egui::Button::selectable(sel.is_selected, opts.label),
                );
                if resp.clicked() {
                    *sel.clicked = true;
                }
                if label_highlight == Some(HighlightKind::Focused) && scroll_pending {
                    resp.scroll_to_me(Some(egui::Align::Center));
                }
                if let Some(kind) = label_highlight {
                    let color = match kind {
                        HighlightKind::Match => {
                            egui::Color32::from_rgba_unmultiplied(255, 220, 60, 60)
                        }
                        HighlightKind::Focused => {
                            egui::Color32::from_rgba_unmultiplied(255, 140, 0, 90)
                        }
                    };
                    ui.painter().rect_filled(resp.rect, 2.0, color);
                }
            }
        }

        // Clickable dot row replaces the old "n / max" label + ± buttons.
        if let Some(target) = DotsGrid::new(trait_.dots(), opts.max_dots)
            .min(trait_.base_dots)
            .show(ui, salt.with("dots"))
        {
            apply_dot_target(trait_, target, opts.default_add_source);
            changed = true;
        }

        // Base dots editor (collapsed by default by being just a small drag).
        ui.separator();
        ui.label("base");
        let mut base = trait_.base_dots as u32;
        let resp = ui.add(
            egui::DragValue::new(&mut base)
                .range(0u32..=opts.max_dots as u32)
                .speed(0.05),
        );
        if resp.changed() {
            trait_.base_dots = base.min(opts.max_dots as u32) as u8;
            changed = true;
        }
    });

    // Purchases list — expanded only on demand.
    if !trait_.purchases.is_empty() {
        egui::CollapsingHeader::new(format!("Purchases ({})", trait_.purchases.len()))
            .id_salt(salt.with("purchases"))
            .show(ui, |ui| {
                let mut delete_idx: Option<usize> = None;
                for (i, p) in trait_.purchases.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("#{}", i + 1));
                        if dot_source_editor(
                            ui,
                            salt.with(("purchase-src", i)),
                            &mut p.source,
                            opts.allowed_sources,
                        ) {
                            changed = true;
                        }
                        if trash_button(ui).clicked() {
                            delete_idx = Some(i);
                        }
                    });
                }
                if let Some(i) = delete_idx {
                    trait_.purchases.remove(i);
                    changed = true;
                }
            });
    }

    if opts.show_specialties {
        let header = if trait_.specialties.is_empty() {
            "Specialties".to_string()
        } else {
            format!("Specialties ({})", trait_.specialties.len())
        };
        // Force open when the focused match is one of our specialties.
        let force_open_specialties = match (opts.search, opts.specialty_ability) {
            (Some(s), Some(ab)) => s.focused_within(
                |t| matches!(t, MatchTarget::Specialty { ability, .. } if *ability == ab),
            ),
            _ => false,
        };
        let mut header_widget =
            egui::CollapsingHeader::new(header).id_salt(salt.with("specialties"));
        if force_open_specialties {
            header_widget = header_widget.open(Some(true));
        }
        header_widget.show(ui, |ui| {
            let mut delete_idx: Option<usize> = None;
            for (i, s) in trait_.specialties.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let target = opts.specialty_ability.map(|ab| MatchTarget::Specialty {
                        ability: ab,
                        idx: i,
                    });
                    let highlight = match (opts.search, target) {
                        (Some(state), Some(t)) => state.highlight_for(t),
                        _ => None,
                    };
                    let resp = if let Some(state) = opts.search {
                        search::highlighted_singleline(
                            ui,
                            &mut s.name,
                            &state.query,
                            highlight,
                            TextEditOpts {
                                desired_width: 160.0,
                                hint: Some("name"),
                            },
                            state.scroll_pending,
                        )
                    } else {
                        ui.add(
                            egui::TextEdit::singleline(&mut s.name)
                                .hint_text("name")
                                .desired_width(160.0),
                        )
                    };
                    if resp.changed() {
                        changed = true;
                    }
                    if dot_source_editor(
                        ui,
                        salt.with(("specialty-src", i)),
                        &mut s.source,
                        opts.allowed_sources,
                    ) {
                        changed = true;
                    }
                    if trash_button(ui).clicked() {
                        delete_idx = Some(i);
                    }
                });
            }
            if let Some(i) = delete_idx {
                trait_.specialties.remove(i);
                changed = true;
            }
            if ui.button("+ Add specialty").clicked() {
                trait_.specialties.push(Specialty {
                    name: String::new(),
                    source: opts.default_add_source,
                });
                changed = true;
            }
        });
    }

    changed
}

/// Drive `trait_.dots()` toward `target` by appending purchases (using
/// `add_source`) or truncating the most recent ones. `base_dots` is the floor —
/// targets below it snap up to it, so we never lose the free starting dots.
fn apply_dot_target(trait_: &mut RatedTrait, target: u8, add_source: DotSource) {
    let target = target.max(trait_.base_dots);
    let current = trait_.dots();
    match target.cmp(&current) {
        Ordering::Greater => {
            for _ in 0..(target - current) {
                trait_.purchases.push(DotPurchase::new(add_source));
            }
        }
        Ordering::Less => {
            // purchases.len() == new_dots − base_dots
            let keep = (target - trait_.base_dots) as usize;
            trait_.purchases.truncate(keep);
        }
        Ordering::Equal => {}
    }
}

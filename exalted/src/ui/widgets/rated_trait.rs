//! The workhorse: edit a `RatedTrait` (base_dots + purchases + specialties).
//! Used by every attribute, ability, virtue, and per-background trait.

use crate::character::{DotPurchase, DotSource, RatedTrait, Specialty};
use crate::ui::widgets::dot_source::{dot_source_editor, DotSourceKind};

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
}

/// Render the trait editor. Returns true if the user changed anything.
pub fn rated_trait_editor(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash + Copy,
    trait_: &mut RatedTrait,
    opts: &RatedTraitOpts,
) -> bool {
    let mut changed = false;
    let salt = egui::Id::new(id_source);

    ui.horizontal(|ui| {
        ui.add_sized([140.0, 0.0], egui::Label::new(opts.label));

        // Live dot count (base + purchases).
        let current = trait_.dots();
        ui.label(format!("{} / {}", current, opts.max_dots));

        // − removes the most recent purchase. Disabled when there is none
        // (we never decrement `base_dots` here; that's an explicit field
        // edit below).
        let minus = ui.add_enabled(
            !trait_.purchases.is_empty(),
            egui::Button::new("−"),
        );
        if minus.clicked() {
            trait_.purchases.pop();
            changed = true;
        }

        // + adds a purchase with `default_add_source`. Disabled at max.
        let plus = ui.add_enabled(
            current < opts.max_dots,
            egui::Button::new("+"),
        );
        if plus.clicked() {
            trait_
                .purchases
                .push(DotPurchase::new(opts.default_add_source));
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
                        if ui.small_button("✕").clicked() {
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
        egui::CollapsingHeader::new(header)
            .id_salt(salt.with("specialties"))
            .show(ui, |ui| {
                let mut delete_idx: Option<usize> = None;
                for (i, s) in trait_.specialties.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut s.name)
                                .hint_text("name")
                                .desired_width(160.0),
                        );
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
                        if ui.small_button("✕").clicked() {
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

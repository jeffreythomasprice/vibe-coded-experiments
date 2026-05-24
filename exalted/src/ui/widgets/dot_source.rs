//! Edit a `DotSource` in place. The caller passes the set of variants that
//! are legal in this context (e.g. ChargenPriority + BonusPoints for
//! attribute purchases, BonusPoints + Xp for charms, etc.).

use crate::character::DotSource;

/// Tag enum mirroring `DotSource` without the payload. Used to advertise the
/// legal variants in a given edit context.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DotSourceKind {
    Base,
    ChargenPriority,
    BonusPoints,
    Xp,
}

impl DotSourceKind {
    pub fn label(self) -> &'static str {
        match self {
            DotSourceKind::Base => "Base",
            DotSourceKind::ChargenPriority => "Chargen",
            DotSourceKind::BonusPoints => "BP",
            DotSourceKind::Xp => "XP",
        }
    }

    pub fn of(source: &DotSource) -> Self {
        match source {
            DotSource::Base => DotSourceKind::Base,
            DotSource::ChargenPriority => DotSourceKind::ChargenPriority,
            DotSource::BonusPoints { .. } => DotSourceKind::BonusPoints,
            DotSource::Xp { .. } => DotSourceKind::Xp,
        }
    }
}

/// Edit a `DotSource` with a ComboBox of legal variants and an inline
/// DragValue for the `spent` field on BP/Xp variants. Returns true if the
/// user changed anything this frame.
pub fn dot_source_editor(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash,
    source: &mut DotSource,
    allowed: &[DotSourceKind],
) -> bool {
    let mut changed = false;
    let mut kind = DotSourceKind::of(source);
    let prev_kind = kind;

    egui::ComboBox::from_id_salt(id_source)
        .width(96.0)
        .selected_text(kind.label())
        .show_ui(ui, |ui| {
            for k in allowed {
                ui.selectable_value(&mut kind, *k, k.label());
            }
        });

    if kind != prev_kind {
        // Re-shape the source to the new variant with a default payload. The
        // user can then tune `spent` via the DragValue below.
        *source = match kind {
            DotSourceKind::Base => DotSource::Base,
            DotSourceKind::ChargenPriority => DotSource::ChargenPriority,
            DotSourceKind::BonusPoints => DotSource::BonusPoints { spent: 0 },
            DotSourceKind::Xp => DotSource::Xp { spent: 0 },
        };
        changed = true;
    }

    match source {
        DotSource::BonusPoints { spent } => {
            let mut v = *spent as u32;
            let resp = ui.add(
                egui::DragValue::new(&mut v)
                    .range(0..=255)
                    .suffix(" BP")
                    .speed(0.1),
            );
            if resp.changed() {
                *spent = v.min(255) as u8;
                changed = true;
            }
        }
        DotSource::Xp { spent } => {
            let resp = ui.add(
                egui::DragValue::new(spent)
                    .range(0u32..=10_000)
                    .suffix(" XP")
                    .speed(0.1),
            );
            if resp.changed() {
                changed = true;
            }
        }
        DotSource::Base | DotSource::ChargenPriority => {}
    }

    changed
}

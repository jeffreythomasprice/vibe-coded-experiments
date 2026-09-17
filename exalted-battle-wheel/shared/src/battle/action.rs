use crate::battle::ids::CombatantId;
use crate::battle::mode::BattleMode;

pub use crate::generated::{ActionKind, DeclaredAction, DeclaredEffect, DvPenaltySpec, Label, Note, SpeedSpec};

impl SpeedSpec {
    pub fn resolve(self, override_value: Option<u32>) -> u32 {
        match self {
            SpeedSpec::Fixed(speed) => speed,
            SpeedSpec::Variable { default } => override_value.unwrap_or(default),
        }
    }
}

impl DvPenaltySpec {
    pub fn resolve(self, override_value: Option<i32>) -> i32 {
        match self {
            DvPenaltySpec::Fixed(penalty) => penalty,
            DvPenaltySpec::Variable { default } => override_value.unwrap_or(default),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ActionTemplate {
    pub kind: ActionKind,
    pub name: &'static str,
    pub speed: SpeedSpec,
    pub dv_penalty: DvPenaltySpec,
    pub reflexive: bool,
    pub flurryable: bool,
}

/// RULES.md §4 and §14 (pp. 141-145): the core action catalog, plus the named miscellaneous
/// actions from §4.7 (p. 144) that share its Speed 5 / DV choice. Mass combat reuses every row
/// here verbatim (see `catalog`) — characters there "substitute long ticks for standard ticks"
/// (§11.1, p. 158) — measured in long ticks instead of ticks.
pub const PERSONAL_CATALOG: &[ActionTemplate] = &[
    ActionTemplate {
        kind: ActionKind::Aim,
        name: "Aim",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Attack,
        name: "Attack",
        speed: SpeedSpec::Variable { default: 5 },
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: true,
    },
    ActionTemplate {
        kind: ActionKind::Dash,
        name: "Dash",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-2),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Guard,
        name: "Guard",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Inactive,
        name: "Inactive",
        speed: SpeedSpec::Fixed(5),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Miscellaneous,
        name: "Miscellaneous Action",
        speed: SpeedSpec::Fixed(5),
        dv_penalty: DvPenaltySpec::Variable { default: -1 },
        reflexive: false,
        flurryable: true,
    },
    ActionTemplate {
        kind: ActionKind::Move,
        name: "Move",
        speed: SpeedSpec::Fixed(0),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: true,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Flurry,
        name: "Flurry",
        speed: SpeedSpec::Variable { default: 5 },
        dv_penalty: DvPenaltySpec::Variable { default: -3 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::ActivateCharm,
        name: "Activate Charm",
        speed: SpeedSpec::Variable { default: 6 },
        dv_penalty: DvPenaltySpec::Variable { default: 0 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Clinch,
        name: "Clinch",
        speed: SpeedSpec::Fixed(6),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: true,
    },
    ActionTemplate {
        kind: ActionKind::JoinBattleInProgress,
        name: "Join Battle (in progress)",
        speed: SpeedSpec::Variable { default: 0 },
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Custom,
        name: "Custom",
        speed: SpeedSpec::Variable { default: 5 },
        dv_penalty: DvPenaltySpec::Variable { default: 0 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::CoordinateAttacks,
        name: "Coordinate Attacks",
        speed: SpeedSpec::Fixed(5),
        dv_penalty: DvPenaltySpec::Variable { default: -1 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::ReadyWeapons,
        name: "Draw / Ready Weapons",
        speed: SpeedSpec::Fixed(5),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::RiseFromProne,
        name: "Rise From Prone",
        speed: SpeedSpec::Fixed(5),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Jump,
        name: "Jump",
        speed: SpeedSpec::Fixed(5),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
];

/// RULES.md §11.1, Exalted 2E pp. 164-165: the unit-level actions mass combat adds on top of
/// `PERSONAL_CATALOG`. Disengage and Expel are reflexive at Speed 0; none is flurryable — the
/// book authorizes flurrying attacks, not these.
pub const MASS_ONLY_CATALOG: &[ActionTemplate] = &[
    ActionTemplate {
        kind: ActionKind::ChangeFormation,
        name: "Change Formation",
        speed: SpeedSpec::Fixed(5),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Disengage,
        name: "Disengage",
        speed: SpeedSpec::Fixed(0),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: true,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Turn,
        name: "Turn (over 90\u{b0})",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::SplitUnit,
        name: "Split Unit",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::ExpelSpecialCharacter,
        name: "Expel a Special Character",
        speed: SpeedSpec::Fixed(0),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: true,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::MergeUnits,
        name: "Merge Units",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::SignalUnits,
        name: "Signal Units",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Rally,
        name: "Rally",
        speed: SpeedSpec::Fixed(4),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
];

/// RULES.md §11.2, Exalted 2E p. 171: social combat's own action list. Deliberately NOT shared
/// with `PERSONAL_CATALOG` — Dash, Inactive, Attack, Flurry, and Miscellaneous all carry
/// different Speeds/DVs here than in physical combat, which is the whole reason the catalog is
/// keyed by mode rather than by `ActionKind` alone.
pub const SOCIAL_CATALOG: &[ActionTemplate] = &[
    ActionTemplate {
        kind: ActionKind::Move,
        name: "Move",
        speed: SpeedSpec::Fixed(0),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: true,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Dash,
        name: "Dash",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-3),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Guard,
        name: "Guard",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Inactive,
        name: "Inactive",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Aim,
        name: "Monologue / Study",
        speed: SpeedSpec::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-2),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Miscellaneous,
        name: "Miscellaneous Action",
        speed: SpeedSpec::Fixed(5),
        dv_penalty: DvPenaltySpec::Variable { default: -2 },
        reflexive: false,
        flurryable: true,
    },
    ActionTemplate {
        kind: ActionKind::JoinBattleInProgress,
        name: "Join Debate (in progress)",
        speed: SpeedSpec::Variable { default: 5 },
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::ReadMotivation,
        name: "Read Motivation",
        speed: SpeedSpec::Fixed(5),
        dv_penalty: DvPenaltySpec::Variable { default: 0 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Flurry,
        name: "Flurry",
        speed: SpeedSpec::Variable { default: 4 },
        dv_penalty: DvPenaltySpec::Variable { default: -4 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::ActivateCharm,
        name: "Activate Charm",
        speed: SpeedSpec::Variable { default: 6 },
        dv_penalty: DvPenaltySpec::Variable { default: 0 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Attack,
        name: "Social Attack",
        speed: SpeedSpec::Variable { default: 4 },
        dv_penalty: DvPenaltySpec::Fixed(-2),
        reflexive: false,
        flurryable: true,
    },
    ActionTemplate {
        kind: ActionKind::Custom,
        name: "Custom",
        speed: SpeedSpec::Variable { default: 5 },
        dv_penalty: DvPenaltySpec::Variable { default: 0 },
        reflexive: false,
        flurryable: false,
    },
];

/// Every action available in `mode`, in menu order. Mass combat chains onto the personal catalog
/// rather than copying it, so a later change to a shared action's Speed/DV cannot silently drift
/// between the two physical modes.
pub fn catalog(mode: BattleMode) -> Box<dyn Iterator<Item = &'static ActionTemplate>> {
    match mode {
        BattleMode::Personal => Box::new(PERSONAL_CATALOG.iter()),
        BattleMode::Mass => Box::new(PERSONAL_CATALOG.iter().chain(MASS_ONLY_CATALOG)),
        BattleMode::Social => Box::new(SOCIAL_CATALOG.iter()),
    }
}

/// A unit does not Clinch; a debater does not Rally. `template` is a partial function over
/// `(mode, kind)`, and this names the miss instead of panicking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{kind:?} is not an action in {mode:?}")]
pub struct ActionError {
    pub mode: BattleMode,
    pub kind: ActionKind,
}

/// Looks up an action's template within a mode.
pub fn template(mode: BattleMode, kind: ActionKind) -> Result<&'static ActionTemplate, ActionError> {
    catalog_index(mode, kind).and_then(|index| catalog(mode).nth(index)).ok_or(ActionError { mode, kind })
}

/// The position of `kind` within `catalog(mode)` — the same index the action panel's `<select>`
/// numbers its options by (`ChoiceKey::Action`), so a caller with a `kind` (e.g. the reference
/// rail) can pick the same option a user picking by name would land on. `None` for a kind that
/// isn't on the menu in this mode, same partiality as `template`.
pub fn catalog_index(mode: BattleMode, kind: ActionKind) -> Option<usize> {
    catalog(mode).position(|template| template.kind == kind)
}

/// Everything a `Declare` click can vary about an `ActionTemplate`. `name` overrides the label a
/// custom or renamed action logs under; blank falls back to the template's own name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declaration {
    pub name: Option<String>,
    pub speed: Option<u32>,
    pub dv_penalty: Option<i32>,
    pub target: Option<CombatantId>,
    pub note: String,
    pub effects: Vec<DeclaredEffect>,
}

/// Mirrors `Label`'s `maxLength` in `shared/schemas/common.json` -- a test asserts the two stay
/// equal. Free-form text is truncated to fit rather than rejected, the same policy
/// `protocol::name::sanitize_name` uses for a player's display name.
pub const MAX_LABEL_LEN: usize = 120;

/// Mirrors `Note`'s `maxLength` in `shared/schemas/common.json` -- see `MAX_LABEL_LEN`.
pub const MAX_NOTE_LEN: usize = 1000;

fn truncate_chars(text: &str, max: usize) -> &str {
    match text.char_indices().nth(max) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

/// Truncates to `Label`'s bound rather than rejecting -- see `MAX_LABEL_LEN`.
pub fn label(text: impl AsRef<str>) -> Label {
    Label::try_from(truncate_chars(text.as_ref(), MAX_LABEL_LEN)).expect("truncated to fit Label's bound")
}

/// Truncates to `Note`'s bound rather than rejecting -- see `MAX_NOTE_LEN`.
pub fn note(text: impl AsRef<str>) -> Note {
    Note::try_from(truncate_chars(text.as_ref(), MAX_NOTE_LEN)).expect("truncated to fit Note's bound")
}

impl ActionTemplate {
    pub fn declare(&self, declaration: Declaration) -> DeclaredAction {
        let label_text = match declaration.name {
            Some(name) if !name.trim().is_empty() => name.trim().to_string(),
            _ => self.name.to_string(),
        };
        DeclaredAction {
            kind: self.kind,
            label: label(label_text),
            speed: self.speed.resolve(declaration.speed),
            dv_penalty: self.dv_penalty.resolve(declaration.dv_penalty),
            reflexive: self.reflexive,
            target: declaration.target,
            note: note(declaration.note),
            effects: declaration.effects,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn personal(kind: ActionKind) -> &'static ActionTemplate {
        template(BattleMode::Personal, kind).expect("personal catalog")
    }

    /// `Label`/`Note` are generated from `shared/schemas/common.json`'s `maxLength`; `MAX_LABEL_LEN`
    /// and `MAX_NOTE_LEN` must never drift from those bounds, since `label()`/`note()`'s truncation
    /// assumes they match exactly. Same indirect check as `protocol::name`'s equivalent test, for
    /// the same reason: typify inlines the literal bound with no constant to compare against.
    #[test]
    fn label_and_note_bounds_match_the_local_constants() {
        assert!(Label::try_from("a".repeat(MAX_LABEL_LEN)).is_ok());
        assert!(Label::try_from("a".repeat(MAX_LABEL_LEN + 1)).is_err());
        assert!(Note::try_from("a".repeat(MAX_NOTE_LEN)).is_ok());
        assert!(Note::try_from("a".repeat(MAX_NOTE_LEN + 1)).is_err());
    }

    #[test]
    fn label_truncates_rather_than_rejecting() {
        let long = "a".repeat(MAX_LABEL_LEN + 10);
        assert_eq!(label(&long).chars().count(), MAX_LABEL_LEN);
    }

    #[test]
    fn note_truncates_rather_than_rejecting() {
        let long = "a".repeat(MAX_NOTE_LEN + 10);
        assert_eq!(note(&long).chars().count(), MAX_NOTE_LEN);
    }

    #[test]
    fn variable_speed_uses_default_without_override() {
        assert_eq!(SpeedSpec::Variable { default: 5 }.resolve(None), 5);
    }

    #[test]
    fn variable_speed_uses_override_when_given() {
        assert_eq!(SpeedSpec::Variable { default: 5 }.resolve(Some(4)), 4);
    }

    #[test]
    fn fixed_speed_ignores_override() {
        assert_eq!(SpeedSpec::Fixed(3).resolve(Some(4)), 3);
    }

    #[test]
    fn declare_uses_a_given_name_over_the_template_name() {
        let action = personal(ActionKind::Attack).declare(Declaration { name: Some("Sweeping Blow".to_string()), ..Default::default() });
        assert_eq!(action.label.to_string(), "Sweeping Blow");
    }

    #[test]
    fn declare_falls_back_to_the_template_name_when_blank() {
        let blank = personal(ActionKind::Attack).declare(Declaration { name: Some("   ".to_string()), ..Default::default() });
        assert_eq!(blank.label.to_string(), "Attack");
        let none = personal(ActionKind::Attack).declare(Declaration::default());
        assert_eq!(none.label.to_string(), "Attack");
    }

    #[test]
    fn declare_trims_a_given_name() {
        let action = personal(ActionKind::Attack).declare(Declaration { name: Some("  Sweeping Blow  ".to_string()), ..Default::default() });
        assert_eq!(action.label.to_string(), "Sweeping Blow");
    }

    #[test]
    fn template_covers_every_personal_catalog_entry() {
        for entry in PERSONAL_CATALOG {
            assert_eq!(personal(entry.kind).kind, entry.kind);
        }
    }

    #[test]
    fn catalog_has_no_duplicate_kinds_within_a_mode() {
        for mode in BattleMode::ALL {
            let mut kinds: Vec<ActionKind> = catalog(mode).map(|t| t.kind).collect();
            let before = kinds.len();
            kinds.sort_by_key(|k| format!("{k:?}"));
            kinds.dedup();
            assert_eq!(kinds.len(), before, "{mode:?} catalog has a duplicate ActionKind");
        }
    }

    #[test]
    fn mass_catalog_contains_every_personal_action() {
        let mass: Vec<ActionKind> = catalog(BattleMode::Mass).map(|t| t.kind).collect();
        for entry in PERSONAL_CATALOG {
            assert!(mass.contains(&entry.kind), "mass catalog is missing personal action {:?}", entry.kind);
        }
    }

    #[test]
    fn template_rejects_a_kind_outside_its_mode() {
        let err = template(BattleMode::Social, ActionKind::Clinch).unwrap_err();
        assert_eq!(err, ActionError { mode: BattleMode::Social, kind: ActionKind::Clinch });
        let err = template(BattleMode::Personal, ActionKind::Rally).unwrap_err();
        assert_eq!(err, ActionError { mode: BattleMode::Personal, kind: ActionKind::Rally });
    }

    #[test]
    fn social_attack_has_its_own_speed_and_dv() {
        let social_attack = template(BattleMode::Social, ActionKind::Attack).unwrap();
        assert_eq!(social_attack.dv_penalty, DvPenaltySpec::Fixed(-2));
        let personal_attack = personal(ActionKind::Attack);
        assert_eq!(personal_attack.dv_penalty, DvPenaltySpec::Fixed(-1));
    }

    #[test]
    fn catalog_index_round_trips_every_catalog_entry() {
        for mode in BattleMode::ALL {
            for (i, entry) in catalog(mode).enumerate() {
                assert_eq!(catalog_index(mode, entry.kind), Some(i), "{mode:?} entry {i} ({:?})", entry.kind);
            }
        }
    }

    #[test]
    fn catalog_index_rejects_a_kind_outside_its_mode() {
        assert_eq!(catalog_index(BattleMode::Social, ActionKind::Clinch), None);
        assert_eq!(catalog_index(BattleMode::Personal, ActionKind::ChangeFormation), None);
    }

    #[test]
    fn catalog_index_offsets_mass_only_actions_past_the_personal_catalog() {
        assert_eq!(catalog_index(BattleMode::Mass, ActionKind::ChangeFormation), Some(PERSONAL_CATALOG.len()));
    }

    #[test]
    fn catalog_index_resolves_a_shared_kind_per_mode() {
        // The same `ActionKind` names a different action per mode (different Speed/DV, different
        // display name), which is why the reference rail must resolve a click through the
        // section's own mode rather than by `ActionKind` alone.
        let personal_index = catalog_index(BattleMode::Personal, ActionKind::Aim).unwrap();
        assert_eq!(catalog(BattleMode::Personal).nth(personal_index).unwrap().name, "Aim");
        let social_index = catalog_index(BattleMode::Social, ActionKind::Aim).unwrap();
        assert_eq!(catalog(BattleMode::Social).nth(social_index).unwrap().name, "Monologue / Study");
    }
}

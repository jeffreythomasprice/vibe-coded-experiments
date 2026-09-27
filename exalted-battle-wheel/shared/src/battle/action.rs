use crate::battle::ids::CombatantId;
use crate::battle::mode::BattleMode;

pub use crate::generated::{
    ActionKind, DeclaredAction, DeclaredEffect, DvPenaltySpec, FlurriedAction, FlurryBreakdown, FlurryDvRule, Label, Note, SpeedSpec,
};

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

/// A template's Speed, unlike `SpeedSpec` (a wire type shared with persisted sequence steps),
/// admits a third case: no default at all. Attack's Speed is the weapon or maneuver used (RULES.md
/// §4.4, p. 143) and Social Attack's is set by the Ability used (§11.2, pp. 171-172) — there's no
/// sensible number to declare with if the user hasn't entered one, so `Required` has none to fall
/// back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSpeed {
    Fixed(u32),
    Variable { default: u32 },
    Required,
}

impl ActionSpeed {
    /// `None` only for `Required` with no `entered` value; every other case always resolves.
    pub fn resolve(self, entered: Option<u32>) -> Option<u32> {
        match self {
            ActionSpeed::Fixed(speed) => Some(speed),
            ActionSpeed::Variable { default } => Some(entered.unwrap_or(default)),
            ActionSpeed::Required => entered,
        }
    }
}

/// An action whose Speed has no default (see `ActionSpeed::Required`) was declared without one
/// entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{name} has no default Speed; enter one to declare it")]
pub struct SpeedRequired {
    pub name: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ActionTemplate {
    pub kind: ActionKind,
    pub name: &'static str,
    pub speed: ActionSpeed,
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
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Attack,
        name: "Attack",
        speed: ActionSpeed::Required,
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: true,
    },
    ActionTemplate {
        kind: ActionKind::Dash,
        name: "Dash",
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-2),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Guard,
        name: "Guard",
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Inactive,
        name: "Inactive",
        speed: ActionSpeed::Fixed(5),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Miscellaneous,
        name: "Miscellaneous Action",
        speed: ActionSpeed::Fixed(5),
        dv_penalty: DvPenaltySpec::Variable { default: -1 },
        reflexive: false,
        flurryable: true,
    },
    ActionTemplate {
        kind: ActionKind::Move,
        name: "Move",
        speed: ActionSpeed::Fixed(0),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: true,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Flurry,
        name: "Flurry",
        speed: ActionSpeed::Variable { default: 5 },
        dv_penalty: DvPenaltySpec::Variable { default: -3 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::ActivateCharm,
        name: "Activate Charm",
        speed: ActionSpeed::Variable { default: 6 },
        dv_penalty: DvPenaltySpec::Variable { default: 0 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Clinch,
        name: "Clinch",
        speed: ActionSpeed::Fixed(6),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: true,
    },
    ActionTemplate {
        kind: ActionKind::JoinBattleInProgress,
        name: "Join Battle (in progress)",
        speed: ActionSpeed::Variable { default: 0 },
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Custom,
        name: "Custom",
        speed: ActionSpeed::Variable { default: 5 },
        dv_penalty: DvPenaltySpec::Variable { default: 0 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::CoordinateAttacks,
        name: "Coordinate Attacks",
        speed: ActionSpeed::Fixed(5),
        dv_penalty: DvPenaltySpec::Variable { default: -1 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::ReadyWeapons,
        name: "Draw / Ready Weapons",
        speed: ActionSpeed::Fixed(5),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::RiseFromProne,
        name: "Rise From Prone",
        speed: ActionSpeed::Fixed(5),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Jump,
        name: "Jump",
        speed: ActionSpeed::Fixed(5),
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
        speed: ActionSpeed::Fixed(5),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Disengage,
        name: "Disengage",
        speed: ActionSpeed::Fixed(0),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: true,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Turn,
        name: "Turn (over 90\u{b0})",
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::SplitUnit,
        name: "Split Unit",
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::ExpelSpecialCharacter,
        name: "Expel a Special Character",
        speed: ActionSpeed::Fixed(0),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: true,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::MergeUnits,
        name: "Merge Units",
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-1),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::SignalUnits,
        name: "Signal Units",
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Rally,
        name: "Rally",
        speed: ActionSpeed::Fixed(4),
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
        speed: ActionSpeed::Fixed(0),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: true,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Dash,
        name: "Dash",
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-3),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Guard,
        name: "Guard",
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Inactive,
        name: "Inactive",
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Aim,
        name: "Monologue / Study",
        speed: ActionSpeed::Fixed(3),
        dv_penalty: DvPenaltySpec::Fixed(-2),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Miscellaneous,
        name: "Miscellaneous Action",
        speed: ActionSpeed::Fixed(5),
        dv_penalty: DvPenaltySpec::Variable { default: -2 },
        reflexive: false,
        flurryable: true,
    },
    ActionTemplate {
        kind: ActionKind::JoinBattleInProgress,
        name: "Join Debate (in progress)",
        speed: ActionSpeed::Variable { default: 5 },
        dv_penalty: DvPenaltySpec::Fixed(0),
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::ReadMotivation,
        name: "Read Motivation",
        speed: ActionSpeed::Fixed(5),
        dv_penalty: DvPenaltySpec::Variable { default: 0 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Flurry,
        name: "Flurry",
        speed: ActionSpeed::Variable { default: 4 },
        dv_penalty: DvPenaltySpec::Variable { default: -4 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::ActivateCharm,
        name: "Activate Charm",
        speed: ActionSpeed::Variable { default: 6 },
        dv_penalty: DvPenaltySpec::Variable { default: 0 },
        reflexive: false,
        flurryable: false,
    },
    ActionTemplate {
        kind: ActionKind::Attack,
        name: "Social Attack",
        speed: ActionSpeed::Required,
        dv_penalty: DvPenaltySpec::Fixed(-2),
        reflexive: false,
        flurryable: true,
    },
    ActionTemplate {
        kind: ActionKind::Custom,
        name: "Custom",
        speed: ActionSpeed::Variable { default: 5 },
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
    catalog_index(mode, kind)
        .and_then(|index| catalog(mode).nth(index))
        .ok_or(ActionError { mode, kind })
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

pub(super) fn truncate_chars(text: &str, max: usize) -> &str {
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
    /// The Speed a `Declaration` would resolve to, or the reason it can't: only fails for an
    /// `ActionSpeed::Required` template (Attack, Social Attack) given no `entered` value.
    pub fn resolve_speed(&self, entered: Option<u32>) -> Result<u32, SpeedRequired> {
        self.speed.resolve(entered).ok_or(SpeedRequired { name: self.name })
    }

    pub fn declare(&self, declaration: Declaration) -> Result<DeclaredAction, SpeedRequired> {
        let label_text = match declaration.name {
            Some(name) if !name.trim().is_empty() => name.trim().to_string(),
            _ => self.name.to_string(),
        };
        let speed = self.resolve_speed(declaration.speed)?;
        Ok(DeclaredAction {
            kind: self.kind,
            label: label(label_text),
            speed,
            dv_penalty: self.dv_penalty.resolve(declaration.dv_penalty),
            reflexive: self.reflexive,
            target: declaration.target,
            note: note(declaration.note),
            effects: declaration.effects,
            flurry: None,
        })
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
    fn action_speed_fixed_ignores_entered() {
        assert_eq!(ActionSpeed::Fixed(3).resolve(Some(4)), Some(3));
    }

    #[test]
    fn action_speed_variable_uses_default_without_entered() {
        assert_eq!(ActionSpeed::Variable { default: 5 }.resolve(None), Some(5));
    }

    #[test]
    fn action_speed_variable_uses_entered_when_given() {
        assert_eq!(ActionSpeed::Variable { default: 5 }.resolve(Some(4)), Some(4));
    }

    #[test]
    fn action_speed_required_has_no_default() {
        assert_eq!(ActionSpeed::Required.resolve(None), None);
        assert_eq!(ActionSpeed::Required.resolve(Some(4)), Some(4));
    }

    #[test]
    fn declare_refuses_a_required_speed_action_with_no_speed_entered() {
        let err = personal(ActionKind::Attack).declare(Declaration::default()).unwrap_err();
        assert_eq!(err, SpeedRequired { name: "Attack" });
    }

    #[test]
    fn declare_accepts_a_required_speed_action_once_a_speed_is_entered() {
        let action = personal(ActionKind::Attack)
            .declare(Declaration {
                speed: Some(4),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(action.speed, 4);
    }

    #[test]
    fn declare_uses_a_given_name_over_the_template_name() {
        let action = personal(ActionKind::Attack)
            .declare(Declaration {
                name: Some("Sweeping Blow".to_string()),
                speed: Some(5),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(action.label.to_string(), "Sweeping Blow");
    }

    #[test]
    fn declare_falls_back_to_the_template_name_when_blank() {
        let blank = personal(ActionKind::Attack)
            .declare(Declaration {
                name: Some("   ".to_string()),
                speed: Some(5),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(blank.label.to_string(), "Attack");
        let none = personal(ActionKind::Guard).declare(Declaration::default()).unwrap();
        assert_eq!(none.label.to_string(), "Guard");
    }

    #[test]
    fn declare_trims_a_given_name() {
        let action = personal(ActionKind::Attack)
            .declare(Declaration {
                name: Some("  Sweeping Blow  ".to_string()),
                speed: Some(5),
                ..Default::default()
            })
            .unwrap();
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
            assert!(
                mass.contains(&entry.kind),
                "mass catalog is missing personal action {:?}",
                entry.kind
            );
        }
    }

    #[test]
    fn template_rejects_a_kind_outside_its_mode() {
        let err = template(BattleMode::Social, ActionKind::Clinch).unwrap_err();
        assert_eq!(
            err,
            ActionError {
                mode: BattleMode::Social,
                kind: ActionKind::Clinch
            }
        );
        let err = template(BattleMode::Personal, ActionKind::Rally).unwrap_err();
        assert_eq!(
            err,
            ActionError {
                mode: BattleMode::Personal,
                kind: ActionKind::Rally
            }
        );
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
        assert_eq!(
            catalog_index(BattleMode::Mass, ActionKind::ChangeFormation),
            Some(PERSONAL_CATALOG.len())
        );
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

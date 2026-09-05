use crate::battle::ids::{CombatantId, MarkerId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeedSpec {
    Fixed(u32),
    Variable { default: u32 },
}

impl SpeedSpec {
    pub fn resolve(self, override_value: Option<u32>) -> u32 {
        match self {
            SpeedSpec::Fixed(speed) => speed,
            SpeedSpec::Variable { default } => override_value.unwrap_or(default),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DvPenaltySpec {
    Fixed(i32),
    Variable { default: i32 },
}

impl DvPenaltySpec {
    pub fn resolve(self, override_value: Option<i32>) -> i32 {
        match self {
            DvPenaltySpec::Fixed(penalty) => penalty,
            DvPenaltySpec::Variable { default } => override_value.unwrap_or(default),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionKind {
    Aim,
    Attack,
    Dash,
    Guard,
    Inactive,
    Miscellaneous,
    Move,
    Flurry,
    ActivateCharm,
    Clinch,
    JoinBattleInProgress,
    Custom,
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

/// RULES.md §4 and §14 (pp. 141-145): the core action catalog.
pub const CATALOG: &[ActionTemplate] = &[
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
];

/// A labelled span the action drops on the wheel when it resolves (RULES.md §4.7, p. 144; §9.4,
/// p. 153 — see `Marker`). Actions resolve on the tick they're declared, so `delay` counts from
/// that tick, not from the actor's own next action. `id` is allocated by the caller
/// (`BattleLog::alloc_marker_id`) so replaying the same event is deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredEffect {
    pub id: MarkerId,
    pub label: String,
    pub delay: u32,
    pub ticks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredAction {
    pub kind: ActionKind,
    pub label: String,
    pub speed: u32,
    pub dv_penalty: i32,
    pub reflexive: bool,
    pub target: Option<CombatantId>,
    pub note: String,
    pub effects: Vec<DeclaredEffect>,
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

impl ActionTemplate {
    pub fn declare(&self, declaration: Declaration) -> DeclaredAction {
        let label = match declaration.name {
            Some(name) if !name.trim().is_empty() => name.trim().to_string(),
            _ => self.name.to_string(),
        };
        DeclaredAction {
            kind: self.kind,
            label,
            speed: self.speed.resolve(declaration.speed),
            dv_penalty: self.dv_penalty.resolve(declaration.dv_penalty),
            reflexive: self.reflexive,
            target: declaration.target,
            note: declaration.note,
            effects: declaration.effects,
        }
    }
}

/// Looks up an `ActionKind`'s template. Every `ActionKind` has exactly one `CATALOG` entry, so
/// this only fails if a new kind is added to the enum without a matching catalog row.
pub fn template(kind: ActionKind) -> &'static ActionTemplate {
    CATALOG.iter().find(|template| template.kind == kind).expect("every ActionKind has a CATALOG entry")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let action = template(ActionKind::Attack).declare(Declaration { name: Some("Sweeping Blow".to_string()), ..Default::default() });
        assert_eq!(action.label, "Sweeping Blow");
    }

    #[test]
    fn declare_falls_back_to_the_template_name_when_blank() {
        let blank = template(ActionKind::Attack).declare(Declaration { name: Some("   ".to_string()), ..Default::default() });
        assert_eq!(blank.label, "Attack");
        let none = template(ActionKind::Attack).declare(Declaration::default());
        assert_eq!(none.label, "Attack");
    }

    #[test]
    fn declare_trims_a_given_name() {
        let action = template(ActionKind::Attack).declare(Declaration { name: Some("  Sweeping Blow  ".to_string()), ..Default::default() });
        assert_eq!(action.label, "Sweeping Blow");
    }

    #[test]
    fn template_covers_every_action_kind() {
        for entry in CATALOG {
            assert_eq!(template(entry.kind).kind, entry.kind);
        }
    }
}

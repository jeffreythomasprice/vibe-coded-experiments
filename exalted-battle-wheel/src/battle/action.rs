use crate::battle::ids::CombatantId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    Sorcery,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredAction {
    pub kind: ActionKind,
    pub label: String,
    pub speed: u32,
    pub dv_penalty: i32,
    pub reflexive: bool,
    pub target: Option<CombatantId>,
    pub note: String,
}

impl ActionTemplate {
    pub fn declare(
        &self,
        speed_override: Option<u32>,
        dv_override: Option<i32>,
        target: Option<CombatantId>,
        note: String,
    ) -> DeclaredAction {
        DeclaredAction {
            kind: self.kind,
            label: self.name.to_string(),
            speed: self.speed.resolve(speed_override),
            dv_penalty: self.dv_penalty.resolve(dv_override),
            reflexive: self.reflexive,
            target,
            note,
        }
    }
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
}

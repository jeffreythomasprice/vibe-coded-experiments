use crate::battle::action::{ActionError, ActionKind, ActionTemplate, DeclaredAction, SpeedRequired, label, note, template};
use crate::battle::mode::BattleMode;

pub use crate::generated::{FlurriedAction, FlurryBreakdown, FlurryDvRule};

/// Whether `template` could appear as one component of a multi-action tick: RULES.md §4.4 (p. 143)
/// bars Aim and Guard from any flurry; Inactive is involuntary rather than chosen; Flurry and
/// JoinBattleInProgress don't nest inside their own cascade; and a reflexive action (Move) costs no
/// time, so it needs no place in one.
pub fn can_flurry(template: &ActionTemplate) -> bool {
    !template.reflexive
        && !matches!(
            template.kind,
            ActionKind::Aim | ActionKind::Guard | ActionKind::Inactive | ActionKind::Flurry | ActionKind::JoinBattleInProgress
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FlurryError {
    #[error("a multi-action needs at least 2 component actions, got {count}")]
    TooFewActions { count: usize },
    #[error("{kind:?} cannot be part of a multi-action")]
    Barred { kind: ActionKind },
    #[error(transparent)]
    Action(#[from] ActionError),
    #[error("a flurry breakdown was given for {kind:?}, which is not Flurry")]
    BreakdownOnNonFlurry { kind: ActionKind },
}

impl FlurryBreakdown {
    pub fn highest_speed(&self) -> u32 {
        self.actions.iter().map(|action| action.speed).max().unwrap_or(0)
    }

    /// RULES.md §4.4, p. 143 (mundane flurry: cumulative) vs. Extra Action Charms, p. 182 (only
    /// the worst DV penalty applies).
    pub fn dv_penalty(&self) -> i32 {
        match self.rule {
            FlurryDvRule::Stacked => self.actions.iter().map(|action| action.dv_penalty).sum(),
            FlurryDvRule::WorstOnly => self.actions.iter().map(|action| action.dv_penalty).min().unwrap_or(0),
        }
    }

    /// The normal multiple-action dice penalty (RULES.md p. 125: -N to the first action, one more
    /// per successive action) — `None` under `WorstOnly`, since Extra Action Charms are explicitly
    /// exempt from it (p. 182: "has no multiple action penalties").
    pub fn dice_penalties(&self) -> Option<Vec<i32>> {
        match self.rule {
            FlurryDvRule::WorstOnly => None,
            FlurryDvRule::Stacked => {
                let n = self.actions.len() as i32;
                Some((0..self.actions.len()).map(|i| -(n + i as i32)).collect())
            }
        }
    }

    pub fn validate(&self, mode: BattleMode) -> Result<(), FlurryError> {
        if self.actions.len() < 2 {
            return Err(FlurryError::TooFewActions { count: self.actions.len() });
        }
        for action in &self.actions {
            let action_template = template(mode, action.kind)?;
            if !can_flurry(action_template) {
                return Err(FlurryError::Barred { kind: action.kind });
            }
        }
        Ok(())
    }
}

/// One row of the client's multi-action builder, before it's resolved into a `FlurriedAction`.
#[derive(Debug, Clone, Copy)]
pub struct FlurryPart {
    pub template: &'static ActionTemplate,
    pub speed: Option<u32>,
    pub dv_penalty: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeclareFlurryError {
    #[error("component {position} has no default Speed; enter one to declare it")]
    SpeedRequired {
        position: usize,
        #[source]
        source: SpeedRequired,
    },
    #[error(transparent)]
    Invalid(#[from] FlurryError),
}

/// Resolves a set of builder rows into one `DeclaredAction` (`kind: Flurry`) carrying its
/// `FlurryBreakdown` — the engine-facing counterpart to `ActionTemplate::declare` for a single
/// action. `speed_override` beats the cascade's highest Speed (RULES.md §4.4, p. 143's own
/// exception for a weapon-draw flurry, or GM fiat); a blank/`None` `name` falls back to the
/// component labels joined with " + ".
pub fn declare_flurry(
    mode: BattleMode,
    parts: &[FlurryPart],
    rule: FlurryDvRule,
    speed_override: Option<u32>,
    name: Option<String>,
) -> Result<DeclaredAction, DeclareFlurryError> {
    let mut actions = Vec::with_capacity(parts.len());
    for (position, part) in parts.iter().enumerate() {
        let speed = part
            .template
            .resolve_speed(part.speed)
            .map_err(|source| DeclareFlurryError::SpeedRequired { position, source })?;
        actions.push(FlurriedAction {
            kind: part.template.kind,
            label: label(part.template.name),
            speed,
            dv_penalty: part.template.dv_penalty.resolve(part.dv_penalty),
        });
    }
    let breakdown = FlurryBreakdown { actions, rule };
    breakdown.validate(mode)?;

    let speed = speed_override.unwrap_or_else(|| breakdown.highest_speed());
    let dv_penalty = breakdown.dv_penalty();
    let label_text = match name {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => breakdown
            .actions
            .iter()
            .map(|action| action.label.to_string())
            .collect::<Vec<_>>()
            .join(" + "),
    };

    Ok(DeclaredAction {
        kind: ActionKind::Flurry,
        label: label(label_text),
        speed,
        dv_penalty,
        reflexive: false,
        target: None,
        note: note(""),
        effects: Vec::new(),
        flurry: Some(breakdown),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::action::template as lookup;

    fn personal(kind: ActionKind) -> &'static ActionTemplate {
        lookup(BattleMode::Personal, kind).expect("personal catalog")
    }

    fn attack_part(speed: u32) -> FlurryPart {
        FlurryPart {
            template: personal(ActionKind::Attack),
            speed: Some(speed),
            dv_penalty: None,
        }
    }

    fn dash_part() -> FlurryPart {
        FlurryPart {
            template: personal(ActionKind::Dash),
            speed: None,
            dv_penalty: None,
        }
    }

    #[test]
    fn stacked_dv_sums_every_component() {
        let action = declare_flurry(
            BattleMode::Personal,
            &[attack_part(5), attack_part(5), dash_part()],
            FlurryDvRule::Stacked,
            None,
            None,
        )
        .unwrap();
        assert_eq!(action.dv_penalty, -1 + -1 + -2);
        assert_eq!(action.speed, 5);
        assert_eq!(action.kind, ActionKind::Flurry);
    }

    #[test]
    fn worst_only_dv_picks_the_minimum() {
        let action = declare_flurry(
            BattleMode::Personal,
            &[attack_part(5), dash_part()],
            FlurryDvRule::WorstOnly,
            None,
            None,
        )
        .unwrap();
        assert_eq!(action.dv_penalty, -2);
    }

    #[test]
    fn dice_penalties_are_cumulative_under_stacked_and_absent_under_worst_only() {
        let breakdown = FlurryBreakdown {
            actions: vec![
                FlurriedAction {
                    kind: ActionKind::Attack,
                    label: label("Attack"),
                    speed: 5,
                    dv_penalty: -1,
                },
                FlurriedAction {
                    kind: ActionKind::Attack,
                    label: label("Attack"),
                    speed: 5,
                    dv_penalty: -1,
                },
                FlurriedAction {
                    kind: ActionKind::Dash,
                    label: label("Dash"),
                    speed: 3,
                    dv_penalty: -2,
                },
            ],
            rule: FlurryDvRule::Stacked,
        };
        assert_eq!(breakdown.dice_penalties(), Some(vec![-3, -4, -5]));
        let worst_only = FlurryBreakdown {
            rule: FlurryDvRule::WorstOnly,
            ..breakdown
        };
        assert_eq!(worst_only.dice_penalties(), None);
    }

    #[test]
    fn speed_override_beats_the_highest_component_speed() {
        let action = declare_flurry(
            BattleMode::Personal,
            &[attack_part(5), dash_part()],
            FlurryDvRule::Stacked,
            Some(3),
            None,
        )
        .unwrap();
        assert_eq!(action.speed, 3);
    }

    #[test]
    fn aim_and_guard_are_barred_from_a_multi_action() {
        for kind in [ActionKind::Aim, ActionKind::Guard] {
            let part = FlurryPart {
                template: personal(kind),
                speed: None,
                dv_penalty: None,
            };
            let err = declare_flurry(BattleMode::Personal, &[attack_part(5), part], FlurryDvRule::Stacked, None, None).unwrap_err();
            assert_eq!(err, DeclareFlurryError::Invalid(FlurryError::Barred { kind }));
        }
    }

    #[test]
    fn move_is_barred_because_it_is_reflexive() {
        let part = FlurryPart {
            template: personal(ActionKind::Move),
            speed: None,
            dv_penalty: None,
        };
        let err = declare_flurry(BattleMode::Personal, &[attack_part(5), part], FlurryDvRule::Stacked, None, None).unwrap_err();
        assert_eq!(err, DeclareFlurryError::Invalid(FlurryError::Barred { kind: ActionKind::Move }));
    }

    #[test]
    fn fewer_than_two_actions_is_rejected() {
        let err = declare_flurry(BattleMode::Personal, &[attack_part(5)], FlurryDvRule::Stacked, None, None).unwrap_err();
        assert_eq!(err, DeclareFlurryError::Invalid(FlurryError::TooFewActions { count: 1 }));
    }

    #[test]
    fn a_missing_required_speed_names_its_position() {
        let part = FlurryPart {
            template: personal(ActionKind::Attack),
            speed: None,
            dv_penalty: None,
        };
        let err = declare_flurry(BattleMode::Personal, &[dash_part(), part], FlurryDvRule::Stacked, None, None).unwrap_err();
        assert_eq!(
            err,
            DeclareFlurryError::SpeedRequired {
                position: 1,
                source: SpeedRequired { name: "Attack" },
            }
        );
    }

    #[test]
    fn a_blank_name_joins_the_component_labels() {
        let action = declare_flurry(
            BattleMode::Personal,
            &[attack_part(5), dash_part()],
            FlurryDvRule::Stacked,
            None,
            None,
        )
        .unwrap();
        assert_eq!(action.label.to_string(), "Attack + Dash");
    }
}

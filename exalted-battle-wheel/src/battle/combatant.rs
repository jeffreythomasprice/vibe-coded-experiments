use crate::battle::ids::{CombatantId, Tick};
use crate::battle::sequence::Sequence;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Side(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinBattleResult {
    Successes(u32),
    Botch,
}

impl JoinBattleResult {
    /// Speed used to schedule this result against a scene's reaction count
    /// (RULES.md §2.2, p. 141): `clamp(reaction_count - successes, 0, 6)`, or 6 on a botch.
    /// Also used for Join Battle in progress (§4.7, p. 144), which is the same formula.
    pub fn speed(self, reaction_count: u32) -> u32 {
        match self {
            JoinBattleResult::Botch => 6,
            JoinBattleResult::Successes(successes) => reaction_count.saturating_sub(successes).min(6),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CombatantState {
    Normal,
    Guarding,
    Aiming { target: Option<CombatantId> },
    Inactive,
    InSequence(Sequence),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DvState {
    pub penalty: i32,
    pub refreshes_at: Option<Tick>,
}

/// What the combatant is currently committed to: the action whose Speed is holding her off the
/// wheel until `next_action_tick`. Declaring an action resolves it immediately (state.rs), so
/// without this the battle keeps only the tick and DV it left behind and cannot say what she's
/// doing. Sequences don't use this — `CombatantState::InSequence` already carries the step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commitment {
    pub label: String,
    pub speed: u32,
    pub declared_at: Tick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combatant {
    pub id: CombatantId,
    pub name: String,
    pub side: Side,
    pub join_battle: JoinBattleResult,
    pub next_action_tick: Tick,
    pub state: CombatantState,
    pub dv: DvState,
    pub commitment: Option<Commitment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_battle_speed_clamps_to_six() {
        assert_eq!(JoinBattleResult::Successes(0).speed(8), 6);
    }

    #[test]
    fn join_battle_speed_clamps_to_zero() {
        assert_eq!(JoinBattleResult::Successes(5).speed(3), 0);
    }

    #[test]
    fn join_battle_botch_is_always_six() {
        assert_eq!(JoinBattleResult::Botch.speed(0), 6);
        assert_eq!(JoinBattleResult::Botch.speed(10), 6);
    }

    #[test]
    fn fastest_successes_land_on_tick_zero() {
        assert_eq!(JoinBattleResult::Successes(5).speed(5), 0);
    }
}

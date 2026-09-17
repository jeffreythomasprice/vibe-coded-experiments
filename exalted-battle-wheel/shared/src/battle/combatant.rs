use crate::battle::ids::{CombatantId, Tick};

pub use crate::generated::{CombatantState, Commitment, DvState, JoinBattleResult, Side};

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

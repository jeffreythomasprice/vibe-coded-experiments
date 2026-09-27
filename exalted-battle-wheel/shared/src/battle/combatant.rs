use crate::battle::action::{DeclaredAction, truncate_chars};
use crate::battle::ids::{CombatantId, Tick};
use crate::battle::sequence::Sequence;

pub use crate::generated::{CombatantName, CombatantState, Commitment, DvState, JoinBattleResult, Side};

/// The most recent non-reflexive action or sequence this combatant declared, kept so the action
/// panel can offer it again next time she's up. A reflexive action (Move, Disengage, ...) never
/// ends a turn, so it never overwrites this — see `apply_declare_action` in `state.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LastDeclared {
    Action(DeclaredAction),
    Sequence(Sequence),
}

/// Mirrors `CombatantName`'s `maxLength` in `shared/schemas/common.json` -- a test asserts the two
/// stay equal. Free-form text is truncated to fit rather than rejected, the same policy
/// `label()`/`note()` and `protocol::name::sanitize_name` use for other user-typed text.
pub const MAX_COMBATANT_NAME_LEN: usize = 250;

/// Truncates to `CombatantName`'s bound rather than rejecting -- see `MAX_COMBATANT_NAME_LEN`.
pub fn combatant_name(text: impl AsRef<str>) -> CombatantName {
    CombatantName::try_from(truncate_chars(text.as_ref(), MAX_COMBATANT_NAME_LEN)).expect("truncated to fit CombatantName's bound")
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
pub struct Combatant {
    pub id: CombatantId,
    pub name: String,
    pub side: Side,
    pub join_battle: JoinBattleResult,
    pub next_action_tick: Tick,
    pub state: CombatantState,
    pub dv: DvState,
    pub commitment: Option<Commitment>,
    pub last_declared: Option<LastDeclared>,
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

    /// `CombatantName` is generated from `shared/schemas/common.json`'s `maxLength`;
    /// `MAX_COMBATANT_NAME_LEN` must never drift from that bound, since `combatant_name()`'s
    /// truncation assumes they match exactly. Same reasoning as `action.rs`'s equivalent test for
    /// `Label`/`Note`.
    #[test]
    fn combatant_name_bound_matches_the_local_constant() {
        assert!(CombatantName::try_from("a".repeat(MAX_COMBATANT_NAME_LEN)).is_ok());
        assert!(CombatantName::try_from("a".repeat(MAX_COMBATANT_NAME_LEN + 1)).is_err());
    }

    #[test]
    fn combatant_name_truncates_rather_than_rejecting() {
        let long = "a".repeat(MAX_COMBATANT_NAME_LEN + 10);
        assert_eq!(combatant_name(&long).chars().count(), MAX_COMBATANT_NAME_LEN);
    }
}

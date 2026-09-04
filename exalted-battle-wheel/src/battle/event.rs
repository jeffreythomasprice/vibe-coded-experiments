use crate::battle::action::DeclaredAction;
use crate::battle::combatant::{JoinBattleResult, Side};
use crate::battle::ids::{CombatantId, MarkerId, Tick};
use crate::battle::sequence::Sequence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterruptReason {
    FailedOccultCheck,
    WentInactive,
    Voluntary,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BattleEvent {
    AddCombatant {
        id: CombatantId,
        name: String,
        side: Side,
        join_battle: JoinBattleResult,
    },
    RemoveCombatant {
        id: CombatantId,
    },
    StartBattle,
    DeclareAction {
        actor: CombatantId,
        action: DeclaredAction,
    },
    StartSequence {
        actor: CombatantId,
        sequence: Sequence,
    },
    AdvanceSequence {
        actor: CombatantId,
        speed_override: Option<u32>,
    },
    InterruptSequence {
        actor: CombatantId,
        reason: InterruptReason,
        rejoin: JoinBattleResult,
    },
    AdvanceTick,
    AddMarker {
        id: MarkerId,
        label: String,
        source: CombatantId,
        at_tick: Tick,
    },
    RemoveMarker {
        id: MarkerId,
    },
}

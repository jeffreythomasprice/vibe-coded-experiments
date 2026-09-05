use crate::battle::action::DeclaredAction;
use crate::battle::combatant::{Commitment, CombatantState, DvState, JoinBattleResult, Side};
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
        ticks: u32,
    },
    RemoveMarker {
        id: MarkerId,
    },
    /// The escape hatch: a full-override correction to a combatant's queue state, appended as a
    /// normal event so Undo/Redo covers it for free (see `BattleLog`). Every field is set, not
    /// patched, so one user edit is exactly one event.
    ReviseCombatant {
        actor: CombatantId,
        next_action_tick: Tick,
        state: CombatantState,
        dv: DvState,
        commitment: Option<Commitment>,
        note: String,
    },
    ReviseMarker {
        id: MarkerId,
        label: String,
        at_tick: Tick,
        ticks: u32,
    },
}

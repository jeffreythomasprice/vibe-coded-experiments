use crate::battle::action::DeclaredAction;
use crate::battle::combatant::{Commitment, CombatantState, DvState, JoinBattleResult, Side};
use crate::battle::ids::{CombatantId, MarkerId, Tick};
use crate::battle::mode::BattleMode;
use crate::battle::sequence::Sequence;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterruptReason {
    FailedOccultCheck,
    WentInactive,
    Voluntary,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BattleEvent {
    /// Chosen during Setup and frozen by `StartBattle`, exactly like the reaction count: the mode
    /// decides which catalog is legal and how long a tick is, so it cannot change once actions are
    /// already on the wheel (RULES.md §11, pp. 158, 169).
    SetMode {
        mode: BattleMode,
    },
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

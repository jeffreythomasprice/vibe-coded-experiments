use crate::battle::action::ActionError;
use crate::battle::ids::{CombatantId, MarkerId, Tick};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BattleError {
    #[error(transparent)]
    Action(#[from] ActionError),
    #[error("no combatant with id {0:?}")]
    UnknownCombatant(CombatantId),
    #[error("no marker with id {0:?}")]
    UnknownMarker(MarkerId),
    #[error("marker {0:?} must span at least one tick")]
    MarkerDurationZero(MarkerId),
    #[error("a marker with id {0:?} already exists")]
    DuplicateMarker(MarkerId),
    #[error("a combatant with id {0:?} already exists")]
    DuplicateCombatant(CombatantId),
    #[error("the battle has not started yet")]
    NotYetStarted,
    #[error("the battle has already started")]
    AlreadyStarted,
    #[error("{actor:?} cannot act yet: next action is tick {next}, current tick is {current}")]
    NotThisCombatantsTick { actor: CombatantId, next: Tick, current: Tick },
    #[error("{0:?} is already in a multi-action sequence")]
    SequenceAlreadyInProgress(CombatantId),
    #[error("{0:?} cannot start a sequence with no steps")]
    EmptySequence(CombatantId),
    #[error("{0:?} is not in a sequence")]
    NoSequenceInProgress(CombatantId),
    #[error("{actor:?}'s revised sequence step {step} is out of range for its {steps}-step sequence")]
    SequenceStepOutOfRange { actor: CombatantId, step: usize, steps: usize },
    #[error("cannot advance the tick: {0:?} still need to act")]
    CombatantsPendingAction(Vec<CombatantId>),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("cannot seek to {requested}: the log has {len} events")]
    CursorOutOfRange { requested: usize, len: usize },
}

/// Deserializing a `BattleLog` (`log.rs`) has never been through `push`'s validation, so a value
/// arriving from storage needs a check `apply`'s panicking `.expect()` in `battle()` cannot do
/// for us: every event must still replay, and the id counters must still be ahead of every id the
/// log already uses.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RestoreError {
    #[error("the saved cursor is {cursor} but the log has {len} events")]
    CursorOutOfRange { cursor: usize, len: usize },
    #[error("saved event {index} no longer replays: {source}")]
    Unreplayable { index: usize, #[source] source: BattleError },
    #[error("the saved combatant id counter is {counter}, but the log already uses {used:?}")]
    StaleCombatantCounter { counter: u32, used: CombatantId },
    #[error("the saved marker id counter is {counter}, but the log already uses {used:?}")]
    StaleMarkerCounter { counter: u32, used: MarkerId },
}

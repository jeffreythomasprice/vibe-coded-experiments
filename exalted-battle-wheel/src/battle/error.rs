use crate::battle::ids::{CombatantId, MarkerId, Tick};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BattleError {
    #[error("no combatant with id {0:?}")]
    UnknownCombatant(CombatantId),
    #[error("no marker with id {0:?}")]
    UnknownMarker(MarkerId),
    #[error("the battle has not started yet")]
    NotYetStarted,
    #[error("the battle has already started")]
    AlreadyStarted,
    #[error("{actor:?} cannot act yet: next action is tick {next}, current tick is {current}")]
    NotThisCombatantsTick { actor: CombatantId, next: Tick, current: Tick },
    #[error("{0:?} is already in a multi-action sequence")]
    SequenceAlreadyInProgress(CombatantId),
    #[error("{0:?} is not in a sequence")]
    NoSequenceInProgress(CombatantId),
    #[error("cannot advance the tick: {0:?} still need to act")]
    CombatantsPendingAction(Vec<CombatantId>),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
}

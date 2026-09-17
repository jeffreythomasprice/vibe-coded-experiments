use crate::battle::{BattleError, BattleLog};

/// What a caller asks for. `PushMinting` carries an event with placeholder ids (`CombatantId(0)`,
/// `MarkerId(0)`, ...) for whatever it mints — only whoever is authoritative for the room turns
/// those into real ones, by stamping them from its own log via `BattleLog::restamp`. Plain `Push`
/// is for events that mint nothing, or that intentionally carry ids minted earlier by another
/// event (`ReviseCombatant`'s `InSequence` case clones an existing sequence's effect ids).
///
/// Defined in `shared/schemas/ws.json`; see `shared/schemas/README.md`.
pub use crate::generated::BattleRequest;

/// The concrete instruction every node applies identically — never a placeholder id in sight.
pub use crate::generated::BattleCommand;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BattleSyncError {
    #[error(transparent)]
    Battle(#[from] BattleError),
}

/// Applies `command` to `log` in place. The one place a `BattleCommand` is actually turned into a
/// mutation — shared by whichever node sequences requests into commands and by everyone else
/// replaying them, so client and server can never disagree about what a command does.
pub fn apply_command(log: &mut BattleLog, command: &BattleCommand) -> Result<(), BattleError> {
    match command {
        BattleCommand::Push(event) => log.push(event.clone()),
        BattleCommand::Undo => log.undo(),
        BattleCommand::Redo => log.redo(),
        BattleCommand::Seek(cursor) => log.seek(*cursor),
        BattleCommand::Reset => {
            *log = BattleLog::new();
            Ok(())
        }
    }
}

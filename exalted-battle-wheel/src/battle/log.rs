use crate::battle::error::BattleError;
use crate::battle::event::BattleEvent;
use crate::battle::ids::{CombatantId, MarkerId};
use crate::battle::state::{apply, Battle};

/// Event-sourced battle state. `Battle` is always derived by replaying `events[..cursor]` from
/// genesis; undo/redo just moves the cursor, and pushing a new event truncates any redo tail.
/// Battles are small (a handful of combatants, a few hundred events), so a full replay is cheap
/// and there is no drift between the log and the derived state by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BattleLog {
    events: Vec<BattleEvent>,
    cursor: usize,
    next_combatant_id: u32,
    next_marker_id: u32,
}

impl BattleLog {
    pub fn new() -> Self {
        Self { events: Vec::new(), cursor: 0, next_combatant_id: 0, next_marker_id: 0 }
    }

    pub fn battle(&self) -> Battle {
        let mut battle = Battle::genesis();
        for event in &self.events[..self.cursor] {
            apply(&mut battle, event).expect("logged events must have been valid when pushed");
        }
        battle
    }

    pub fn push(&mut self, event: BattleEvent) -> Result<(), BattleError> {
        let mut battle = self.battle();
        apply(&mut battle, &event)?;
        self.events.truncate(self.cursor);
        self.events.push(event);
        self.cursor += 1;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<(), BattleError> {
        if self.cursor == 0 {
            return Err(BattleError::NothingToUndo);
        }
        self.cursor -= 1;
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), BattleError> {
        if self.cursor == self.events.len() {
            return Err(BattleError::NothingToRedo);
        }
        self.cursor += 1;
        Ok(())
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor < self.events.len()
    }

    pub fn events(&self) -> &[BattleEvent] {
        &self.events
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn seek(&mut self, cursor: usize) -> Result<(), BattleError> {
        if cursor > self.events.len() {
            return Err(BattleError::CursorOutOfRange { requested: cursor, len: self.events.len() });
        }
        self.cursor = cursor;
        Ok(())
    }

    pub fn alloc_combatant_id(&mut self) -> CombatantId {
        let id = CombatantId(self.next_combatant_id);
        self.next_combatant_id += 1;
        id
    }

    pub fn alloc_marker_id(&mut self) -> MarkerId {
        let id = MarkerId(self.next_marker_id);
        self.next_marker_id += 1;
        id
    }
}

impl Default for BattleLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::combatant::{JoinBattleResult, Side};

    fn add_event(log: &mut BattleLog, successes: u32) -> CombatantId {
        let id = log.alloc_combatant_id();
        log.push(BattleEvent::AddCombatant {
            id,
            name: format!("C{}", id.0),
            side: Side("A".to_string()),
            join_battle: JoinBattleResult::Successes(successes),
        })
        .unwrap();
        id
    }

    #[test]
    fn undo_returns_to_genesis_and_redo_restores() {
        let mut log = BattleLog::new();
        add_event(&mut log, 5);
        add_event(&mut log, 2);
        log.push(BattleEvent::StartBattle).unwrap();
        let started = log.battle();
        assert_eq!(started.combatants.len(), 2);

        log.undo().unwrap();
        log.undo().unwrap();
        log.undo().unwrap();
        assert_eq!(log.battle(), Battle::genesis());
        assert!(!log.can_undo());

        log.redo().unwrap();
        log.redo().unwrap();
        log.redo().unwrap();
        assert_eq!(log.battle(), started);
        assert!(!log.can_redo());
    }

    #[test]
    fn pushing_after_undo_truncates_the_redo_tail() {
        let mut log = BattleLog::new();
        add_event(&mut log, 5);
        add_event(&mut log, 2);
        log.undo().unwrap();
        assert!(log.can_redo());

        add_event(&mut log, 9);
        assert!(!log.can_redo());
        assert_eq!(log.battle().combatants.len(), 2);
    }

    #[test]
    fn invalid_event_is_rejected_without_mutating_the_log() {
        let mut log = BattleLog::new();
        let events_len_before = log.events().len();
        let err = log.push(BattleEvent::StartSequence {
            actor: CombatantId(999),
            sequence: crate::battle::sequence::Sequence::shape_terrestrial(),
        });
        assert!(err.is_err());
        assert_eq!(log.events().len(), events_len_before);
    }

    #[test]
    fn allocated_ids_never_repeat_even_across_undo() {
        let mut log = BattleLog::new();
        let first = log.alloc_combatant_id();
        log.undo().unwrap_err();
        let second = log.alloc_combatant_id();
        assert_ne!(first, second);
    }

    #[test]
    fn seek_moves_the_cursor_in_both_directions() {
        let mut log = BattleLog::new();
        add_event(&mut log, 5);
        add_event(&mut log, 2);
        log.push(BattleEvent::StartBattle).unwrap();

        log.seek(1).unwrap();
        assert_eq!(log.battle().combatants.len(), 1);

        log.seek(3).unwrap();
        assert_eq!(log.battle().combatants.len(), 2);
        assert!(matches!(log.battle().phase, crate::battle::state::Phase::Running { .. }));
    }

    #[test]
    fn seek_past_the_end_is_rejected() {
        let mut log = BattleLog::new();
        add_event(&mut log, 5);
        let err = log.seek(5);
        assert_eq!(err, Err(BattleError::CursorOutOfRange { requested: 5, len: 1 }));
        assert_eq!(log.cursor(), 1);
    }

    #[test]
    fn seek_preserves_the_redo_tail() {
        let mut log = BattleLog::new();
        add_event(&mut log, 5);
        add_event(&mut log, 2);
        add_event(&mut log, 9);

        log.seek(1).unwrap();
        assert!(log.can_redo());
        assert_eq!(log.events().len(), 3);
    }

    #[test]
    fn pushing_after_seek_truncates_the_tail() {
        let mut log = BattleLog::new();
        add_event(&mut log, 5);
        add_event(&mut log, 2);

        log.seek(1).unwrap();
        add_event(&mut log, 9);

        assert_eq!(log.events().len(), 2);
        assert!(!log.can_redo());
    }
}

use crate::battle::combatant::CombatantState;
use crate::battle::error::{BattleError, RestoreError};
use crate::battle::event::BattleEvent;
use crate::battle::ids::{CombatantId, MarkerId};
use crate::battle::state::{apply, Battle};
use serde::{Deserialize, Serialize};

/// Event-sourced battle state. `Battle` is always derived by replaying `events[..cursor]` from
/// genesis; undo/redo just moves the cursor, and pushing a new event truncates any redo tail.
/// Battles are small (a handful of combatants, a few hundred events), so a full replay is cheap
/// and there is no drift between the log and the derived state by construction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RestoredLog")]
pub struct BattleLog {
    events: Vec<BattleEvent>,
    cursor: usize,
    next_combatant_id: u32,
    next_marker_id: u32,
}

/// The combatant id an `AddCombatant` event mints, if any — every other variant only ever
/// references ids minted elsewhere.
fn minted_combatant_id(event: &BattleEvent) -> Option<CombatantId> {
    match event {
        BattleEvent::AddCombatant { id, .. } => Some(*id),
        BattleEvent::SetMode { .. }
        | BattleEvent::RemoveCombatant { .. }
        | BattleEvent::StartBattle
        | BattleEvent::DeclareAction { .. }
        | BattleEvent::StartSequence { .. }
        | BattleEvent::AdvanceSequence { .. }
        | BattleEvent::InterruptSequence { .. }
        | BattleEvent::AdvanceTick
        | BattleEvent::AddMarker { .. }
        | BattleEvent::RemoveMarker { .. }
        | BattleEvent::ReviseCombatant { .. }
        | BattleEvent::ReviseMarker { .. } => None,
    }
}

/// Every marker id an event mints. Beyond the obvious `AddMarker`, a declared action or a started
/// sequence carries its own `effects: Vec<DeclaredEffect>` and mints one id per effect
/// (`spawn_effects` in `state.rs`); `ReviseCombatant` can install a fresh `InSequence` state
/// (it's the escape hatch, not gated by having gone through `StartSequence`), so its effects count
/// too.
fn minted_marker_ids(event: &BattleEvent) -> Vec<MarkerId> {
    match event {
        BattleEvent::AddMarker { id, .. } => vec![*id],
        BattleEvent::DeclareAction { action, .. } => action.effects.iter().map(|effect| effect.id).collect(),
        BattleEvent::StartSequence { sequence, .. } => sequence.effects.iter().map(|effect| effect.id).collect(),
        BattleEvent::ReviseCombatant { state, .. } => match state {
            CombatantState::InSequence(sequence) => sequence.effects.iter().map(|effect| effect.id).collect(),
            CombatantState::Normal | CombatantState::Guarding | CombatantState::Aiming { .. } | CombatantState::Inactive => Vec::new(),
        },
        BattleEvent::SetMode { .. }
        | BattleEvent::AddCombatant { .. }
        | BattleEvent::RemoveCombatant { .. }
        | BattleEvent::StartBattle
        | BattleEvent::AdvanceSequence { .. }
        | BattleEvent::InterruptSequence { .. }
        | BattleEvent::AdvanceTick
        | BattleEvent::RemoveMarker { .. }
        | BattleEvent::ReviseMarker { .. } => Vec::new(),
    }
}

/// Mirrors `BattleLog`'s fields so deserializing can validate before committing to them (see the
/// `#[serde(try_from)]` on `BattleLog`). `battle()` `.expect()`s that every logged event was valid
/// when pushed — an invariant `push` maintains by construction, but a value decoded from storage
/// has never been through `push`.
#[derive(Deserialize)]
struct RestoredLog {
    events: Vec<BattleEvent>,
    cursor: usize,
    next_combatant_id: u32,
    next_marker_id: u32,
}

impl TryFrom<RestoredLog> for BattleLog {
    type Error = RestoreError;

    fn try_from(restored: RestoredLog) -> Result<Self, Self::Error> {
        let RestoredLog { events, cursor, next_combatant_id, next_marker_id } = restored;

        if cursor > events.len() {
            return Err(RestoreError::CursorOutOfRange { cursor, len: events.len() });
        }

        let mut battle = Battle::genesis();
        for (index, event) in events.iter().enumerate() {
            apply(&mut battle, event).map_err(|source| RestoreError::Unreplayable { index, source })?;

            if let Some(id) = minted_combatant_id(event)
                && id.0 >= next_combatant_id
            {
                return Err(RestoreError::StaleCombatantCounter { counter: next_combatant_id, used: id });
            }
            for id in minted_marker_ids(event) {
                if id.0 >= next_marker_id {
                    return Err(RestoreError::StaleMarkerCounter { counter: next_marker_id, used: id });
                }
            }
        }

        Ok(BattleLog { events, cursor, next_combatant_id, next_marker_id })
    }
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
    use crate::battle::action::{template, ActionKind, Declaration, DeclaredEffect};
    use crate::battle::combatant::{CombatantState, DvState, JoinBattleResult, Side};
    use crate::battle::event::InterruptReason;
    use crate::battle::mode::BattleMode;
    use crate::battle::sequence::Sequence;

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
    fn revising_an_action_then_undo_restores_the_original_tick_and_redo_reapplies_it() {
        use crate::battle::action::{template, ActionKind, Declaration};
        use crate::battle::combatant::{CombatantState, DvState};
        use crate::battle::mode::BattleMode;

        let mut log = BattleLog::new();
        let id = add_event(&mut log, 5);
        log.push(BattleEvent::StartBattle).unwrap();
        let attack = template(BattleMode::Personal, ActionKind::Attack).unwrap().declare(Declaration { speed: Some(5), ..Default::default() });
        log.push(BattleEvent::DeclareAction { actor: id, action: attack }).unwrap();
        assert_eq!(log.battle().find(id).unwrap().next_action_tick, 5);

        log.push(BattleEvent::ReviseCombatant {
            actor: id,
            next_action_tick: 2,
            state: CombatantState::Normal,
            dv: DvState { penalty: -1, refreshes_at: Some(2) },
            commitment: None,
            note: "retconned to resolve sooner".to_string(),
        })
        .unwrap();
        assert_eq!(log.battle().find(id).unwrap().next_action_tick, 2);

        log.undo().unwrap();
        assert_eq!(log.battle().find(id).unwrap().next_action_tick, 5, "undo should restore the pre-revision tick");

        log.redo().unwrap();
        assert_eq!(log.battle().find(id).unwrap().next_action_tick, 2, "redo should reapply the revision");
    }

    #[test]
    fn revising_a_tick_backward_replays_every_prefix_without_panicking() {
        use crate::battle::combatant::{CombatantState, DvState};

        let mut log = BattleLog::new();
        let id = add_event(&mut log, 5);
        log.push(BattleEvent::StartBattle).unwrap();
        log.push(BattleEvent::ReviseCombatant {
            actor: id,
            next_action_tick: 0,
            state: CombatantState::Normal,
            dv: DvState::default(),
            commitment: None,
            note: String::new(),
        })
        .unwrap();

        // `battle()` replays events[..cursor] from genesis on every call and `.expect()`s that
        // each logged event is still valid; this must not panic at any cursor position.
        for cursor in 0..=log.events().len() {
            log.seek(cursor).unwrap();
            let _ = log.battle();
        }
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

    /// Every `BattleEvent` variant, by name. The match is exhaustive and wildcard-free so adding
    /// a variant to `event.rs` fails this to compile until it's added here too.
    fn kind(event: &BattleEvent) -> &'static str {
        match event {
            BattleEvent::SetMode { .. } => "SetMode",
            BattleEvent::AddCombatant { .. } => "AddCombatant",
            BattleEvent::RemoveCombatant { .. } => "RemoveCombatant",
            BattleEvent::StartBattle => "StartBattle",
            BattleEvent::DeclareAction { .. } => "DeclareAction",
            BattleEvent::StartSequence { .. } => "StartSequence",
            BattleEvent::AdvanceSequence { .. } => "AdvanceSequence",
            BattleEvent::InterruptSequence { .. } => "InterruptSequence",
            BattleEvent::AdvanceTick => "AdvanceTick",
            BattleEvent::AddMarker { .. } => "AddMarker",
            BattleEvent::RemoveMarker { .. } => "RemoveMarker",
            BattleEvent::ReviseCombatant { .. } => "ReviseCombatant",
            BattleEvent::ReviseMarker { .. } => "ReviseMarker",
        }
    }

    #[test]
    fn round_trips_every_event_variant_through_json() {
        const ALL_KINDS: &[&str] = &[
            "SetMode",
            "AddCombatant",
            "RemoveCombatant",
            "StartBattle",
            "DeclareAction",
            "StartSequence",
            "AdvanceSequence",
            "InterruptSequence",
            "AdvanceTick",
            "AddMarker",
            "RemoveMarker",
            "ReviseCombatant",
            "ReviseMarker",
        ];

        let mut log = BattleLog::new();
        log.push(BattleEvent::SetMode { mode: BattleMode::Personal }).unwrap();
        let a = add_event(&mut log, 5);
        let stale_b = add_event(&mut log, 2);
        log.push(BattleEvent::RemoveCombatant { id: stale_b }).unwrap();
        let b = add_event(&mut log, 2);
        log.push(BattleEvent::StartBattle).unwrap();

        let effect_id = log.alloc_marker_id();
        let attack = template(BattleMode::Personal, ActionKind::Attack)
            .unwrap()
            .declare(Declaration {
                speed: Some(5),
                effects: vec![DeclaredEffect { id: effect_id, label: "Bleed".to_string(), delay: 1, ticks: 2 }],
                ..Default::default()
            });
        log.push(BattleEvent::DeclareAction { actor: a, action: attack }).unwrap();
        log.push(BattleEvent::ReviseCombatant {
            actor: a,
            next_action_tick: 100,
            state: CombatantState::Guarding,
            dv: DvState { penalty: -1, refreshes_at: Some(100) },
            commitment: None,
            note: "parked while b resolves its sorcery".to_string(),
        })
        .unwrap();

        for _ in 0..3 {
            log.push(BattleEvent::AdvanceTick).unwrap();
        }
        log.push(BattleEvent::StartSequence { actor: b, sequence: Sequence::shape_terrestrial() }).unwrap();
        for _ in 0..5 {
            log.push(BattleEvent::AdvanceTick).unwrap();
        }
        log.push(BattleEvent::AdvanceSequence { actor: b, speed_override: Some(4) }).unwrap();
        log.push(BattleEvent::InterruptSequence {
            actor: b,
            reason: InterruptReason::Voluntary,
            rejoin: JoinBattleResult::Successes(1),
        })
        .unwrap();

        let marker_id = log.alloc_marker_id();
        log.push(BattleEvent::AddMarker { id: marker_id, label: "Window".to_string(), source: a, at_tick: 0, ticks: 3 }).unwrap();
        log.push(BattleEvent::ReviseMarker { id: marker_id, label: "Wider window".to_string(), at_tick: 1, ticks: 4 }).unwrap();
        log.push(BattleEvent::RemoveMarker { id: marker_id }).unwrap();

        log.undo().unwrap();
        log.undo().unwrap();

        let observed: std::collections::HashSet<&str> = log.events().iter().map(kind).collect();
        assert_eq!(observed, ALL_KINDS.iter().copied().collect(), "must exercise every BattleEvent variant");

        let json = serde_json::to_string(&log).unwrap();
        let restored: BattleLog = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, log);
        assert_eq!(restored.battle(), log.battle());
        assert!(restored.can_redo(), "the redo tail must survive the round trip");
    }

    #[test]
    fn a_fresh_log_serializes_to_a_stable_default() {
        // Load-or-default (`persist.rs`'s save effect) compares this JSON against a fresh
        // encoding to decide whether anything needs to be written at all.
        let json = serde_json::to_string(&BattleLog::new()).unwrap();
        assert_eq!(json, serde_json::to_string(&BattleLog::default()).unwrap());
        assert_eq!(serde_json::from_str::<BattleLog>(&json).unwrap(), BattleLog::new());
    }

    #[test]
    fn restoring_rejects_a_cursor_past_the_end() {
        let restored = RestoredLog { events: Vec::new(), cursor: 1, next_combatant_id: 0, next_marker_id: 0 };
        let error = BattleLog::try_from(restored).unwrap_err();
        assert_eq!(error, RestoreError::CursorOutOfRange { cursor: 1, len: 0 });
    }

    #[test]
    fn restoring_rejects_an_event_that_no_longer_replays() {
        let events =
            vec![BattleEvent::StartSequence { actor: CombatantId(999), sequence: Sequence::shape_terrestrial() }];
        let restored = RestoredLog { events, cursor: 1, next_combatant_id: 0, next_marker_id: 0 };
        let error = BattleLog::try_from(restored).unwrap_err();
        assert!(matches!(error, RestoreError::Unreplayable { index: 0, .. }));
    }

    #[test]
    fn restoring_rejects_a_stale_combatant_counter() {
        let events = vec![BattleEvent::AddCombatant {
            id: CombatantId(2),
            name: "C2".to_string(),
            side: Side("A".to_string()),
            join_battle: JoinBattleResult::Successes(0),
        }];
        let restored = RestoredLog { events, cursor: 1, next_combatant_id: 2, next_marker_id: 0 };
        let error = BattleLog::try_from(restored).unwrap_err();
        assert_eq!(error, RestoreError::StaleCombatantCounter { counter: 2, used: CombatantId(2) });
    }

    #[test]
    fn restoring_rejects_a_stale_marker_counter() {
        let events = vec![
            BattleEvent::AddCombatant {
                id: CombatantId(0),
                name: "C0".to_string(),
                side: Side("A".to_string()),
                join_battle: JoinBattleResult::Successes(0),
            },
            BattleEvent::StartBattle,
            BattleEvent::AddMarker { id: MarkerId(3), label: "Window".to_string(), source: CombatantId(0), at_tick: 0, ticks: 1 },
        ];
        let restored = RestoredLog { events, cursor: 3, next_combatant_id: 1, next_marker_id: 3 };
        let error = BattleLog::try_from(restored).unwrap_err();
        assert_eq!(error, RestoreError::StaleMarkerCounter { counter: 3, used: MarkerId(3) });
    }
}

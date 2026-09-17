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

/// Raises the id watermarks past whatever `event` already carries, so any concrete event —
/// whether built locally via `alloc_combatant_id`/`alloc_marker_id`, or arriving pre-stamped from
/// a networked peer that allocated against a different log — always leaves the counters ahead of
/// every id actually in the log, which `RestoredLog`'s validation depends on.
fn raise_watermarks(event: &BattleEvent, next_combatant_id: &mut u32, next_marker_id: &mut u32) {
    if let Some(id) = minted_combatant_id(event) {
        *next_combatant_id = (*next_combatant_id).max(id.0 + 1);
    }
    for id in minted_marker_ids(event) {
        *next_marker_id = (*next_marker_id).max(id.0 + 1);
    }
}

/// Overwrites every id `event` mints with a freshly allocated one, in declaration order. Sibling
/// of `minted_combatant_id`/`minted_marker_ids`, and exhaustive for the same reason: adding a
/// `BattleEvent` variant that mints an id must fail to compile here until it's handled.
///
/// Deliberately does *not* touch `ReviseCombatant`'s `InSequence` effects: those ids were minted
/// earlier by `StartSequence` (the queue editor only ever clones an existing sequence to change
/// its `current` step), so restamping them here would renumber ids that already exist elsewhere
/// in the log.
fn restamp_minted_ids(event: &mut BattleEvent, next_combatant_id: &mut u32, next_marker_id: &mut u32) {
    match event {
        BattleEvent::AddCombatant { id, .. } => {
            *id = CombatantId(*next_combatant_id);
            *next_combatant_id += 1;
        }
        BattleEvent::DeclareAction { action, .. } => {
            for effect in &mut action.effects {
                effect.id = MarkerId(*next_marker_id);
                *next_marker_id += 1;
            }
        }
        BattleEvent::StartSequence { sequence, .. } => {
            for effect in &mut sequence.effects {
                effect.id = MarkerId(*next_marker_id);
                *next_marker_id += 1;
            }
        }
        BattleEvent::AddMarker { id, .. } => {
            *id = MarkerId(*next_marker_id);
            *next_marker_id += 1;
        }
        BattleEvent::SetMode { .. }
        | BattleEvent::RemoveCombatant { .. }
        | BattleEvent::StartBattle
        | BattleEvent::AdvanceSequence { .. }
        | BattleEvent::InterruptSequence { .. }
        | BattleEvent::AdvanceTick
        | BattleEvent::RemoveMarker { .. }
        | BattleEvent::ReviseCombatant { .. }
        | BattleEvent::ReviseMarker { .. } => {}
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
        raise_watermarks(&event, &mut self.next_combatant_id, &mut self.next_marker_id);
        self.events.push(event);
        self.cursor += 1;
        Ok(())
    }

    /// Stamps `event`'s placeholder ids using this log's current counters, without appending
    /// anything or advancing them — the actual counter bump happens later, when the stamped event
    /// is pushed (by whoever ends up applying it, `push` raises the watermark on append). Used
    /// only to turn a network peer's id-less proposal into the one concrete event every node then
    /// validates and pushes identically; a single-player caller can just use
    /// `alloc_combatant_id`/`alloc_marker_id` directly and never needs this.
    pub fn restamp(&self, mut event: BattleEvent) -> BattleEvent {
        let mut next_combatant_id = self.next_combatant_id;
        let mut next_marker_id = self.next_marker_id;
        restamp_minted_ids(&mut event, &mut next_combatant_id, &mut next_marker_id);
        event
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
    use crate::battle::action::{label, note, template, ActionKind, ActionTemplate, Declaration, DeclaredEffect};
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
    fn pushing_a_pre_stamped_event_raises_the_watermark_past_it() {
        // The shape a networked peer receives: a concrete, already-minted id from someone else's
        // counter, arriving with no local `alloc_combatant_id` call at all.
        let mut log = BattleLog::new();
        log.push(BattleEvent::AddCombatant {
            id: CombatantId(41),
            name: "Remote".to_string(),
            side: Side("A".to_string()),
            join_battle: JoinBattleResult::Successes(0),
        })
        .unwrap();
        assert_eq!(log.alloc_combatant_id(), CombatantId(42));
    }

    #[test]
    fn rejected_push_does_not_raise_the_watermark() {
        let mut log = BattleLog::new();
        log.push(BattleEvent::AddCombatant {
            id: CombatantId(5),
            name: "First".to_string(),
            side: Side("A".to_string()),
            join_battle: JoinBattleResult::Successes(0),
        })
        .unwrap();
        // Same id again: rejected as a duplicate, so the watermark must not move.
        log.push(BattleEvent::AddCombatant {
            id: CombatantId(99),
            name: "Second".to_string(),
            side: Side("A".to_string()),
            join_battle: JoinBattleResult::Successes(0),
        })
        .unwrap();
        assert_eq!(log.alloc_combatant_id(), CombatantId(100));
    }

    #[test]
    fn restamp_assigns_the_next_combatant_id_without_mutating_the_log() {
        let mut log = BattleLog::new();
        add_event(&mut log, 5);
        let placeholder = BattleEvent::AddCombatant {
            id: CombatantId(0),
            name: "Newcomer".to_string(),
            side: Side("A".to_string()),
            join_battle: JoinBattleResult::Successes(0),
        };
        let stamped = log.restamp(placeholder);
        assert_eq!(minted_combatant_id(&stamped), Some(CombatantId(1)));
        // Calling it again from the same log produces the same id: restamp only peeks.
        let placeholder_again = BattleEvent::AddCombatant {
            id: CombatantId(0),
            name: "Newcomer".to_string(),
            side: Side("A".to_string()),
            join_battle: JoinBattleResult::Successes(0),
        };
        assert_eq!(minted_combatant_id(&log.restamp(placeholder_again)), Some(CombatantId(1)));
    }

    #[test]
    fn restamp_assigns_one_marker_id_per_effect_in_order() {
        let mut log = BattleLog::new();
        let cid = add_event(&mut log, 5);
        log.push(BattleEvent::StartBattle).unwrap();
        let action = personal(ActionKind::Attack).declare(Declaration {
            effects: vec![
                DeclaredEffect { id: MarkerId(0), label: label("A"), delay: 0, ticks: 1 },
                DeclaredEffect { id: MarkerId(0), label: label("B"), delay: 0, ticks: 1 },
            ],
            ..Default::default()
        });
        let BattleEvent::DeclareAction { action: stamped, .. } =
            log.restamp(BattleEvent::DeclareAction { actor: cid, action })
        else {
            unreachable!()
        };
        assert_eq!(stamped.effects[0].id, MarkerId(0));
        assert_eq!(stamped.effects[1].id, MarkerId(1));
    }

    #[test]
    fn restamp_leaves_an_in_sequence_revision_untouched() {
        // These marker ids were minted earlier by `StartSequence`; restamping them here would
        // renumber ids that already exist elsewhere in the log.
        let mut log = BattleLog::new();
        let cid = add_event(&mut log, 5);
        log.push(BattleEvent::StartBattle).unwrap();
        let mut sequence = Sequence::shape_terrestrial();
        sequence.effects = vec![DeclaredEffect { id: MarkerId(7), label: label("Cast"), delay: 0, ticks: 1 }];
        let event = BattleEvent::ReviseCombatant {
            actor: cid,
            next_action_tick: 0,
            state: CombatantState::InSequence(sequence),
            dv: DvState::default(),
            commitment: None,
            note: note(""),
        };
        let BattleEvent::ReviseCombatant { state: CombatantState::InSequence(stamped), .. } = log.restamp(event) else {
            unreachable!()
        };
        assert_eq!(stamped.effects[0].id, MarkerId(7));
    }

    fn personal(kind: ActionKind) -> &'static ActionTemplate {
        template(BattleMode::Personal, kind).expect("personal catalog")
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
            note: note("retconned to resolve sooner"),
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
            note: note(""),
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
                effects: vec![DeclaredEffect { id: effect_id, label: label("Bleed"), delay: 1, ticks: 2 }],
                ..Default::default()
            });
        log.push(BattleEvent::DeclareAction { actor: a, action: attack }).unwrap();
        log.push(BattleEvent::ReviseCombatant {
            actor: a,
            next_action_tick: 100,
            state: CombatantState::Guarding,
            dv: DvState { penalty: -1, refreshes_at: Some(100) },
            commitment: None,
            note: note("parked while b resolves its sorcery"),
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
        log.push(BattleEvent::AddMarker { id: marker_id, label: label("Window"), source: a, at_tick: 0, ticks: 3 }).unwrap();
        log.push(BattleEvent::ReviseMarker { id: marker_id, label: label("Wider window"), at_tick: 1, ticks: 4 }).unwrap();
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
            BattleEvent::AddMarker { id: MarkerId(3), label: label("Window"), source: CombatantId(0), at_tick: 0, ticks: 1 },
        ];
        let restored = RestoredLog { events, cursor: 3, next_combatant_id: 1, next_marker_id: 3 };
        let error = BattleLog::try_from(restored).unwrap_err();
        assert_eq!(error, RestoreError::StaleMarkerCounter { counter: 3, used: MarkerId(3) });
    }
}

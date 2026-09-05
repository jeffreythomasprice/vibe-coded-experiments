use crate::battle::action::{ActionKind, DeclaredAction};
use crate::battle::combatant::{Combatant, CombatantState, DvState, JoinBattleResult};
use crate::battle::error::BattleError;
use crate::battle::event::BattleEvent;
use crate::battle::ids::{CombatantId, MarkerId, Tick};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Setup,
    Running { reaction_count: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub id: MarkerId,
    pub label: String,
    pub source: CombatantId,
    pub at_tick: Tick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Battle {
    pub phase: Phase,
    pub current_tick: Tick,
    pub combatants: Vec<Combatant>,
    pub markers: Vec<Marker>,
}

impl Battle {
    pub fn genesis() -> Self {
        Battle { phase: Phase::Setup, current_tick: 0, combatants: Vec::new(), markers: Vec::new() }
    }

    pub fn find(&self, id: CombatantId) -> Option<&Combatant> {
        self.combatants.iter().find(|c| c.id == id)
    }

    fn find_mut(&mut self, id: CombatantId) -> Result<&mut Combatant, BattleError> {
        self.combatants.iter_mut().find(|c| c.id == id).ok_or(BattleError::UnknownCombatant(id))
    }

    /// The scene's reaction count (RULES.md §2.2, p. 141): a scene constant once the battle
    /// starts, but derived live from the highest Join Battle successes while still in Setup so
    /// the roster preview updates as combatants are added.
    pub fn reaction_count(&self) -> u32 {
        match self.phase {
            Phase::Running { reaction_count } => reaction_count,
            Phase::Setup => self
                .combatants
                .iter()
                .map(|c| match c.join_battle {
                    JoinBattleResult::Successes(successes) => successes,
                    JoinBattleResult::Botch => 0,
                })
                .max()
                .unwrap_or(0),
        }
    }

    fn reschedule_from_join_battle(&mut self, id: CombatantId) -> Result<(), BattleError> {
        let reaction_count = self.reaction_count();
        let current_tick = self.current_tick;
        let combatant = self.find_mut(id)?;
        combatant.next_action_tick = current_tick + combatant.join_battle.speed(reaction_count);
        Ok(())
    }
}

pub fn apply(battle: &mut Battle, event: &BattleEvent) -> Result<(), BattleError> {
    match event {
        BattleEvent::AddCombatant { id, name, side, join_battle } => {
            battle.combatants.push(Combatant {
                id: *id,
                name: name.clone(),
                side: side.clone(),
                join_battle: *join_battle,
                next_action_tick: 0,
                state: CombatantState::Normal,
                dv: DvState::default(),
            });
            match battle.phase {
                Phase::Setup => {
                    let ids: Vec<CombatantId> = battle.combatants.iter().map(|c| c.id).collect();
                    for id in ids {
                        battle.reschedule_from_join_battle(id)?;
                    }
                }
                Phase::Running { .. } => battle.reschedule_from_join_battle(*id)?,
            }
            Ok(())
        }

        BattleEvent::RemoveCombatant { id } => {
            let index = battle
                .combatants
                .iter()
                .position(|c| c.id == *id)
                .ok_or(BattleError::UnknownCombatant(*id))?;
            battle.combatants.remove(index);
            if matches!(battle.phase, Phase::Setup) {
                let ids: Vec<CombatantId> = battle.combatants.iter().map(|c| c.id).collect();
                for id in ids {
                    battle.reschedule_from_join_battle(id)?;
                }
            }
            Ok(())
        }

        BattleEvent::StartBattle => {
            if !matches!(battle.phase, Phase::Setup) {
                return Err(BattleError::AlreadyStarted);
            }
            battle.phase = Phase::Running { reaction_count: battle.reaction_count() };
            Ok(())
        }

        BattleEvent::DeclareAction { actor, action } => apply_declare_action(battle, *actor, action),

        BattleEvent::StartSequence { actor, sequence } => {
            if !matches!(battle.phase, Phase::Running { .. }) {
                return Err(BattleError::NotYetStarted);
            }
            let current_tick = battle.current_tick;
            let combatant = battle.find_mut(*actor)?;
            if matches!(combatant.state, CombatantState::InSequence(_)) {
                return Err(BattleError::SequenceAlreadyInProgress(*actor));
            }
            if combatant.next_action_tick > current_tick {
                return Err(BattleError::NotThisCombatantsTick {
                    actor: *actor,
                    next: combatant.next_action_tick,
                    current: current_tick,
                });
            }
            let mut sequence = sequence.clone();
            sequence.current = 0;
            let step = sequence.current_step().clone();
            let next_action_tick = current_tick + step.speed.resolve(None);
            combatant.next_action_tick = next_action_tick;
            combatant.dv = DvState { penalty: step.dv_penalty, refreshes_at: Some(next_action_tick) };
            combatant.state = CombatantState::InSequence(sequence);
            Ok(())
        }

        BattleEvent::AdvanceSequence { actor, speed_override } => {
            if !matches!(battle.phase, Phase::Running { .. }) {
                return Err(BattleError::NotYetStarted);
            }
            let current_tick = battle.current_tick;
            let combatant = battle.find_mut(*actor)?;
            let CombatantState::InSequence(sequence) = &combatant.state else {
                return Err(BattleError::NoSequenceInProgress(*actor));
            };
            if combatant.next_action_tick > current_tick {
                return Err(BattleError::NotThisCombatantsTick {
                    actor: *actor,
                    next: combatant.next_action_tick,
                    current: current_tick,
                });
            }
            if sequence.is_final_step() {
                combatant.state = CombatantState::Normal;
                return Ok(());
            }
            let CombatantState::InSequence(sequence) = &mut combatant.state else {
                unreachable!("checked above")
            };
            sequence.current += 1;
            let step = sequence.current_step().clone();
            let next_action_tick = current_tick + step.speed.resolve(*speed_override);
            combatant.next_action_tick = next_action_tick;
            combatant.dv = DvState { penalty: step.dv_penalty, refreshes_at: Some(next_action_tick) };
            Ok(())
        }

        BattleEvent::InterruptSequence { actor, reason: _, rejoin } => {
            let reaction_count = battle.reaction_count();
            let current_tick = battle.current_tick;
            let combatant = battle.find_mut(*actor)?;
            if !matches!(combatant.state, CombatantState::InSequence(_)) {
                return Err(BattleError::NoSequenceInProgress(*actor));
            }
            let next_action_tick = current_tick + rejoin.speed(reaction_count);
            combatant.state = CombatantState::Normal;
            combatant.next_action_tick = next_action_tick;
            combatant.dv = DvState { penalty: 0, refreshes_at: Some(next_action_tick) };
            Ok(())
        }

        BattleEvent::AdvanceTick => {
            if !matches!(battle.phase, Phase::Running { .. }) {
                return Err(BattleError::NotYetStarted);
            }
            let pending: Vec<CombatantId> = battle
                .combatants
                .iter()
                .filter(|c| !matches!(c.state, CombatantState::Inactive) && c.next_action_tick <= battle.current_tick)
                .map(|c| c.id)
                .collect();
            if !pending.is_empty() {
                return Err(BattleError::CombatantsPendingAction(pending));
            }
            battle.current_tick += 1;
            Ok(())
        }

        BattleEvent::AddMarker { id, label, source, at_tick } => {
            battle.find(*source).ok_or(BattleError::UnknownCombatant(*source))?;
            battle.markers.push(Marker { id: *id, label: label.clone(), source: *source, at_tick: *at_tick });
            Ok(())
        }

        BattleEvent::RemoveMarker { id } => {
            let index =
                battle.markers.iter().position(|m| m.id == *id).ok_or(BattleError::UnknownMarker(*id))?;
            battle.markers.remove(index);
            Ok(())
        }
    }
}

fn apply_declare_action(battle: &mut Battle, actor: CombatantId, action: &DeclaredAction) -> Result<(), BattleError> {
    if !matches!(battle.phase, Phase::Running { .. }) {
        return Err(BattleError::NotYetStarted);
    }
    let current_tick = battle.current_tick;
    let combatant = battle.find_mut(actor)?;

    if action.reflexive {
        return Ok(());
    }

    let aborting_early = matches!(combatant.state, CombatantState::Guarding | CombatantState::Aiming { .. })
        && combatant.next_action_tick > current_tick;

    if combatant.next_action_tick > current_tick && !aborting_early {
        return Err(BattleError::NotThisCombatantsTick {
            actor,
            next: combatant.next_action_tick,
            current: current_tick,
        });
    }

    // Inactive always forces DV to 0 outright, even mid-Guard/Aim: it isn't a chosen follow-up
    // action whose penalty should stack onto the suppressed one, it's an involuntary state that
    // overrides whatever she was doing.
    let suppress_refresh = action.kind != ActionKind::Inactive
        && match combatant.state {
            CombatantState::Guarding => aborting_early,
            CombatantState::Aiming { .. } => true,
            _ => false,
        };

    let next_action_tick = current_tick + action.speed;
    combatant.dv = DvState {
        penalty: if suppress_refresh { combatant.dv.penalty + action.dv_penalty } else { action.dv_penalty },
        refreshes_at: Some(next_action_tick),
    };
    combatant.next_action_tick = next_action_tick;
    combatant.state = match action.kind {
        ActionKind::Guard => CombatantState::Guarding,
        ActionKind::Aim => CombatantState::Aiming { target: action.target },
        ActionKind::Inactive => CombatantState::Inactive,
        _ => CombatantState::Normal,
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::action::CATALOG;
    use crate::battle::combatant::Side;
    use crate::battle::event::InterruptReason;
    use crate::battle::ids::CombatantId;
    use crate::battle::sequence::Sequence;

    fn template(kind: ActionKind) -> &'static crate::battle::action::ActionTemplate {
        CATALOG.iter().find(|t| t.kind == kind).unwrap()
    }

    fn add(battle: &mut Battle, id: u32, successes: u32) -> CombatantId {
        let cid = CombatantId(id);
        apply(
            battle,
            &BattleEvent::AddCombatant {
                id: cid,
                name: format!("C{id}"),
                side: Side("A".to_string()),
                join_battle: JoinBattleResult::Successes(successes),
            },
        )
        .unwrap();
        cid
    }

    #[test]
    fn first_actions_follow_reaction_count_clamp() {
        let mut battle = Battle::genesis();
        let fast = add(&mut battle, 1, 5);
        let mid = add(&mut battle, 2, 3);
        let slow = add(&mut battle, 3, 0);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        assert_eq!(battle.phase, Phase::Running { reaction_count: 5 });
        assert_eq!(battle.find(fast).unwrap().next_action_tick, 0);
        assert_eq!(battle.find(mid).unwrap().next_action_tick, 2);
        assert_eq!(battle.find(slow).unwrap().next_action_tick, 5);
    }

    #[test]
    fn botch_forces_first_action_to_six() {
        let mut battle = Battle::genesis();
        let cid = CombatantId(1);
        apply(
            &mut battle,
            &BattleEvent::AddCombatant {
                id: cid,
                name: "Botcher".to_string(),
                side: Side("A".to_string()),
                join_battle: JoinBattleResult::Botch,
            },
        )
        .unwrap();
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();
        assert_eq!(battle.find(cid).unwrap().next_action_tick, 6);
    }

    #[test]
    fn next_action_tick_is_current_plus_speed() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        let action = template(ActionKind::Dash).declare(None, None, None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action }).unwrap();
        assert_eq!(battle.find(cid).unwrap().next_action_tick, 3);
        assert_eq!(battle.find(cid).unwrap().dv.penalty, -2);
    }

    #[test]
    fn dv_refreshes_at_the_top_of_the_tick_after_the_penalized_window() {
        // Worked example from RULES.md §3: Speed 5 on tick 3 -> penalized ticks 3-7, refreshed at 8.
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();
        let guard = template(ActionKind::Guard).declare(None, None, None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: guard }).unwrap();
        for _ in 0..3 {
            apply(&mut battle, &BattleEvent::AdvanceTick).unwrap();
        }
        assert_eq!(battle.current_tick, 3);

        let action = template(ActionKind::Miscellaneous).declare(None, Some(-1), None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action }).unwrap();
        assert_eq!(battle.find(cid).unwrap().next_action_tick, 8);
        assert_eq!(battle.find(cid).unwrap().dv.refreshes_at, Some(8));
    }

    #[test]
    fn move_is_reflexive_and_does_not_reschedule_or_change_state() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 0);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        let guard = template(ActionKind::Guard).declare(None, None, None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: guard }).unwrap();
        assert_eq!(battle.find(cid).unwrap().state, CombatantState::Guarding);
        let next_action_tick_before = battle.find(cid).unwrap().next_action_tick;

        let mv = template(ActionKind::Move).declare(None, None, None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: mv }).unwrap();
        assert_eq!(battle.find(cid).unwrap().next_action_tick, next_action_tick_before);
        assert_eq!(battle.find(cid).unwrap().state, CombatantState::Guarding);
    }

    #[test]
    fn guard_abort_does_not_refresh_dv_and_reschedules_from_now() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 0);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        let guard = template(ActionKind::Guard).declare(None, None, None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: guard }).unwrap();
        assert_eq!(battle.find(cid).unwrap().next_action_tick, 3);

        // Abort on tick 1, before Guard's Speed 3 has elapsed.
        apply(&mut battle, &BattleEvent::AdvanceTick).unwrap();
        let dash = template(ActionKind::Dash).declare(None, None, None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: dash }).unwrap();

        let combatant = battle.find(cid).unwrap();
        assert_eq!(combatant.next_action_tick, 1 + 3);
        assert_eq!(combatant.dv.penalty, 0 + -2);
        assert_eq!(combatant.state, CombatantState::Normal);
    }

    #[test]
    fn inactive_resets_dv_to_zero_even_when_aborting_early_out_of_aim() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 0);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        let aim = template(ActionKind::Aim).declare(None, None, None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: aim }).unwrap();
        assert_eq!(battle.find(cid).unwrap().dv.penalty, -1);

        // Abort on tick 1, before Aim's Speed 3 has elapsed.
        apply(&mut battle, &BattleEvent::AdvanceTick).unwrap();
        let inactive = template(ActionKind::Inactive).declare(None, None, None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: inactive }).unwrap();

        let combatant = battle.find(cid).unwrap();
        assert_eq!(combatant.dv.penalty, 0);
        assert_eq!(combatant.state, CombatantState::Inactive);
    }

    #[test]
    fn cannot_act_before_your_scheduled_tick() {
        let mut battle = Battle::genesis();
        let _fast = add(&mut battle, 1, 5);
        let cid = add(&mut battle, 2, 0);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        let dash = template(ActionKind::Dash).declare(None, None, None, String::new());
        let err = apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: dash }).unwrap_err();
        assert_eq!(err, BattleError::NotThisCombatantsTick { actor: cid, next: 5, current: 0 });
    }

    #[test]
    fn advance_tick_blocked_while_someone_is_due() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        let err = apply(&mut battle, &BattleEvent::AdvanceTick).unwrap_err();
        assert_eq!(err, BattleError::CombatantsPendingAction(vec![cid]));

        let guard = template(ActionKind::Guard).declare(None, None, None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: guard }).unwrap();
        apply(&mut battle, &BattleEvent::AdvanceTick).unwrap();
        assert_eq!(battle.current_tick, 1);
    }

    #[test]
    fn celestial_sorcery_advances_through_both_shapes_then_casts() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        apply(&mut battle, &BattleEvent::StartSequence { actor: cid, sequence: Sequence::shape_celestial() })
            .unwrap();
        assert_eq!(battle.find(cid).unwrap().next_action_tick, 5);
        assert_eq!(battle.find(cid).unwrap().dv.penalty, -3);

        for _ in 0..5 {
            apply(&mut battle, &BattleEvent::AdvanceTick).unwrap();
        }
        apply(&mut battle, &BattleEvent::AdvanceSequence { actor: cid, speed_override: None }).unwrap();
        assert_eq!(battle.find(cid).unwrap().next_action_tick, 10);
        assert_eq!(battle.find(cid).unwrap().dv.penalty, -3);
        assert!(matches!(battle.find(cid).unwrap().state, CombatantState::InSequence(_)));

        for _ in 0..5 {
            apply(&mut battle, &BattleEvent::AdvanceTick).unwrap();
        }
        apply(&mut battle, &BattleEvent::AdvanceSequence { actor: cid, speed_override: Some(4) }).unwrap();
        assert_eq!(battle.find(cid).unwrap().next_action_tick, 14);
        assert_eq!(battle.find(cid).unwrap().dv.penalty, 0);

        for _ in 0..4 {
            apply(&mut battle, &BattleEvent::AdvanceTick).unwrap();
        }
        apply(&mut battle, &BattleEvent::AdvanceSequence { actor: cid, speed_override: None }).unwrap();
        assert_eq!(battle.find(cid).unwrap().state, CombatantState::Normal);
    }

    #[test]
    fn interrupting_a_sequence_drops_it_and_rejoins_from_frozen_reaction_count() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 5);
        let _other = add(&mut battle, 2, 2);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        apply(&mut battle, &BattleEvent::StartSequence { actor: cid, sequence: Sequence::shape_terrestrial() })
            .unwrap();
        apply(
            &mut battle,
            &BattleEvent::InterruptSequence {
                actor: cid,
                reason: InterruptReason::FailedOccultCheck,
                rejoin: JoinBattleResult::Successes(1),
            },
        )
        .unwrap();

        let combatant = battle.find(cid).unwrap();
        assert_eq!(combatant.state, CombatantState::Normal);
        assert_eq!(combatant.next_action_tick, 4);
        assert_eq!(combatant.dv.penalty, 0);
    }

    #[test]
    fn joining_in_progress_uses_the_frozen_reaction_count() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();
        let guard = template(ActionKind::Guard).declare(None, None, None, String::new());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: guard }).unwrap();
        for _ in 0..3 {
            apply(&mut battle, &BattleEvent::AdvanceTick).unwrap();
        }

        let latecomer = add(&mut battle, 2, 2);
        assert_eq!(battle.find(latecomer).unwrap().next_action_tick, 3 + 3);
    }
}

use crate::battle_net::{BattleView, Battles};
use crate::ui::glossary::Topic;
use crate::ui::{ticks, Modal, TextTip, Tip};
use exalted_battle_wheel::battle::{
    apply, template, Battle, BattleEvent, BattleLog, BattleMode, CombatantId, CombatantState, InterruptReason,
    JoinBattleResult,
};
use leptos::prelude::*;

struct EventLine {
    text: String,
    detail: Option<String>,
}

fn name(battle: &Battle, id: CombatantId) -> String {
    battle.find(id).map(|c| c.name.clone()).unwrap_or_else(|| format!("combatant #{}", id.0))
}

fn join_battle_detail(join_battle: JoinBattleResult) -> String {
    match join_battle {
        JoinBattleResult::Successes(successes) => format!("Join Battle: {successes} successes"),
        JoinBattleResult::Botch => "Join Battle: botch".to_string(),
    }
}

fn marker_span(mode: BattleMode, at_tick: exalted_battle_wheel::battle::Tick, span_ticks: u32) -> String {
    ticks::span(mode, at_tick, span_ticks)
}

fn state_label(state: &CombatantState) -> String {
    match state {
        CombatantState::Normal => "Normal".to_string(),
        CombatantState::Guarding => "Guarding".to_string(),
        CombatantState::Aiming { .. } => "Aiming".to_string(),
        CombatantState::Inactive => "Inactive".to_string(),
        CombatantState::InSequence(sequence) => {
            format!("{} (step {}/{})", sequence.name, sequence.current + 1, sequence.steps.len())
        }
    }
}

fn interrupt_reason(reason: &InterruptReason) -> String {
    match reason {
        InterruptReason::FailedOccultCheck => "failed Occult check".to_string(),
        InterruptReason::WentInactive => "went inactive".to_string(),
        InterruptReason::Voluntary => "voluntary".to_string(),
        InterruptReason::Other(reason) => reason.clone(),
    }
}

fn describe(battle: &Battle, event: &BattleEvent) -> EventLine {
    match event {
        BattleEvent::SetMode { mode } => EventLine { text: format!("Mode set to {}", mode.label()), detail: None },
        BattleEvent::AddCombatant { name, side, join_battle, .. } => EventLine {
            text: format!("Added {name} ({})", side.0),
            detail: Some(join_battle_detail(*join_battle)),
        },
        BattleEvent::RemoveCombatant { id } => {
            EventLine { text: format!("Removed {}", name(battle, *id)), detail: None }
        }
        BattleEvent::StartBattle => EventLine {
            text: "Battle started".to_string(),
            detail: Some(format!("Reaction count {}", battle.reaction_count())),
        },
        BattleEvent::DeclareAction { actor, action } => {
            let target = action.target.map(|id| format!(" on {}", name(battle, id))).unwrap_or_default();
            // Falls back to the action's own stored label if its kind no longer resolves in this
            // battle's mode (only reachable by replaying a log across a hand-edited mode change).
            let kind_name = template(battle.mode, action.kind).map(|t| t.name).unwrap_or(action.label.as_str());
            let mut detail = if action.label == kind_name {
                format!("Speed {}, DV {}", action.speed, action.dv_penalty)
            } else {
                format!("{kind_name}, Speed {}, DV {}", action.speed, action.dv_penalty)
            };
            if action.reflexive {
                detail.push_str(", reflexive");
            }
            if !action.note.is_empty() {
                detail.push_str(&format!(" — {}", action.note));
            }
            EventLine {
                text: format!("{} declares {}{target}", name(battle, *actor), action.label),
                detail: Some(detail),
            }
        }
        BattleEvent::StartSequence { actor, sequence } => {
            let detail = sequence.steps.iter().map(|step| step.label.as_str()).collect::<Vec<_>>().join(" \u{2192} ");
            EventLine {
                text: format!("{} starts {}", name(battle, *actor), sequence.name),
                detail: Some(detail),
            }
        }
        BattleEvent::AdvanceSequence { actor, speed_override } => {
            let combatant = battle.find(*actor);
            let sequence = combatant.and_then(|c| match &c.state {
                CombatantState::InSequence(sequence) => Some(sequence),
                _ => None,
            });
            let text = match sequence {
                Some(sequence) => format!(
                    "{} completes {} ({}/{})",
                    name(battle, *actor),
                    sequence.current_step().label,
                    sequence.current + 1,
                    sequence.steps.len()
                ),
                None => format!("{} advances their sequence", name(battle, *actor)),
            };
            EventLine { text, detail: speed_override.map(|speed| format!("Speed override {speed}")) }
        }
        BattleEvent::InterruptSequence { actor, reason, rejoin } => {
            let combatant = battle.find(*actor);
            let sequence_name = combatant.and_then(|c| match &c.state {
                CombatantState::InSequence(sequence) => Some(sequence.name.clone()),
                _ => None,
            });
            let text = match sequence_name {
                Some(sequence_name) => {
                    format!("{}'s {sequence_name} is interrupted ({})", name(battle, *actor), interrupt_reason(reason))
                }
                None => format!("{} is interrupted ({})", name(battle, *actor), interrupt_reason(reason)),
            };
            let detail = match rejoin {
                JoinBattleResult::Successes(successes) => format!("Rejoins with {successes} successes"),
                JoinBattleResult::Botch => "Rejoins on a botch".to_string(),
            };
            EventLine { text, detail: Some(detail) }
        }
        BattleEvent::AdvanceTick => EventLine {
            text: format!("Advanced to {}", ticks::at(battle.mode, battle.current_tick + 1)),
            detail: None,
        },
        BattleEvent::AddMarker { label, source, at_tick, ticks: span_ticks, .. } => {
            let span = marker_span(battle.mode, *at_tick, *span_ticks);
            EventLine { text: format!("Marker \"{label}\" on {span} (from {})", name(battle, *source)), detail: None }
        }
        BattleEvent::RemoveMarker { id } => {
            let label = battle.markers.iter().find(|m| m.id == *id).map(|m| m.label.clone());
            let text = match label {
                Some(label) => format!("Removed marker \"{label}\""),
                None => "Removed marker".to_string(),
            };
            EventLine { text, detail: None }
        }
        BattleEvent::ReviseCombatant { actor, next_action_tick, state, dv, commitment, note } => {
            let before = battle.find(*actor);
            let mut parts = Vec::new();
            if let Some(before) = before {
                if before.next_action_tick != *next_action_tick {
                    parts.push(format!("{} \u{2192} {}", ticks::at(battle.mode, before.next_action_tick), ticks::at(battle.mode, *next_action_tick)));
                }
                if before.dv.penalty != dv.penalty {
                    parts.push(format!("DV {} \u{2192} {}", before.dv.penalty, dv.penalty));
                }
                if before.state != *state {
                    parts.push(format!("{} \u{2192} {}", state_label(&before.state), state_label(state)));
                }
                match (&before.commitment, commitment) {
                    (Some(prev), None) => parts.push(format!("cleared {}", prev.label)),
                    (None, Some(next)) => parts.push(format!("set {}", next.label)),
                    (Some(prev), Some(next)) if prev.label != next.label => {
                        parts.push(format!("{} \u{2192} {}", prev.label, next.label));
                    }
                    _ => {}
                }
            }
            if !note.is_empty() {
                parts.push(note.clone());
            }
            let detail = if parts.is_empty() { "No changes".to_string() } else { parts.join("; ") };
            EventLine { text: format!("Revised {}", name(battle, *actor)), detail: Some(detail) }
        }
        BattleEvent::ReviseMarker { id, label, at_tick, ticks } => {
            let before = battle.markers.iter().find(|m| m.id == *id);
            let title = before.map(|m| m.label.clone()).unwrap_or_else(|| label.clone());
            let mut parts = Vec::new();
            if let Some(before) = before {
                if before.label != *label {
                    parts.push(format!("\"{}\" \u{2192} \"{label}\"", before.label));
                }
                let before_span = marker_span(battle.mode, before.at_tick, before.ticks);
                let after_span = marker_span(battle.mode, *at_tick, *ticks);
                if before_span != after_span {
                    parts.push(format!("{before_span} \u{2192} {after_span}"));
                }
            }
            let detail = if parts.is_empty() { "No changes".to_string() } else { parts.join("; ") };
            EventLine { text: format!("Retimed \"{title}\""), detail: Some(detail) }
        }
    }
}

fn lines(log: &BattleLog) -> Vec<EventLine> {
    let mut battle = Battle::genesis();
    log.events()
        .iter()
        .map(|event| {
            let line = describe(&battle, event);
            _ = apply(&mut battle, event);
            line
        })
        .collect()
}

#[component]
pub fn EventLogButton() -> impl IntoView {
    let log = expect_context::<BattleView>();
    let battles = expect_context::<Battles>();
    let open = RwSignal::new(false);

    let jump = move |target: usize| {
        battles.seek(target);
        open.set(false);
    };

    view! {
        <Tip topic=Topic::EventLog>
            <button on:click=move |_| open.set(true)>"Event Log"</button>
        </Tip>
        {move || {
            open.get()
                .then(|| {
                    let cursor = log.read().cursor();
                    let rows = lines(&log.read());
                    view! {
                        <Modal title="Event Log" on_close=move || open.set(false)>
                            <ul class="event-log-list">
                                <li>
                                    <button
                                        class="event-log-row"
                                        class:event-log-row-current=cursor == 0
                                        on:click=move |_| jump(0)
                                    >
                                        "Battle start"
                                    </button>
                                </li>
                                {rows
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, line)| {
                                        let is_current = cursor == index + 1;
                                        let is_future = index >= cursor;
                                        let row = view! {
                                            <button
                                                class="event-log-row"
                                                class:event-log-row-current=is_current
                                                class:event-log-row-future=is_future
                                                on:click=move |_| jump(index + 1)
                                            >
                                                {line.text}
                                            </button>
                                        };
                                        view! {
                                            <li>
                                                {match line.detail {
                                                    Some(detail) => {
                                                        view! { <TextTip text=detail>{row}</TextTip> }.into_any()
                                                    }
                                                    None => row.into_any(),
                                                }}
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        </Modal>
                    }
                })
        }}
    }
}

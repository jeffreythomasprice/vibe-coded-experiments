use crate::ui::glossary::Topic;
use crate::ui::{DetailTip, MarkerForm, Modal, Tip};
use exalted_battle_wheel::battle::{
    queue, Battle, BattleEvent, BattleLog, Combatant, CombatantId, CombatantState, DvState, Marker, MarkerId, QueueItem,
    Tick,
};
use leptos::prelude::*;

pub fn span_label(marker: &Marker) -> String {
    if marker.ticks <= 1 { format!("tick {}", marker.at_tick) } else { format!("ticks {}\u{2013}{}", marker.at_tick, marker.last_tick()) }
}

/// The tick-span half of a marker's queue row: what's shown depends on whether its span has
/// started yet. `span_label` alone can't say this — it only knows the span, not `now`.
fn marker_queue_span(marker: &Marker, now: Tick) -> String {
    if marker.at_tick > now {
        let noun = if marker.ticks == 1 { "tick" } else { "ticks" };
        format!("starts tick {}, for {} {noun}", marker.at_tick, marker.ticks)
    } else {
        span_label(marker)
    }
}

/// The combatant half of a queue row. Ready-now takes priority over everything else — an
/// in-sequence combatant who is also due still reads as ready, matching the action panel's "Up
/// now" section, which shows sequence controls for her rather than a queue-style projection.
fn combatant_row_text(combatant: &Combatant, now: Tick) -> String {
    if combatant.next_action_tick <= now {
        return format!("{} \u{2014} ready now", combatant.name);
    }
    let until = combatant.next_action_tick - now;
    if let CombatantState::InSequence(sequence) = &combatant.state {
        return format!("{} will do {} on tick {} (in {until})", combatant.name, sequence.current_step().label, combatant.next_action_tick);
    }
    match &combatant.commitment {
        Some(commitment) => {
            format!("{} \u{2014} {} resolving, ready tick {} (in {until})", combatant.name, commitment.label, combatant.next_action_tick)
        }
        None => format!("{} \u{2014} ready to act on tick {} (in {until})", combatant.name, combatant.next_action_tick),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Editing {
    Combatant(CombatantId),
    Marker(MarkerId),
}

#[component]
pub fn QueuePanel() -> impl IntoView {
    let log = expect_context::<RwSignal<BattleLog>>();
    let battle = expect_context::<Memo<Battle>>();
    let editing = RwSignal::new(None::<Editing>);

    let rows = move || queue(&battle.read());

    view! {
        <div class="queue-panel">
            <Tip topic=Topic::Queue>
                <h2>"Queue"</h2>
            </Tip>
            <ul class="queue-list">
                <For each=rows key=|row| row.item let:row>
                    {match row.item {
                        QueueItem::Combatant(id) => view! { <CombatantQueueRow id=id battle=battle editing=editing /> }.into_any(),
                        QueueItem::Marker(id) => view! { <MarkerQueueRow id=id battle=battle editing=editing /> }.into_any(),
                    }}
                </For>
            </ul>
            {move || {
                editing.get().and_then(|target| match target {
                    Editing::Combatant(id) => {
                        let initial = battle.read().find(id)?.clone();
                        let others: Vec<Combatant> = battle.read().combatants.iter().filter(|c| c.id != id).cloned().collect();
                        let current_tick = battle.read().current_tick;
                        Some(view! {
                            <Modal title="Revise combatant" on_close=move || editing.set(None)>
                                <CombatantEditor
                                    actor_id=id
                                    initial=initial
                                    others=others
                                    current_tick=current_tick
                                    log=log
                                    on_close=move || editing.set(None)
                                />
                            </Modal>
                        }.into_any())
                    }
                    Editing::Marker(id) => {
                        let initial = battle.read().markers.iter().find(|m| m.id == id)?.clone();
                        Some(view! {
                            <Modal title="Revise marker" on_close=move || editing.set(None)>
                                <MarkerEditor marker_id=id initial=initial log=log on_close=move || editing.set(None) />
                            </Modal>
                        }.into_any())
                    }
                })
            }}
            <MarkerForm />
        </div>
    }
}

#[component]
fn CombatantQueueRow(id: CombatantId, battle: Memo<Battle>, editing: RwSignal<Option<Editing>>) -> impl IntoView {
    let text = move || {
        let battle = battle.read();
        battle.find(id).map(|c| combatant_row_text(c, battle.current_tick)).unwrap_or_default()
    };
    let ready = move || {
        let battle = battle.read();
        battle.find(id).is_some_and(|c| c.next_action_tick <= battle.current_tick)
    };
    let dv_line = move || {
        let battle = battle.read();
        battle.find(id).filter(|c| c.dv.penalty != 0).map(|c| match c.dv.refreshes_at {
            Some(refresh) => format!("DV {}, refreshes tick {refresh}", c.dv.penalty),
            None => format!("DV {}", c.dv.penalty),
        })
    };

    view! {
        <li>
            <button class="queue-row" class:queue-row-ready=ready on:click=move |_| editing.set(Some(Editing::Combatant(id)))>
                <span class="queue-row-main">{text}</span>
                {move || dv_line().map(|line| view! { <span class="queue-row-sub">{line}</span> })}
            </button>
        </li>
    }
}

#[component]
fn MarkerQueueRow(id: MarkerId, battle: Memo<Battle>, editing: RwSignal<Option<Editing>>) -> impl IntoView {
    let text = move || {
        let battle = battle.read();
        battle.markers.iter().find(|m| m.id == id).map(|marker| {
            let source = battle.find(marker.source).map(|c| c.name.clone()).unwrap_or_else(|| format!("#{}", marker.source.0));
            format!("{} \u{2014} {}, from {source}", marker.label, marker_queue_span(marker, battle.current_tick))
        }).unwrap_or_default()
    };
    let pending = move || {
        let battle = battle.read();
        battle.markers.iter().find(|m| m.id == id).is_some_and(|m| m.at_tick > battle.current_tick)
    };
    let topic = Signal::derive(move || if pending() { Topic::PendingMarker } else { Topic::Markers });
    let detail = Signal::derive(String::new);

    view! {
        <li>
            <DetailTip topic=topic detail=detail>
                <button
                    class="queue-row queue-row-marker"
                    class:queue-row-pending=pending
                    on:click=move |_| editing.set(Some(Editing::Marker(id)))
                >
                    {text}
                </button>
            </DetailTip>
        </li>
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateKind {
    Normal,
    Guarding,
    Aiming,
    Inactive,
    InSequence,
}

fn state_kind_of(state: &CombatantState) -> StateKind {
    match state {
        CombatantState::Normal => StateKind::Normal,
        CombatantState::Guarding => StateKind::Guarding,
        CombatantState::Aiming { .. } => StateKind::Aiming,
        CombatantState::Inactive => StateKind::Inactive,
        CombatantState::InSequence(_) => StateKind::InSequence,
    }
}

#[component]
fn CombatantEditor(
    actor_id: CombatantId,
    initial: Combatant,
    others: Vec<Combatant>,
    current_tick: Tick,
    log: RwSignal<BattleLog>,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let initial_commitment = initial.commitment.clone();
    let commitment_label = initial_commitment.as_ref().map(|c| c.label.clone());
    let original_sequence = match &initial.state {
        CombatantState::InSequence(sequence) => Some(sequence.clone()),
        _ => None,
    };
    let has_sequence = original_sequence.is_some();
    let sequence_for_view = original_sequence.clone();

    let next_tick = RwSignal::new(initial.next_action_tick.to_string());
    let dv_penalty = RwSignal::new(initial.dv.penalty.to_string());
    let no_refresh = RwSignal::new(initial.dv.refreshes_at.is_none());
    let dv_refreshes = RwSignal::new(initial.dv.refreshes_at.map(|t| t.to_string()).unwrap_or_default());
    let state_kind = RwSignal::new(state_kind_of(&initial.state));
    let aim_target = RwSignal::new(match &initial.state {
        CombatantState::Aiming { target } => *target,
        _ => None,
    });
    let sequence_step = RwSignal::new(original_sequence.as_ref().map(|s| s.current).unwrap_or(0));
    let clear_commitment = RwSignal::new(false);
    let note = RwSignal::new(String::new());

    let cancel_action = move |_| {
        next_tick.set(current_tick.to_string());
        state_kind.set(StateKind::Normal);
        clear_commitment.set(true);
    };

    let apply = move |_| {
        let Ok(parsed_tick) = next_tick.get().trim().parse::<Tick>() else { return };
        let state = match state_kind.get() {
            StateKind::Normal => CombatantState::Normal,
            StateKind::Guarding => CombatantState::Guarding,
            StateKind::Aiming => CombatantState::Aiming { target: aim_target.get() },
            StateKind::Inactive => CombatantState::Inactive,
            StateKind::InSequence => match &original_sequence {
                Some(sequence) => {
                    let mut sequence = sequence.clone();
                    sequence.current = sequence_step.get();
                    CombatantState::InSequence(sequence)
                }
                None => CombatantState::Normal,
            },
        };
        let dv = DvState {
            penalty: dv_penalty.get().trim().parse().unwrap_or(0),
            refreshes_at: if no_refresh.get() { None } else { dv_refreshes.get().trim().parse().ok() },
        };
        let commitment = if clear_commitment.get() { None } else { initial_commitment.clone() };
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::ReviseCombatant {
                actor: actor_id,
                next_action_tick: parsed_tick,
                state,
                dv,
                commitment,
                note: note.get(),
            }) {
                tracing::error!(%error, "could not revise combatant");
            }
        });
        on_close();
    };

    view! {
        <div class="queue-editor">
            <Tip topic=Topic::ReviseCombatant>
                <div class="queue-editor-hint">"Applying this appends a correction event \u{2014} Undo reverts it."</div>
            </Tip>
            <label class="queue-field">
                "Next action tick"
                <input type="number" prop:value=move || next_tick.get() on:input=move |ev| next_tick.set(event_target_value(&ev)) />
            </label>
            <label class="queue-field">
                "DV penalty"
                <input type="number" prop:value=move || dv_penalty.get() on:input=move |ev| dv_penalty.set(event_target_value(&ev)) />
            </label>
            <label class="queue-field">
                "DV refreshes at tick"
                <input
                    type="number"
                    prop:value=move || dv_refreshes.get()
                    on:input=move |ev| dv_refreshes.set(event_target_value(&ev))
                    disabled=move || no_refresh.get()
                />
            </label>
            <label class="queue-field">
                <input type="checkbox" prop:checked=move || no_refresh.get() on:change=move |ev| no_refresh.set(event_target_checked(&ev)) />
                "No pending refresh"
            </label>
            <label class="queue-field">
                "State"
                <select
                    prop:value=move || match state_kind.get() {
                        StateKind::Normal => "normal",
                        StateKind::Guarding => "guarding",
                        StateKind::Aiming => "aiming",
                        StateKind::Inactive => "inactive",
                        StateKind::InSequence => "in_sequence",
                    }
                    on:change=move |ev| {
                        state_kind.set(match event_target_value(&ev).as_str() {
                            "guarding" => StateKind::Guarding,
                            "aiming" => StateKind::Aiming,
                            "inactive" => StateKind::Inactive,
                            "in_sequence" => StateKind::InSequence,
                            _ => StateKind::Normal,
                        });
                    }
                >
                    <option value="normal">"Normal"</option>
                    <option value="guarding">"Guarding"</option>
                    <option value="aiming">"Aiming"</option>
                    <option value="inactive">"Inactive"</option>
                    {has_sequence.then(|| view! { <option value="in_sequence">"In sequence (keep)"</option> })}
                </select>
            </label>
            {has_sequence.then(|| view! {
                <Tip topic=Topic::CancelSequenceEarly>
                    <div class="queue-editor-hint">
                        "Choosing anything but \u{201c}In sequence (keep)\u{201d} abandons the spell without an automatic rejoin roll \u{2014} set the next action tick yourself, or use Interrupt in the action panel for the modeled rejoin."
                    </div>
                </Tip>
            })}
            {move || (state_kind.get() == StateKind::Aiming).then(|| view! {
                <label class="queue-field">
                    "Aim target"
                    <select
                        prop:value=move || aim_target.get().map(|id| id.0.to_string()).unwrap_or_default()
                        on:change=move |ev| aim_target.set(event_target_value(&ev).parse::<u32>().ok().map(CombatantId))
                    >
                        <option value="">"None"</option>
                        {others.iter().map(|c| view! { <option value=c.id.0.to_string()>{c.name.clone()}</option> }).collect_view()}
                    </select>
                </label>
            })}
            {move || (state_kind.get() == StateKind::InSequence).then(|| view! {
                <label class="queue-field">
                    "Sequence step"
                    <select
                        prop:value=move || sequence_step.get().to_string()
                        on:change=move |ev| sequence_step.set(event_target_value(&ev).parse().unwrap_or(0))
                    >
                        {sequence_for_view.as_ref().map(|sequence| {
                            sequence.steps.iter().enumerate().map(|(i, step)| view! {
                                <option value=i.to_string()>{step.label.clone()}</option>
                            }).collect_view()
                        })}
                    </select>
                </label>
            })}
            {commitment_label.map(|label| view! {
                <div class="queue-commitment">
                    <span>"Committed to: " {label}</span>
                    <label>
                        <input
                            type="checkbox"
                            prop:checked=move || clear_commitment.get()
                            on:change=move |ev| clear_commitment.set(event_target_checked(&ev))
                        />
                        "Clear"
                    </label>
                </div>
            })}
            <label class="queue-field">
                "Note"
                <input prop:value=move || note.get() on:input=move |ev| note.set(event_target_value(&ev)) />
            </label>
            <div class="queue-editor-actions">
                <button on:click=cancel_action class="interrupt-button">"Cancel action \u{2014} ready now"</button>
                <button on:click=apply>"Apply"</button>
            </div>
        </div>
    }
}

#[component]
fn MarkerEditor(marker_id: MarkerId, initial: Marker, log: RwSignal<BattleLog>, on_close: impl Fn() + Copy + 'static) -> impl IntoView {
    let label = RwSignal::new(initial.label.clone());
    let at_tick = RwSignal::new(initial.at_tick.to_string());
    let ticks = RwSignal::new(initial.ticks.to_string());

    let apply = move |_| {
        let Ok(parsed_tick) = at_tick.get().trim().parse::<Tick>() else { return };
        let Ok(parsed_ticks) = ticks.get().trim().parse::<u32>() else { return };
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::ReviseMarker { id: marker_id, label: label.get(), at_tick: parsed_tick, ticks: parsed_ticks }) {
                tracing::error!(%error, "could not revise marker");
            }
        });
        on_close();
    };

    let remove = move |_| {
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::RemoveMarker { id: marker_id }) {
                tracing::error!(%error, "could not remove marker");
            }
        });
        on_close();
    };

    view! {
        <div class="queue-editor">
            <label class="queue-field">
                "Label"
                <input prop:value=move || label.get() on:input=move |ev| label.set(event_target_value(&ev)) />
            </label>
            <label class="queue-field">
                "Starts at tick"
                <input type="number" prop:value=move || at_tick.get() on:input=move |ev| at_tick.set(event_target_value(&ev)) />
            </label>
            <label class="queue-field">
                "Duration (ticks)"
                <input type="number" min="1" prop:value=move || ticks.get() on:input=move |ev| ticks.set(event_target_value(&ev)) />
            </label>
            <div class="queue-editor-actions">
                <button on:click=remove class="interrupt-button">"Remove"</button>
                <button on:click=apply>"Apply"</button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exalted_battle_wheel::battle::{Commitment, JoinBattleResult, Sequence, Side};

    fn combatant(state: CombatantState, commitment: Option<Commitment>) -> Combatant {
        Combatant {
            id: CombatantId(0),
            name: "Rin".to_string(),
            side: Side("A".to_string()),
            join_battle: JoinBattleResult::Successes(0),
            next_action_tick: 12,
            state,
            dv: DvState::default(),
            commitment,
        }
    }

    #[test]
    fn combatant_row_reports_ready_now_when_due() {
        let mut c = combatant(CombatantState::Normal, None);
        c.next_action_tick = 5;
        assert_eq!(combatant_row_text(&c, 5), "Rin \u{2014} ready now");
    }

    #[test]
    fn combatant_row_reports_the_sequence_step_when_in_sequence() {
        let c = combatant(CombatantState::InSequence(Sequence::shape_terrestrial()), None);
        assert_eq!(combatant_row_text(&c, 3), "Rin will do Shape Terrestrial Circle Sorcery (1/1) on tick 12 (in 9)");
    }

    #[test]
    fn combatant_row_reports_a_resolving_commitment() {
        let commitment = Commitment { label: "Attack".to_string(), speed: 5, declared_at: 7 };
        let c = combatant(CombatantState::Normal, Some(commitment));
        assert_eq!(combatant_row_text(&c, 7), "Rin \u{2014} Attack resolving, ready tick 12 (in 5)");
    }

    #[test]
    fn combatant_row_falls_back_to_a_plain_ready_message() {
        let c = combatant(CombatantState::Normal, None);
        assert_eq!(combatant_row_text(&c, 7), "Rin \u{2014} ready to act on tick 12 (in 5)");
    }

    #[test]
    fn marker_span_reports_pending_before_it_starts() {
        let marker = Marker { id: MarkerId(0), label: "Ambush".to_string(), source: CombatantId(0), at_tick: 14, ticks: 3 };
        assert_eq!(marker_queue_span(&marker, 10), "starts tick 14, for 3 ticks");
    }

    #[test]
    fn marker_span_reports_the_active_span_once_started() {
        let marker = Marker { id: MarkerId(0), label: "Ambush".to_string(), source: CombatantId(0), at_tick: 8, ticks: 3 };
        assert_eq!(marker_queue_span(&marker, 9), "ticks 8\u{2013}10");
    }

    #[test]
    fn span_label_covers_a_single_tick() {
        let marker = Marker { id: MarkerId(0), label: "Window".to_string(), source: CombatantId(0), at_tick: 5, ticks: 1 };
        assert_eq!(span_label(&marker), "tick 5");
    }
}

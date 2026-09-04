use crate::ui::glossary::{action_topic, Topic};
use crate::ui::Tip;
use exalted_battle_wheel::battle::{
    ActionTemplate, Battle, BattleEvent, BattleLog, CombatantId, CombatantState, DvPenaltySpec, InterruptReason,
    JoinBattleResult, Phase, Sequence, SpeedSpec, CATALOG,
};
use leptos::prelude::*;

#[component]
pub fn ActionPanel() -> impl IntoView {
    let log = expect_context::<RwSignal<BattleLog>>();
    let battle = expect_context::<Memo<Battle>>();

    let up_now = move || -> Vec<CombatantId> {
        let battle = battle.read();
        if !matches!(battle.phase, Phase::Running { .. }) {
            return Vec::new();
        }
        battle
            .combatants
            .iter()
            .filter(|c| !matches!(c.state, CombatantState::Inactive) && c.next_action_tick <= battle.current_tick)
            .map(|c| c.id)
            .collect()
    };

    let shaping_but_not_up = move || -> Vec<CombatantId> {
        let battle = battle.read();
        let up: Vec<CombatantId> = up_now();
        battle
            .combatants
            .iter()
            .filter(|c| matches!(c.state, CombatantState::InSequence(_)) && !up.contains(&c.id))
            .map(|c| c.id)
            .collect()
    };

    view! {
        <div class="action-panel">
            <Tip topic=Topic::UpNow>
                <h2>"Up now"</h2>
            </Tip>
            <For each=up_now key=|id| *id let:actor_id>
                <ActorRow actor_id=actor_id log=log battle=battle />
            </For>
            <Tip topic=Topic::ShapingSection>
                <h2>"Shaping (can be interrupted anytime)"</h2>
            </Tip>
            <For each=shaping_but_not_up key=|id| *id let:actor_id>
                <div class="actor-row">
                    <InterruptControls actor_id=actor_id log=log battle=battle />
                </div>
            </For>
        </div>
    }
}

#[component]
fn ActorRow(actor_id: CombatantId, log: RwSignal<BattleLog>, battle: Memo<Battle>) -> impl IntoView {
    let name = move || {
        battle
            .read()
            .find(actor_id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("#{}", actor_id.0))
    };

    let in_sequence = move || matches!(battle.read().find(actor_id).map(|c| &c.state), Some(CombatantState::InSequence(_)));

    view! {
        <div class="actor-row">
            <span class="name">{name}</span>
            {move || {
                if in_sequence() {
                    view! { <SequenceControls actor_id=actor_id log=log battle=battle /> }.into_any()
                } else {
                    view! { <NormalControls actor_id=actor_id log=log /> }.into_any()
                }
            }}
        </div>
    }
}

fn format_speed(spec: SpeedSpec) -> String {
    match spec {
        SpeedSpec::Fixed(speed) => speed.to_string(),
        SpeedSpec::Variable { default } => format!("varies (default {default})"),
    }
}

fn format_dv_penalty(spec: DvPenaltySpec) -> String {
    match spec {
        DvPenaltySpec::Fixed(penalty) => penalty.to_string(),
        DvPenaltySpec::Variable { default } => format!("varies (default {default})"),
    }
}

#[component]
fn NormalControls(actor_id: CombatantId, log: RwSignal<BattleLog>) -> impl IntoView {
    let speed_override = RwSignal::new(String::new());
    let dv_override = RwSignal::new(String::new());
    let selected_kind = RwSignal::new(0usize);
    let selected_template = move || -> Option<&'static ActionTemplate> { CATALOG.get(selected_kind.get()) };

    let declare = move |_| {
        let Some(template) = selected_template() else { return };
        let speed = speed_override.get().parse().ok();
        let dv = dv_override.get().parse().ok();
        let action = template.declare(speed, dv, None, String::new());
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::DeclareAction { actor: actor_id, action }) {
                tracing::warn!(%error, "could not declare action");
            }
        });
    };

    let start_sequence = move |sequence: fn() -> Sequence| {
        move |_| {
            log.update(|log| {
                if let Err(error) = log.push(BattleEvent::StartSequence { actor: actor_id, sequence: sequence() }) {
                    tracing::warn!(%error, "could not start sequence");
                }
            });
        }
    };

    view! {
        <Tip topic=Topic::ActionSelect>
            <select on:change=move |ev| {
                selected_kind.set(event_target_value(&ev).parse().unwrap_or(0));
            }>
                <For each=|| CATALOG.iter().enumerate() key=|(i, _)| *i let:entry>
                    <option value=entry.0.to_string()>{entry.1.name}</option>
                </For>
            </select>
        </Tip>
        {move || {
            selected_template()
                .map(|template| {
                    let kind_topic = action_topic(template.kind);
                    let kind_entry = kind_topic.entry();
                    view! {
                        <div class="action-summary">
                            <Tip topic=Topic::Speed>
                                <span class="action-summary-chip">"Speed " {format_speed(template.speed)}</span>
                            </Tip>
                            <Tip topic=Topic::DvPenalty>
                                <span class="action-summary-chip">"DV " {format_dv_penalty(template.dv_penalty)}</span>
                            </Tip>
                            <Tip topic=Topic::Reflexive>
                                <span class="action-summary-chip">
                                    "Reflexive: " {if template.reflexive { "yes" } else { "no" }}
                                </span>
                            </Tip>
                            <Tip topic=Topic::Flurryable>
                                <span class="action-summary-chip">
                                    "Flurryable: " {if template.flurryable { "yes" } else { "no" }}
                                </span>
                            </Tip>
                            <Tip topic=kind_topic>
                                <span class="action-summary-note">{kind_entry.what}</span>
                            </Tip>
                        </div>
                    }
                })
        }}
        <Tip topic=Topic::SpeedOverride>
            <input
                placeholder="speed override"
                prop:value=move || speed_override.get()
                on:input=move |ev| speed_override.set(event_target_value(&ev))
            />
        </Tip>
        <Tip topic=Topic::DvOverride>
            <input
                placeholder="DV override"
                prop:value=move || dv_override.get()
                on:input=move |ev| dv_override.set(event_target_value(&ev))
            />
        </Tip>
        <Tip topic=Topic::Declare>
            <button on:click=declare>"Declare"</button>
        </Tip>
        <span class="sorcery-buttons">
            <Tip topic=Topic::ShapeTerrestrial>
                <button on:click=start_sequence(Sequence::shape_terrestrial)>"Shape Terrestrial"</button>
            </Tip>
            <Tip topic=Topic::ShapeCelestial>
                <button on:click=start_sequence(Sequence::shape_celestial)>"Shape Celestial"</button>
            </Tip>
            <Tip topic=Topic::ShapeSolar>
                <button on:click=start_sequence(Sequence::shape_solar)>"Shape Solar"</button>
            </Tip>
        </span>
    }
}

#[component]
fn SequenceControls(actor_id: CombatantId, log: RwSignal<BattleLog>, battle: Memo<Battle>) -> impl IntoView {
    let speed_override = RwSignal::new(String::new());

    let advance = move |_| {
        let speed = speed_override.get().parse().ok();
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::AdvanceSequence { actor: actor_id, speed_override: speed }) {
                tracing::warn!(%error, "could not advance sequence");
            }
        });
    };

    view! {
        <InterruptControls actor_id=actor_id log=log battle=battle />
        <Tip topic=Topic::CastSpeedOverride>
            <input
                placeholder="speed override (Cast)"
                prop:value=move || speed_override.get()
                on:input=move |ev| speed_override.set(event_target_value(&ev))
            />
        </Tip>
        <Tip topic=Topic::AdvanceSequence>
            <button on:click=advance>"Advance"</button>
        </Tip>
    }
}

#[component]
fn InterruptControls(actor_id: CombatantId, log: RwSignal<BattleLog>, battle: Memo<Battle>) -> impl IntoView {
    let rejoin_successes = RwSignal::new(String::new());

    let description = move || {
        let battle = battle.read();
        let Some(combatant) = battle.find(actor_id) else { return String::new() };
        let CombatantState::InSequence(sequence) = &combatant.state else { return String::new() };
        format!(
            "{} — step {}/{}: {} (resolves tick {})",
            sequence.name,
            sequence.current + 1,
            sequence.steps.len(),
            sequence.current_step().label,
            combatant.next_action_tick
        )
    };

    let interrupt = move |_| {
        let rejoin = match rejoin_successes.get().parse::<u32>() {
            Ok(successes) => JoinBattleResult::Successes(successes),
            Err(_) => JoinBattleResult::Botch,
        };
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::InterruptSequence {
                actor: actor_id,
                reason: InterruptReason::Voluntary,
                rejoin,
            }) {
                tracing::warn!(%error, "could not interrupt sequence");
            }
        });
    };

    view! {
        <Tip topic=Topic::SequenceStep>
            <span class="sequence-description">{description}</span>
        </Tip>
        <Tip topic=Topic::RejoinSuccesses>
            <input
                placeholder="rejoin successes"
                prop:value=move || rejoin_successes.get()
                on:input=move |ev| rejoin_successes.set(event_target_value(&ev))
            />
        </Tip>
        <Tip topic=Topic::InterruptSequence>
            <button on:click=interrupt class="interrupt-button">"Interrupt"</button>
        </Tip>
    }
}

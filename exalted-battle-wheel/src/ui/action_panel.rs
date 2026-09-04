use exalted_battle_wheel::battle::{
    Battle, BattleEvent, BattleLog, CombatantId, CombatantState, InterruptReason, JoinBattleResult, Phase,
    Sequence, CATALOG,
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
            <h2>"Up now"</h2>
            <For each=up_now key=|id| *id let:actor_id>
                <ActorRow actor_id=actor_id log=log battle=battle />
            </For>
            <h2>"Shaping (can be interrupted anytime)"</h2>
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

#[component]
fn NormalControls(actor_id: CombatantId, log: RwSignal<BattleLog>) -> impl IntoView {
    let speed_override = RwSignal::new(String::new());
    let dv_override = RwSignal::new(String::new());
    let selected_kind = RwSignal::new(0usize);

    let declare = move |_| {
        let Some(template) = CATALOG.get(selected_kind.get()) else { return };
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
        <select on:change=move |ev| {
            selected_kind.set(event_target_value(&ev).parse().unwrap_or(0));
        }>
            <For each=|| CATALOG.iter().enumerate() key=|(i, _)| *i let:entry>
                <option value=entry.0.to_string()>{entry.1.name}</option>
            </For>
        </select>
        <input placeholder="speed override" prop:value=move || speed_override.get() on:input=move |ev| speed_override.set(event_target_value(&ev)) />
        <input placeholder="DV override" prop:value=move || dv_override.get() on:input=move |ev| dv_override.set(event_target_value(&ev)) />
        <button on:click=declare>"Declare"</button>
        <span class="sorcery-buttons">
            <button on:click=start_sequence(Sequence::shape_terrestrial)>"Shape Terrestrial"</button>
            <button on:click=start_sequence(Sequence::shape_celestial)>"Shape Celestial"</button>
            <button on:click=start_sequence(Sequence::shape_solar)>"Shape Solar"</button>
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
        <input
            placeholder="speed override (Cast)"
            prop:value=move || speed_override.get()
            on:input=move |ev| speed_override.set(event_target_value(&ev))
        />
        <button on:click=advance>"Advance"</button>
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
        <span class="sequence-description">{description}</span>
        <input
            placeholder="rejoin successes"
            prop:value=move || rejoin_successes.get()
            on:input=move |ev| rejoin_successes.set(event_target_value(&ev))
        />
        <button on:click=interrupt class="interrupt-button">"Interrupt"</button>
    }
}

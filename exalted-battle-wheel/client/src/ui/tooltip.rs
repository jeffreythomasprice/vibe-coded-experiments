use crate::ui::glossary::Topic;
use crate::ui::{ticks, Tip};
use shared::battle::{Battle, CombatantId, CombatantState};
use leptos::prelude::*;

pub type Hovered = RwSignal<Option<CombatantId>>;

fn state_topic(state: &CombatantState) -> Topic {
    match state {
        CombatantState::Normal => Topic::StateNormal,
        CombatantState::Guarding => Topic::StateGuarding,
        CombatantState::Aiming { .. } => Topic::StateAiming,
        CombatantState::Inactive => Topic::StateInactive,
        CombatantState::InSequence(_) => Topic::StateInSequence,
    }
}

fn state_description(state: &CombatantState, battle: &Battle) -> String {
    match state {
        CombatantState::Normal => "Normal".to_string(),
        CombatantState::Guarding => "Guarding (may abort into another action)".to_string(),
        CombatantState::Aiming { target } => match target.and_then(|id| battle.find(id)) {
            Some(target) => format!("Aiming at {}", target.name),
            None => "Aiming".to_string(),
        },
        CombatantState::Inactive => "Inactive".to_string(),
        CombatantState::InSequence(sequence) => {
            format!(
                "{} — step {} of {} ({})",
                sequence.name,
                sequence.current + 1,
                sequence.steps.len(),
                sequence.current_step().label
            )
        }
    }
}

#[component]
pub fn HoverCard() -> impl IntoView {
    let hovered = expect_context::<Hovered>();
    let battle = expect_context::<Memo<Battle>>();

    let content = move || {
        let id = hovered.get()?;
        let battle = battle.read();
        let combatant = battle.find(id)?;
        let ticks_until = combatant.next_action_tick as i64 - battle.current_tick as i64;
        Some((
            battle.mode,
            combatant.name.clone(),
            combatant.side.0.clone(),
            combatant.next_action_tick,
            ticks_until,
            combatant.dv.penalty,
            combatant.dv.refreshes_at,
            state_description(&combatant.state, &battle),
            state_topic(&combatant.state),
        ))
    };

    let dismiss = move |_| hovered.set(None);

    view! {
        <div class="hover-card" class:hover-card-visible=move || content().is_some()>
            {move || {
                content()
                    .map(|(mode, name, side, next_tick, ticks_until, dv_penalty, refreshes_at, state, topic)| {
                        let next_label = ticks::at(mode, next_tick);
                        let until_label = ticks::count(mode, ticks_until.unsigned_abs() as u32);
                        view! {
                            <button class="hover-card-dismiss" on:click=dismiss>
                                "\u{2715}"
                            </button>
                            <Tip topic=Topic::CombatantName>
                                <div class="hover-card-title">{name} " (" {side} ")"</div>
                            </Tip>
                            <Tip topic=Topic::NextActionTick>
                                <div class="hover-card-row">
                                    "Next action: " {next_label} " (in " {until_label} ")"
                                </div>
                            </Tip>
                            <div class="hover-card-row">
                                <Tip topic=Topic::DvPenalty>
                                    <span>"DV penalty: " {dv_penalty}</span>
                                </Tip>
                                {refreshes_at.map(|tick| {
                                    let refresh_label = ticks::at(mode, tick);
                                    view! {
                                        <Tip topic=Topic::DvRefresh>
                                            <span>", refreshes at " {refresh_label}</span>
                                        </Tip>
                                    }
                                })}
                            </div>
                            <Tip topic=topic>
                                <div class="hover-card-row">{state}</div>
                            </Tip>
                        }
                    })
            }}
        </div>
    }
}

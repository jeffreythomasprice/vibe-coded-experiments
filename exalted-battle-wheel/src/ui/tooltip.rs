use exalted_battle_wheel::battle::{Battle, CombatantId, CombatantState};
use leptos::prelude::*;

pub type Hovered = RwSignal<Option<CombatantId>>;

fn state_description(state: &CombatantState) -> String {
    match state {
        CombatantState::Normal => "Normal".to_string(),
        CombatantState::Guarding => "Guarding (may abort into another action)".to_string(),
        CombatantState::Aiming { target } => match target {
            Some(target) => format!("Aiming at {target:?}"),
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
            combatant.name.clone(),
            combatant.side.0.clone(),
            combatant.next_action_tick,
            ticks_until,
            combatant.dv.penalty,
            combatant.dv.refreshes_at,
            state_description(&combatant.state),
        ))
    };

    view! {
        <div class="hover-card" class:hover-card-visible=move || content().is_some()>
            {move || {
                content()
                    .map(|(name, side, next_tick, ticks_until, dv_penalty, refreshes_at, state)| {
                        view! {
                            <div class="hover-card-title">{name} " (" {side} ")"</div>
                            <div class="hover-card-row">"Next action: tick " {next_tick} " (in " {ticks_until} " ticks)"</div>
                            <div class="hover-card-row">"DV penalty: " {dv_penalty}
                                {refreshes_at.map(|tick| format!(", refreshes at tick {tick}"))}
                            </div>
                            <div class="hover-card-row">{state}</div>
                        }
                    })
            }}
        </div>
    }
}

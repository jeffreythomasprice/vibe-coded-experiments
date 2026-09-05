use crate::ui::glossary::Topic;
use crate::ui::Tip;
use exalted_battle_wheel::battle::{Battle, BattleEvent, BattleLog, CombatantId, JoinBattleResult, Phase, Side};
use leptos::prelude::*;

#[component]
pub fn Roster() -> impl IntoView {
    let log = expect_context::<RwSignal<BattleLog>>();
    let battle = expect_context::<Memo<Battle>>();

    let name = RwSignal::new(String::new());
    let side = RwSignal::new(String::from("A"));
    let successes = RwSignal::new(0i32);
    let botch = RwSignal::new(false);

    let add_combatant = move |_| {
        let entered_name = name.get();
        if entered_name.trim().is_empty() {
            return;
        }
        let join_battle = if botch.get() {
            JoinBattleResult::Botch
        } else {
            JoinBattleResult::Successes(successes.get().max(0) as u32)
        };
        log.update(|log| {
            let id = log.alloc_combatant_id();
            if let Err(error) = log.push(BattleEvent::AddCombatant {
                id,
                name: entered_name,
                side: Side(side.get()),
                join_battle,
            }) {
                tracing::error!(%error, "could not add combatant");
            }
        });
        name.set(String::new());
        successes.set(0);
        botch.set(false);
    };

    let start_battle = move |_| {
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::StartBattle) {
                tracing::error!(%error, "could not start battle");
            }
        });
    };

    let combatant_ids = move || battle.read().combatants.iter().map(|c| c.id).collect::<Vec<_>>();

    view! {
        <div class="roster">
            <Tip topic=Topic::Roster>
                <h2>"Combatants"</h2>
            </Tip>
            <div class="roster-form">
                <Tip topic=Topic::CombatantName>
                    <input
                        placeholder="Name"
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </Tip>
                <Tip topic=Topic::Side>
                    <input
                        placeholder="Side"
                        prop:value=move || side.get()
                        on:input=move |ev| side.set(event_target_value(&ev))
                    />
                </Tip>
                <Tip topic=Topic::JoinBattleSuccesses>
                    <label class="join-battle-successes-label">
                        "Join Battle successes"
                        <input
                            type="number"
                            prop:value=move || successes.get().to_string()
                            on:input=move |ev| successes.set(event_target_value(&ev).parse().unwrap_or(0))
                            disabled=move || botch.get()
                        />
                    </label>
                </Tip>
                <Tip topic=Topic::Botch>
                    <label>
                        <input
                            type="checkbox"
                            prop:checked=move || botch.get()
                            on:change=move |ev| botch.set(event_target_checked(&ev))
                        />
                        "Botch"
                    </label>
                </Tip>
                <Tip topic=Topic::AddCombatant>
                    <button on:click=add_combatant>"Add"</button>
                </Tip>
            </div>
            <ul class="roster-list">
                <For each=combatant_ids key=|id| *id let:id>
                    <RosterRow id=id battle=battle log=log />
                </For>
            </ul>
            <Tip topic=Topic::StartBattle>
                <button on:click=start_battle disabled=move || !matches!(battle.read().phase, Phase::Setup)>
                    "Start Battle"
                </button>
            </Tip>
        </div>
    }
}

#[component]
fn RosterRow(id: CombatantId, battle: Memo<Battle>, log: RwSignal<BattleLog>) -> impl IntoView {
    let name = move || battle.read().find(id).map(|c| c.name.clone()).unwrap_or_default();
    let side = move || battle.read().find(id).map(|c| c.side.0.clone()).unwrap_or_default();
    let tick = move || battle.read().find(id).map(|c| c.next_action_tick);
    let tick_topic = move || match battle.read().phase {
        Phase::Setup => Topic::FirstAction,
        Phase::Running { .. } => Topic::NextActionTick,
    };

    let remove = move |_| {
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::RemoveCombatant { id }) {
                tracing::error!(%error, "could not remove combatant");
            }
        });
    };

    view! {
        <li>
            <Tip topic=Topic::CombatantName>
                <span class="name">{name}</span>
            </Tip>
            <Tip topic=Topic::Side>
                <span class="side">{side}</span>
            </Tip>
            {move || {
                let topic = tick_topic();
                view! {
                    <Tip topic=topic>
                        <span class="tick">"tick " {tick}</span>
                    </Tip>
                }
            }}
            <Tip topic=Topic::RemoveCombatant>
                <button on:click=remove>"Remove"</button>
            </Tip>
        </li>
    }
}

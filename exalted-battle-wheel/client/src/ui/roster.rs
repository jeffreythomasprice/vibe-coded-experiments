use crate::battle_net::Battles;
use crate::ui::glossary::Topic;
use crate::ui::{Combobox, DetailTip, Tip};
use shared::battle::{Battle, BattleEvent, BattleMode, CombatantId, JoinBattleResult, Phase, Side};
use leptos::prelude::*;

#[component]
pub fn Roster() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let battle = expect_context::<Memo<Battle>>();

    let name = RwSignal::new(String::new());
    let side = RwSignal::new(String::from("A"));
    let successes = RwSignal::new(0i32);
    let botch = RwSignal::new(false);

    let mode = move || battle.read().mode;
    let in_setup = move || matches!(battle.read().phase, Phase::Setup);
    let sides = Signal::derive(move || battle.read().sides());
    let set_mode = move |mode: BattleMode| battles.push(BattleEvent::SetMode { mode });

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
        // A typed side that matches an existing faction apart from casing joins that faction
        // instead of splitting it into a second one.
        let typed_side = side.get();
        let entered_side = battle
            .read_untracked()
            .canonical_side(typed_side.trim())
            .unwrap_or_else(|| Side(typed_side.trim().to_string()));
        battles.push_minting(BattleEvent::AddCombatant {
            id: CombatantId(0),
            name: entered_name,
            side: entered_side.clone(),
            join_battle,
        });
        name.set(String::new());
        successes.set(0);
        botch.set(false);
        // Side is deliberately not cleared, so adding a group only means retyping the name — but
        // it's rewritten to the canonical spelling so the field reflects what was just recorded.
        side.set(entered_side.0);
    };

    let start_battle = move |_| battles.push(BattleEvent::StartBattle);

    let combatant_ids = move || battle.read().combatants.iter().map(|c| c.id).collect::<Vec<_>>();

    view! {
        <div class="roster">
            <Tip topic=Topic::Roster>
                <h2>"Combatants"</h2>
            </Tip>
            <div class="battle-mode-row">
                <Tip topic=Topic::BattleModeSelect>
                    <label class="header-control battle-mode-select">
                        "Battle mode"
                        <select
                            prop:value=move || match mode() {
                                BattleMode::Personal => "personal",
                                BattleMode::Mass => "mass",
                                BattleMode::Social => "social",
                            }
                            disabled=move || battles.read_only().get() || !in_setup()
                            on:change=move |ev| {
                                set_mode(match event_target_value(&ev).as_str() {
                                    "mass" => BattleMode::Mass,
                                    "social" => BattleMode::Social,
                                    _ => BattleMode::Personal,
                                });
                            }
                        >
                            <option value="personal">"Personal combat"</option>
                            <option value="mass">"Mass combat"</option>
                            <option value="social">"Social combat"</option>
                        </select>
                    </label>
                </Tip>
            </div>
            <div class="roster-form">
                <Tip topic=Topic::CombatantName>
                    <input
                        placeholder="Name"
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </Tip>
                <Tip topic=Topic::Side>
                    <Combobox value=side options=sides list_id="roster-side-options" placeholder="Side" />
                </Tip>
                <DetailTip
                    topic=Signal::derive(move || match mode() {
                        BattleMode::Personal => Topic::JoinBattleSuccesses,
                        BattleMode::Mass => Topic::JoinWar,
                        BattleMode::Social => Topic::JoinDebate,
                    })
                    detail=Signal::derive(String::new)
                >
                    <label class="join-battle-successes-label">
                        {move || format!("{} successes", mode().join_roll_name())}
                        <input
                            type="number"
                            prop:value=move || successes.get().to_string()
                            on:input=move |ev| successes.set(event_target_value(&ev).parse().unwrap_or(0))
                            disabled=move || botch.get()
                        />
                    </label>
                </DetailTip>
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
                    <button on:click=add_combatant disabled=move || battles.read_only().get()>"Add"</button>
                </Tip>
            </div>
            <ul class="roster-list">
                <For each=combatant_ids key=|id| *id let:id>
                    <RosterRow id=id battle=battle battles=battles />
                </For>
            </ul>
            <Tip topic=Topic::StartBattle>
                <button
                    on:click=start_battle
                    disabled=move || battles.read_only().get() || !matches!(battle.read().phase, Phase::Setup)
                >
                    "Start Battle"
                </button>
            </Tip>
        </div>
    }
}

#[component]
fn RosterRow(id: CombatantId, battle: Memo<Battle>, battles: Battles) -> impl IntoView {
    let name = move || battle.read().find(id).map(|c| c.name.clone()).unwrap_or_default();
    let side = move || battle.read().find(id).map(|c| c.side.0.clone()).unwrap_or_default();
    let tick = move || battle.read().find(id).map(|c| c.next_action_tick);
    let tick_topic = move || match battle.read().phase {
        Phase::Setup => Topic::FirstAction,
        Phase::Running { .. } => Topic::NextActionTick,
    };

    let remove = move |_| battles.push(BattleEvent::RemoveCombatant { id });

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
                let mode = battle.read().mode;
                let label = tick().map(|t| crate::ui::ticks::at(mode, t)).unwrap_or_default();
                view! {
                    <Tip topic=topic>
                        <span class="tick">{label}</span>
                    </Tip>
                }
            }}
            <Tip topic=Topic::RemoveCombatant>
                <button on:click=remove disabled=move || battles.read_only().get()>"Remove"</button>
            </Tip>
        </li>
    }
}

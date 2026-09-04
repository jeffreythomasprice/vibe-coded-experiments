use crate::ui::{ActionPanel, HoverCard, Hovered, Roster, Wheel};
use exalted_battle_wheel::battle::{BattleEvent, BattleLog, CombatantId};
use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    tracing::trace!("rendering App");

    let log = RwSignal::new(BattleLog::new());
    provide_context(log);

    let battle = Memo::new(move |_| log.read().battle());
    provide_context(battle);

    let hovered: Hovered = RwSignal::new(None::<CombatantId>);
    provide_context(hovered);

    let advance_tick = move |_| {
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::AdvanceTick) {
                tracing::warn!(%error, "could not advance tick");
            }
        });
    };
    let undo = move |_| log.update(|log| _ = log.undo());
    let redo = move |_| log.update(|log| _ = log.redo());

    view! {
        <div class="app">
            <header class="app-header">
                <h1>"Exalted Battle Wheel"</h1>
                <button on:click=undo disabled=move || !log.read().can_undo()>
                    "Undo"
                </button>
                <button on:click=redo disabled=move || !log.read().can_redo()>
                    "Redo"
                </button>
                <div class="tick-readout">"Tick " {move || battle.read().current_tick}</div>
                <button on:click=advance_tick>"Advance Tick"</button>
            </header>
            <div class="app-body">
                <Roster />
                <div class="wheel-column">
                    <Wheel />
                    <HoverCard />
                </div>
                <ActionPanel />
            </div>
        </div>
    }
}

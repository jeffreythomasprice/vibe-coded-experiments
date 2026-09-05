use crate::prefs::{Prefs, Theme};
use crate::ui::glossary::Topic;
use crate::ui::{ActionPanel, ActiveTip, EventLogButton, HoverCard, Hovered, QueuePanel, Roster, Tip, TipLayer, Wheel};
use exalted_battle_wheel::battle::{BattleEvent, BattleLog, CombatantId, Phase};
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

    let active_tip: ActiveTip = RwSignal::new(None);
    provide_context(active_tip);
    let prefs = Prefs::load();
    provide_context(prefs);

    let advance_tick = move |_| {
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::AdvanceTick) {
                tracing::error!(%error, "could not advance tick");
            }
        });
    };
    let undo = move |_| log.update(|log| _ = log.undo());
    let redo = move |_| log.update(|log| _ = log.redo());

    let reaction_count = move || match battle.read().phase {
        Phase::Running { .. } => Some(battle.read().reaction_count()),
        Phase::Setup => None,
    };

    view! {
        <div class="app">
            <TipLayer />
            <header class="app-header">
                <Tip topic=Topic::AppOverview>
                    <h1>"Exalted Battle Wheel"</h1>
                </Tip>
                <Tip topic=Topic::Undo>
                    <button on:click=undo disabled=move || !log.read().can_undo()>
                        "Undo"
                    </button>
                </Tip>
                <Tip topic=Topic::Redo>
                    <button on:click=redo disabled=move || !log.read().can_redo()>
                        "Redo"
                    </button>
                </Tip>
                <EventLogButton />
                <Tip topic=Topic::CurrentTick>
                    <div class="tick-readout">"Tick " {move || battle.read().current_tick}</div>
                </Tip>
                {move || {
                    reaction_count()
                        .map(|count| {
                            view! {
                                <Tip topic=Topic::ReactionCount>
                                    <div class="reaction-count-readout">"Reaction count " {count}</div>
                                </Tip>
                            }
                        })
                }}
                <Tip topic=Topic::AdvanceTick>
                    <button on:click=advance_tick>"Advance Tick"</button>
                </Tip>
                <Tip topic=Topic::TeachingMode>
                    <label class="header-control">
                        <input
                            type="checkbox"
                            prop:checked=move || prefs.teaching_mode.get()
                            on:change=move |ev| prefs.teaching_mode.set(event_target_checked(&ev))
                        />
                        "Teaching mode"
                    </label>
                </Tip>
                <Tip topic=Topic::Theme>
                    <label class="header-control">
                        "Theme"
                        <select
                            prop:value=move || match prefs.theme.get() {
                                Theme::System => "system",
                                Theme::Light => "light",
                                Theme::Dark => "dark",
                            }
                            on:change=move |ev| {
                                prefs.theme.set(match event_target_value(&ev).as_str() {
                                    "light" => Theme::Light,
                                    "dark" => Theme::Dark,
                                    _ => Theme::System,
                                });
                            }
                        >
                            <option value="system">"System"</option>
                            <option value="light">"Light"</option>
                            <option value="dark">"Dark"</option>
                        </select>
                    </label>
                </Tip>
            </header>
            <div class="app-body">
                <div class="side-column">
                    <Roster />
                    <QueuePanel />
                </div>
                <div class="wheel-column">
                    <Wheel />
                    <HoverCard />
                </div>
                <ActionPanel />
            </div>
        </div>
    }
}

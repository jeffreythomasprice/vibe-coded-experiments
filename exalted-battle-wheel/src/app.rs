use crate::persist::Persisted;
use crate::prefs::{Prefs, Theme};
use crate::ui::glossary::Topic;
use crate::ui::ticks;
use crate::ui::{
    ActionPanel, ActiveTip, DetailTip, EventLogButton, HoverCard, Hovered, Modal, QueuePanel, RailSelection,
    ReferenceRail, Roster, Tip, TipLayer, ToastLayer, Toasts, Wheel,
};
use exalted_battle_wheel::battle::{BattleEvent, BattleLog, CombatantId, Phase};
use leptos::prelude::*;

/// "tick" -> "Tick", "long tick" -> "Long Tick" — for button labels built from `BattleMode`'s
/// lowercase nouns.
fn capitalize(words: &str) -> String {
    words.split(' ').map(|word| {
        let mut chars = word.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }).collect::<Vec<_>>().join(" ")
}

#[component]
pub fn App() -> impl IntoView {
    tracing::trace!("rendering App");

    // Provided before any `Persisted` value loads, so a load failure has a layer to report to.
    let toasts: Toasts = RwSignal::new(Vec::new());
    provide_context(toasts);

    let battle_log = Persisted::new("battle", BattleLog::new);
    let log = *battle_log;
    provide_context(log);

    let battle = Memo::new(move |_| log.read().battle());
    provide_context(battle);

    let hovered: Hovered = RwSignal::new(None::<CombatantId>);
    provide_context(hovered);

    let active_tip: ActiveTip = RwSignal::new(None);
    provide_context(active_tip);
    let prefs = Prefs::load();
    provide_context(prefs);

    provide_context(RailSelection::new());

    let confirming_reset = RwSignal::new(false);
    let reset = move || {
        battle_log.reset();
        confirming_reset.set(false);
    };

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
            <ToastLayer />
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
                <DetailTip
                    topic=Topic::CurrentTick
                    detail=Signal::derive(move || battle.read().mode.tick_note().unwrap_or_default().to_string())
                >
                    <div class="tick-readout">{move || {
                        let battle = battle.read();
                        ticks::at(battle.mode, battle.current_tick)
                    }}</div>
                </DetailTip>
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
                <DetailTip
                    topic=Topic::AdvanceTick
                    detail=Signal::derive(move || battle.read().mode.tick_note().unwrap_or_default().to_string())
                >
                    <button on:click=advance_tick>{move || format!("Advance {}", capitalize(battle.read().mode.tick_noun()))}</button>
                </DetailTip>
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
                <Tip topic=Topic::Reset>
                    <button
                        class="reset-button"
                        disabled=move || log.read().events().is_empty()
                        on:click=move |_| confirming_reset.set(true)
                    >
                        "Reset"
                    </button>
                </Tip>
            </header>
            {move || {
                confirming_reset
                    .get()
                    .then(|| {
                        view! {
                            <Modal title="Reset battle?" on_close=move || confirming_reset.set(false)>
                                <p class="reset-warning">
                                    "This clears every combatant, declared action, marker, and undo step, and the copy saved in this browser. Undo cannot bring it back. Saved actions, theme, and Teaching mode are kept."
                                </p>
                                <div class="reset-actions">
                                    <button class="btn" on:click=move |_| confirming_reset.set(false)>"Cancel"</button>
                                    <button class="btn reset-confirm" on:click=move |_| reset()>"Reset battle"</button>
                                </div>
                            </Modal>
                        }
                    })
            }}
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
                <ReferenceRail />
            </div>
        </div>
    }
}

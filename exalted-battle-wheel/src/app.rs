use crate::battle_net::{BattleView, Battles};
use crate::persist::Persisted;
use crate::prefs::{Prefs, Theme};
use crate::ui::glossary::Topic;
use crate::ui::ticks;
use crate::ui::{
    ActionPanel, ActiveTip, DetailTip, EventLogButton, HoverCard, Hovered, Modal, PendingJoin, QueuePanel,
    RailSelection, ReferenceRail, RoomButton, Roster, Tip, TipLayer, ToastLayer, Toasts, Wheel,
};
use exalted_battle_wheel::battle::{BattleEvent, BattleLog, CombatantId, Phase};
use leptos::prelude::*;
use wasm_bindgen::JsValue;

/// Reads a one-shot `#j=<code>&stun=<url>,<url>` fragment (an invite link, or a bare STUN
/// override), applies the STUN override immediately, and clears the fragment via
/// `replace_state` so a refresh doesn't re-open a spent invite. Both params ride the fragment
/// rather than the query string: fragments never reach CloudFront, so an invite code never lands
/// in an access log, and can't perturb the CDN's cache key either.
fn consume_invite_fragment() -> Option<String> {
    let window = web_sys::window()?;
    let location = window.location();
    let hash = location.hash().ok()?;
    let query = hash.strip_prefix('#').unwrap_or(&hash);
    if query.is_empty() {
        return None;
    }
    let params = web_sys::UrlSearchParams::new_with_str(query).ok()?;

    if let Some(stun) = params.get("stun") {
        let servers: Vec<String> = stun.split(',').map(str::trim).filter(|url| !url.is_empty()).map(str::to_string).collect();
        if !servers.is_empty() {
            crate::net::set_stun_servers(servers);
        }
    }
    let join_code = params.get("j");

    let cleared_url = format!("{}{}", location.pathname().unwrap_or_default(), location.search().unwrap_or_default());
    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&cleared_url));
    }

    join_code
}

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

    // Captured once, here, where a real reactive owner is guaranteed current — `net::session`'s
    // deferred WebRTC-callback handlers have no owner of their own to work with and re-enter this
    // one explicitly instead. See `session.rs`'s `ROOT_OWNER` doc comment for why that's necessary.
    if let Some(owner) = Owner::current() {
        crate::net::set_root_owner(owner);
    }

    // Read once, synchronously, before anything below could open a room of its own — applies any
    // `#stun=` override immediately and hands the `#j=` invite code (if any) to `RoomButton`.
    let pending_join = PendingJoin(RwSignal::new(consume_invite_fragment()));
    provide_context(pending_join);

    // Created before the log it gates, and threaded into both — see `Session::new`'s doc comment.
    let room_active = RwSignal::new(false);
    let battle_log = Persisted::new_gated("battle", BattleLog::new, move || !room_active.get());
    let log = *battle_log;
    let battles = Battles::new(log, room_active);
    provide_context(battles);
    provide_context(log.read_only() as BattleView);

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
        battles.reset();
        confirming_reset.set(false);
    };

    let advance_tick = move |_| battles.push(BattleEvent::AdvanceTick);
    let undo = move |_| battles.undo();
    let redo = move |_| battles.redo();

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
                <RoomButton />
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
            <div class="app-body" class:app-busy=move || battles.busy().get()>
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

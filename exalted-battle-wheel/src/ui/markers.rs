use crate::ui::glossary::Topic;
use crate::ui::Tip;
use exalted_battle_wheel::battle::{Battle, BattleEvent, BattleLog, CombatantId, Phase};
use leptos::prelude::*;

/// The add-marker form; the markers themselves (active and pending) are listed and edited from
/// `QueuePanel`.
#[component]
pub fn MarkerForm() -> impl IntoView {
    let log = expect_context::<RwSignal<BattleLog>>();
    let battle = expect_context::<Memo<Battle>>();

    let label = RwSignal::new(String::new());
    let offset = RwSignal::new(String::from("0"));
    let ticks = RwSignal::new(String::from("1"));
    let source = RwSignal::new(String::new());

    let add = move |_| {
        let Ok(source_id) = source.get().parse::<u32>() else { return };
        let entered_label = label.get();
        if entered_label.trim().is_empty() {
            return;
        }
        let offset_ticks: u32 = offset.get().parse().unwrap_or(0);
        let duration: u32 = ticks.get().parse().unwrap_or(1).max(1);
        log.update(|log| {
            let id = log.alloc_marker_id();
            let at_tick = log.battle().current_tick + offset_ticks;
            if let Err(error) = log.push(BattleEvent::AddMarker {
                id,
                label: entered_label,
                source: CombatantId(source_id),
                at_tick,
                ticks: duration,
            }) {
                tracing::error!(%error, "could not add marker");
            }
        });
        label.set(String::new());
        offset.set(String::from("0"));
        ticks.set(String::from("1"));
    };

    let running = move || matches!(battle.read().phase, Phase::Running { .. });

    view! {
        {move || {
            running().then(|| view! {
                <div class="marker-list-form">
                    <Tip topic=Topic::Markers>
                        <input
                            placeholder="Label"
                            prop:value=move || label.get()
                            on:input=move |ev| label.set(event_target_value(&ev))
                        />
                    </Tip>
                    <Tip topic=Topic::MarkerDuration>
                        <label class="marker-form-field">
                            "in"
                            <input
                                type="number"
                                prop:value=move || offset.get()
                                on:input=move |ev| offset.set(event_target_value(&ev))
                            />
                            "ticks"
                        </label>
                    </Tip>
                    <Tip topic=Topic::MarkerDuration>
                        <label class="marker-form-field">
                            "for"
                            <input
                                type="number"
                                min="1"
                                prop:value=move || ticks.get()
                                on:input=move |ev| ticks.set(event_target_value(&ev))
                            />
                            "ticks"
                        </label>
                    </Tip>
                    <select prop:value=move || source.get() on:change=move |ev| source.set(event_target_value(&ev))>
                        <option value="">"Source\u{2026}"</option>
                        {move || {
                            battle.read().combatants.iter().map(|c| {
                                view! { <option value=c.id.0.to_string()>{c.name.clone()}</option> }
                            }).collect_view()
                        }}
                    </select>
                    <button on:click=add>"Add marker"</button>
                </div>
            })
        }}
    }
}

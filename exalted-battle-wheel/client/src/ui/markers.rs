use crate::battle_net::Battles;
use crate::ui::glossary::Topic;
use crate::ui::{Tip, ticks};
use leptos::prelude::*;
use shared::battle::{Battle, BattleEvent, BattleMode, CombatantId, MarkerId, Phase, Tick};

/// How the add-marker form's start value is interpreted: a delay from now, a backdate from now,
/// or an absolute tick. All three resolve to the same `at_tick` the engine actually stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartMode {
    In,
    Ago,
    OnTick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpanError {
    BeforeTickZero,
    AlreadyEnded { last_tick: Tick },
    OutOfRange,
}

impl SpanError {
    fn message(self, mode: BattleMode) -> String {
        match self {
            SpanError::BeforeTickZero => "That start is before tick 0.".to_string(),
            SpanError::AlreadyEnded { last_tick } => {
                format!("That span already ended on {} \u{2014} it would never appear.", ticks::at(mode, last_tick))
            }
            SpanError::OutOfRange => "That start is out of range.".to_string(),
        }
    }
}

/// Resolves the form's `(mode, value, duration)` into an absolute `at_tick`, rejecting a start
/// before tick 0 and a span that has already fully elapsed (it would never appear on the wheel
/// or in the queue). A span ending exactly on `now` is still valid — it's the marker's last
/// visible tick.
fn resolve_span(mode: StartMode, value: u32, duration: u32, now: Tick) -> Result<Tick, SpanError> {
    let start = match mode {
        StartMode::In => now.checked_add(value).ok_or(SpanError::OutOfRange)?,
        StartMode::Ago => now.checked_sub(value).ok_or(SpanError::BeforeTickZero)?,
        StartMode::OnTick => value,
    };
    let last_tick = start.checked_add(duration.saturating_sub(1)).ok_or(SpanError::OutOfRange)?;
    if last_tick < now {
        return Err(SpanError::AlreadyEnded { last_tick });
    }
    Ok(start)
}

/// The relative-to-now half of the form's preview line, independent of the span itself.
fn relative_label(mode: BattleMode, start: Tick, now: Tick) -> String {
    if start > now {
        format!("starts in {}", ticks::count(mode, start - now))
    } else if start == now {
        "starts now".to_string()
    } else {
        format!("started {} ago", ticks::count(mode, now - start))
    }
}

/// The add-marker form; the markers themselves (active and pending) are listed and edited from
/// `QueuePanel`.
#[component]
pub fn MarkerForm() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let battle = expect_context::<Memo<Battle>>();

    let label = RwSignal::new(String::new());
    let start_mode = RwSignal::new(StartMode::In);
    let start_value = RwSignal::new(String::from("0"));
    let ticks_field = RwSignal::new(String::from("1"));
    let source = RwSignal::new(String::new());

    let on_start_mode_change = move |ev| {
        let new_mode = match event_target_value(&ev).as_str() {
            "ago" => StartMode::Ago,
            "on_tick" => StartMode::OnTick,
            _ => StartMode::In,
        };
        if new_mode == StartMode::OnTick {
            start_value.set(battle.read_untracked().current_tick.to_string());
        } else if start_mode.get_untracked() == StartMode::OnTick {
            start_value.set(String::from("0"));
        }
        start_mode.set(new_mode);
    };

    let span = move || {
        let value: u32 = start_value.get().trim().parse().unwrap_or(0);
        let duration: u32 = ticks_field.get().trim().parse().unwrap_or(1).max(1);
        let now = battle.read().current_tick;
        resolve_span(start_mode.get(), value, duration, now).map(|start| (start, duration, now))
    };

    let add = move |_| {
        let Ok(source_id) = source.get().parse::<u32>() else { return };
        let entered_label = label.get();
        if entered_label.trim().is_empty() {
            return;
        }
        let Ok((at_tick, duration, _)) = span() else { return };
        battles.push_minting(BattleEvent::AddMarker {
            id: MarkerId(0),
            label: shared::battle::label(entered_label),
            source: CombatantId(source_id),
            at_tick,
            ticks: duration,
        });
        label.set(String::new());
        start_mode.set(StartMode::In);
        start_value.set(String::from("0"));
        ticks_field.set(String::from("1"));
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
                    <Tip topic=Topic::MarkerStart>
                        <label class="marker-form-field">
                            <select
                                prop:value=move || match start_mode.get() {
                                    StartMode::In => "in",
                                    StartMode::Ago => "ago",
                                    StartMode::OnTick => "on_tick",
                                }
                                on:change=on_start_mode_change
                            >
                                <option value="in">"in"</option>
                                <option value="ago">"ago"</option>
                                <option value="on_tick">"on tick"</option>
                            </select>
                            <input
                                type="number"
                                prop:value=move || start_value.get()
                                on:input=move |ev| start_value.set(event_target_value(&ev))
                            />
                            {move || (start_mode.get() != StartMode::OnTick).then_some("ticks")}
                        </label>
                    </Tip>
                    <Tip topic=Topic::MarkerDuration>
                        <label class="marker-form-field">
                            "for"
                            <input
                                type="number"
                                min="1"
                                prop:value=move || ticks_field.get()
                                on:input=move |ev| ticks_field.set(event_target_value(&ev))
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
                    <button on:click=add disabled=move || battles.read_only().get() || span().is_err()>"Add marker"</button>
                    {move || {
                        let mode = battle.read().mode;
                        match span() {
                            Ok((start, duration, now)) => view! {
                                <span class="marker-form-preview">
                                    {format!("\u{2192} {} ({})", ticks::span(mode, start, duration), relative_label(mode, start, now))}
                                </span>
                            }.into_any(),
                            Err(e) => view! {
                                <span class="marker-form-preview marker-form-preview-error">{e.message(mode)}</span>
                            }.into_any(),
                        }
                    }}
                </div>
            })
        }}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_mode_adds_the_offset_to_now() {
        assert_eq!(resolve_span(StartMode::In, 3, 1, 10), Ok(13));
    }

    #[test]
    fn ago_mode_subtracts_the_offset_from_now() {
        assert_eq!(resolve_span(StartMode::Ago, 2, 15, 10), Ok(8));
    }

    #[test]
    fn on_tick_mode_uses_the_value_directly() {
        assert_eq!(resolve_span(StartMode::OnTick, 12, 1, 10), Ok(12));
    }

    #[test]
    fn ago_past_tick_zero_is_rejected() {
        assert_eq!(resolve_span(StartMode::Ago, 11, 1, 10), Err(SpanError::BeforeTickZero));
    }

    #[test]
    fn a_span_already_fully_elapsed_is_rejected() {
        assert_eq!(
            resolve_span(StartMode::OnTick, 3, 5, 10),
            Err(SpanError::AlreadyEnded { last_tick: 7 })
        );
    }

    #[test]
    fn a_span_ending_exactly_on_now_is_accepted() {
        assert_eq!(resolve_span(StartMode::OnTick, 6, 5, 10), Ok(6));
    }

    #[test]
    fn an_overflowing_start_is_rejected() {
        assert_eq!(resolve_span(StartMode::In, u32::MAX, 1, 10), Err(SpanError::OutOfRange));
    }
}

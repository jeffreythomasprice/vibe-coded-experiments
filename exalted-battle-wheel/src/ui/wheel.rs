use crate::ui::glossary::Topic;
use crate::ui::tip::{
    on_focus_in, on_focus_in_text, on_focus_out, on_focus_out_text, on_pointer_enter, on_pointer_enter_text,
    on_pointer_leave, on_pointer_leave_text,
};
use crate::ui::{Hovered, Tip};
use exalted_battle_wheel::battle::{Battle, Combatant, Marker, Tick};
use leptos::prelude::*;

const SLOT_COUNT: i64 = 12;
const VIEW_SIZE: f64 = 420.0;
const CENTER: f64 = VIEW_SIZE / 2.0;
const RING_RADIUS: f64 = 160.0;
const LABEL_RADIUS: f64 = 188.0;
const TOKEN_RADIUS: f64 = 160.0;
const TOKEN_FAN_DEGREES: f64 = 9.0;
const TOKEN_R: f64 = 15.0;
const MARKER_RADIUS: f64 = 130.0;
const MARKER_FAN_DEGREES: f64 = 9.0;
const MARKER_R: f64 = 8.0;

fn slot_step() -> f64 {
    360.0 / SLOT_COUNT as f64
}

fn slot_of(tick: Tick) -> i64 {
    (tick as i64).rem_euclid(SLOT_COUNT)
}

/// The absolute tick currently occupying a fixed visual slot, given the current tick: the
/// unique value in `[now, now + SLOT_COUNT - 1]` congruent to `slot` modulo `SLOT_COUNT`.
fn label_for_slot(slot: i64, now: Tick) -> Tick {
    let now = now as i64;
    (now + (slot - now).rem_euclid(SLOT_COUNT)) as Tick
}

fn in_horizon(tick: Tick, now: Tick) -> bool {
    let delta = tick as i64 - now as i64;
    (0..SLOT_COUNT).contains(&delta)
}

fn point_on_circle(radius: f64, angle_deg: f64) -> (f64, f64) {
    let angle_rad = angle_deg.to_radians();
    (CENTER + radius * angle_rad.sin(), CENTER - radius * angle_rad.cos())
}

fn side_color_var(side: &str) -> &'static str {
    let sum: u32 = side.bytes().map(u32::from).sum();
    match sum % 4 {
        0 => "var(--side-a)",
        1 => "var(--side-b)",
        2 => "var(--side-c)",
        _ => "var(--side-d)",
    }
}

#[component]
pub fn Wheel() -> impl IntoView {
    let battle = expect_context::<Memo<Battle>>();

    // transform-box must be set explicitly: SVG elements do not default the transform-origin
    // reference box to the viewBox, so an unset transform-origin sends rotated content wildly
    // off-canvas.
    let group_rotation = move || {
        let now = battle.read().current_tick as f64;
        format!(
            "transform: rotate({}deg); transform-origin: {CENTER}px {CENTER}px; transform-box: view-box;",
            -now * slot_step()
        )
    };

    let now_marker = {
        let (x, y) = point_on_circle(RING_RADIUS, 0.0);
        format!("M {CENTER} {} L {} {} L {} {}", CENTER - RING_RADIUS - 14.0, x - 8.0, y - 18.0, x + 8.0, y - 18.0)
    };

    let slots = move || (0..SLOT_COUNT).collect::<Vec<_>>();

    let over_horizon = move || {
        let battle = battle.read();
        let now = battle.current_tick;
        let mut beyond: Vec<Combatant> =
            battle.combatants.iter().filter(|c| !in_horizon(c.next_action_tick, now)).cloned().collect();
        beyond.sort_by_key(|c| c.next_action_tick);
        beyond
    };

    view! {
        <div class="wheel-panel">
            <svg viewBox=format!("0 0 {VIEW_SIZE} {VIEW_SIZE}") class="wheel">
                <circle
                    cx=CENTER
                    cy=CENTER
                    r=RING_RADIUS
                    class="wheel-ring"
                    tabindex="0"
                    on:pointerenter=on_pointer_enter(Topic::TickWheel)
                    on:pointerleave=on_pointer_leave(Topic::TickWheel)
                    on:focusin=on_focus_in(Topic::TickWheel)
                    on:focusout=on_focus_out(Topic::TickWheel)
                />
                <g style=group_rotation class="wheel-ring-group">
                    <For each=slots key=|slot| *slot let:slot>
                        <WheelSlot slot_index=slot battle=battle />
                    </For>
                </g>
                <path
                    d=now_marker
                    class="now-marker"
                    tabindex="0"
                    on:pointerenter=on_pointer_enter(Topic::NowMarker)
                    on:pointerleave=on_pointer_leave(Topic::NowMarker)
                    on:focusin=on_focus_in(Topic::NowMarker)
                    on:focusout=on_focus_out(Topic::NowMarker)
                />
                <text
                    x=CENTER
                    y=CENTER
                    class="center-tick"
                    tabindex="0"
                    on:pointerenter=on_pointer_enter(Topic::CurrentTick)
                    on:pointerleave=on_pointer_leave(Topic::CurrentTick)
                    on:focusin=on_focus_in(Topic::CurrentTick)
                    on:focusout=on_focus_out(Topic::CurrentTick)
                >
                    {move || battle.read().current_tick}
                </text>
            </svg>
            <div class="over-horizon">
                <Tip topic=Topic::BeyondHorizon>
                    <h3>"Beyond the horizon"</h3>
                </Tip>
                <For each=over_horizon key=|c| c.id let:combatant>
                    <div class="over-horizon-entry">
                        <Tip topic=Topic::CombatantName>
                            <span class="name">{combatant.name.clone()}</span>
                        </Tip>
                        <Tip topic=Topic::NextActionTick>
                            <span class="tick">"tick " {combatant.next_action_tick}</span>
                        </Tip>
                    </div>
                </For>
            </div>
        </div>
    }
}

#[component]
fn WheelSlot(slot_index: i64, battle: Memo<Battle>) -> impl IntoView {
    let (label_x, label_y) = point_on_circle(LABEL_RADIUS, slot_index as f64 * slot_step());

    let counter_rotation_style = move || {
        let now = battle.read().current_tick as f64;
        format!(
            "transform: rotate({}deg); transform-origin: {label_x}px {label_y}px; transform-box: view-box;",
            now * slot_step()
        )
    };

    let label = move || label_for_slot(slot_index, battle.read().current_tick);

    let tokens = move || -> Vec<(usize, usize, Combatant)> {
        let battle = battle.read();
        let now = battle.current_tick;
        let mut here: Vec<Combatant> = battle
            .combatants
            .iter()
            .filter(|c| slot_of(c.next_action_tick) == slot_index && in_horizon(c.next_action_tick, now))
            .cloned()
            .collect();
        here.sort_by_key(|c| c.id.0);
        let total = here.len();
        here.into_iter().enumerate().map(|(i, c)| (i, total, c)).collect()
    };

    let markers = move || -> Vec<(usize, usize, Marker)> {
        let battle = battle.read();
        let now = battle.current_tick;
        let tick = label_for_slot(slot_index, now);
        let mut here: Vec<Marker> =
            battle.active_markers().filter(|m| m.covers(tick) && slot_of(tick) == slot_index).cloned().collect();
        here.sort_by_key(|m| m.id.0);
        let total = here.len();
        here.into_iter().enumerate().map(|(i, m)| (i, total, m)).collect()
    };

    view! {
        <g class="wheel-slot">
            <text
                x=label_x
                y=label_y
                style=counter_rotation_style
                class="slot-label"
                tabindex="0"
                on:pointerenter=on_pointer_enter(Topic::TickSlot)
                on:pointerleave=on_pointer_leave(Topic::TickSlot)
                on:focusin=on_focus_in(Topic::TickSlot)
                on:focusout=on_focus_out(Topic::TickSlot)
            >
                {label}
            </text>
            <For each=tokens key=|(_, _, c)| c.id let:entry>
                <WheelToken slot_index=slot_index index=entry.0 total=entry.1 combatant=entry.2 />
            </For>
            <For each=markers key=|(_, _, m)| m.id let:entry>
                <WheelMarker slot_index=slot_index index=entry.0 total=entry.1 marker=entry.2 />
            </For>
        </g>
    }
}

#[component]
fn WheelMarker(slot_index: i64, index: usize, total: usize, marker: Marker) -> impl IntoView {
    let fan_offset = (index as f64 - (total.saturating_sub(1)) as f64 / 2.0) * MARKER_FAN_DEGREES;
    let angle = slot_index as f64 * slot_step() + fan_offset;
    let (x, y) = point_on_circle(MARKER_RADIUS, angle);
    let battle = expect_context::<Memo<Battle>>();

    let counter_rotation_style = move || {
        let now = battle.read().current_tick as f64;
        format!(
            "transform: rotate({}deg); transform-origin: {x}px {y}px; transform-box: view-box;",
            now * slot_step()
        )
    };

    let span = if marker.ticks <= 1 { format!("tick {}", marker.at_tick) } else { format!("ticks {}\u{2013}{}", marker.at_tick, marker.last_tick()) };
    let tip_text = format!("{} ({span})", marker.label);

    view! {
        <g
            style=counter_rotation_style
            class="wheel-marker"
            tabindex="0"
            on:pointerenter=on_pointer_enter_text(tip_text.clone())
            on:pointerleave=on_pointer_leave_text(tip_text.clone())
            on:focusin=on_focus_in_text(tip_text.clone())
            on:focusout=on_focus_out_text(tip_text)
        >
            <circle cx=x cy=y r=MARKER_R />
        </g>
    }
}

#[component]
fn WheelToken(slot_index: i64, index: usize, total: usize, combatant: Combatant) -> impl IntoView {
    let fan_offset = (index as f64 - (total.saturating_sub(1)) as f64 / 2.0) * TOKEN_FAN_DEGREES;
    let angle = slot_index as f64 * slot_step() + fan_offset;
    let (x, y) = point_on_circle(TOKEN_RADIUS, angle);
    let battle = expect_context::<Memo<Battle>>();
    let hovered = expect_context::<Hovered>();
    let color = side_color_var(&combatant.side.0);
    let initial = combatant.name.chars().next().unwrap_or('?').to_string();
    let id = combatant.id;

    let counter_rotation_style = move || {
        let now = battle.read().current_tick as f64;
        format!(
            "transform: rotate({}deg); transform-origin: {x}px {y}px; transform-box: view-box;",
            now * slot_step()
        )
    };

    // Deliberately sticky: leaving or blurring the token does not clear `hovered`, so the hover
    // card stays up and its own rows (DV penalty, state, ...) can in turn be hovered for their
    // tooltips. HoverCard carries the dismiss control.
    let show_on_hover = move |_: leptos::ev::PointerEvent| hovered.set(Some(id));
    let show_on_focus = move |_: leptos::ev::FocusEvent| hovered.set(Some(id));

    view! {
        <g
            style=counter_rotation_style
            class="wheel-token"
            tabindex="0"
            on:pointerenter=show_on_hover
            on:focusin=show_on_focus
        >
            <circle cx=x cy=y r=TOKEN_R style:fill=color />
            <text x=x y=y>
                {initial}
            </text>
        </g>
    }
}

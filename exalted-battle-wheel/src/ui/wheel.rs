use crate::ui::Hovered;
use exalted_battle_wheel::battle::{Battle, Combatant, Tick};
use leptos::prelude::*;

const SLOT_COUNT: i64 = 12;
const VIEW_SIZE: f64 = 420.0;
const CENTER: f64 = VIEW_SIZE / 2.0;
const RING_RADIUS: f64 = 160.0;
const LABEL_RADIUS: f64 = 188.0;
const TOKEN_RADIUS: f64 = 160.0;
const TOKEN_FAN_DEGREES: f64 = 9.0;
const TOKEN_R: f64 = 15.0;

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
                <circle cx=CENTER cy=CENTER r=RING_RADIUS class="wheel-ring" />
                <g style=group_rotation class="wheel-ring-group">
                    <For each=slots key=|slot| *slot let:slot>
                        <WheelSlot slot_index=slot battle=battle />
                    </For>
                </g>
                <path d=now_marker class="now-marker" />
                <text x=CENTER y=CENTER class="center-tick">
                    {move || battle.read().current_tick}
                </text>
            </svg>
            <div class="over-horizon">
                <h3>"Beyond the horizon"</h3>
                <For each=over_horizon key=|c| c.id let:combatant>
                    <div class="over-horizon-entry">
                        <span class="name">{combatant.name.clone()}</span>
                        <span class="tick">"tick " {combatant.next_action_tick}</span>
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

    view! {
        <g class="wheel-slot">
            <text x=label_x y=label_y style=counter_rotation_style class="slot-label">
                {label}
            </text>
            <For each=tokens key=|(_, _, c)| c.id let:entry>
                <WheelToken slot_index=slot_index index=entry.0 total=entry.1 combatant=entry.2 />
            </For>
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

    view! {
        <g
            style=counter_rotation_style
            class="wheel-token"
            on:pointerenter=move |_| hovered.set(Some(id))
            on:pointerleave=move |_| hovered.set(None)
        >
            <circle cx=x cy=y r=TOKEN_R style:fill=color />
            <text x=x y=y>
                {initial}
            </text>
        </g>
    }
}

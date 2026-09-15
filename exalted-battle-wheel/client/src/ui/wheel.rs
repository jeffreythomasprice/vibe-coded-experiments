//! The battle wheel: seven wedges counting down to "now" at the top, with six concentric rings
//! marking DV penalty from the rim (-0) inward to the hub (-5 or worse). A token's angle is when
//! it next acts; its distance from the rim is how badly its last action degraded its DV — so one
//! position on the wheel carries both facts at once, styled after Voidstate's printed
//! *Exalted 2nd Edition Battlewheel* poster. Unlike the poster (and unlike this wheel's own
//! predecessor, which rotated the whole ring group to keep "now" at the top) the frame here never
//! moves: sector 0 always means "now," and tokens sweep toward it and slide outward as ticks
//! advance and DV refreshes, so every transform is a plain, non-rotating `translate`.

use crate::ui::glossary::Topic;
use crate::ui::tip::{
    on_focus_in, on_focus_in_text, on_focus_out, on_focus_out_text, on_pointer_enter, on_pointer_enter_text,
    on_pointer_leave, on_pointer_leave_text,
};
use crate::ui::{ticks, Hovered, Tip};
use shared::battle::{Battle, Combatant, CombatantId, Marker, MarkerId, Tick};
use leptos::prelude::*;
use std::collections::BTreeMap;

const SECTOR_COUNT: i64 = 7;
const RING_COUNT: usize = 6;
const VIEW_SIZE: f64 = 600.0;
const CENTER: f64 = VIEW_SIZE / 2.0;
const RIM_RADIUS: f64 = 220.0;
const RING_INNER: f64 = 70.0;
const HUB_RADIUS: f64 = RING_INNER;
const TOKEN_R: f64 = 10.0;
const TOKEN_FAN_DEGREES: f64 = 6.0;
const MAX_VISIBLE_TOKENS: usize = 3;
const DIVIDER_HALF_ANGLE: f64 = 4.6;
const RIM_ARROW_LEN: f64 = 9.0;
const RIM_ARROW_HALF_WIDTH: f64 = 7.0;
const MARKER_GUTTER: [f64; 2] = [234.0, 250.0];
const SECTOR_LABEL_RADIUS: f64 = 260.0;
const SECTOR_ABS_LABEL_RADIUS: f64 = 277.0;
const NOW_POINTER_BASE_HALF_WIDTH: f64 = 16.0;
const NOW_POINTER_GAP: f64 = 26.0;

fn sector_step() -> f64 {
    360.0 / SECTOR_COUNT as f64
}

/// `Some(0..=6)` when `tick` is within the horizon of `now`. Sector 0 is "now"; sector `n` is `n`
/// ticks from now. A `tick` already in the past clamps to sector 0 rather than counting as
/// "beyond the horizon": that only happens for an Inactive combatant, whom `AdvanceTick` exempts
/// from the pending check while her `next_action_tick` stays put, so without this she'd silently
/// fall off the wheel and reappear on the wrong side of it — the horizon list — after six ticks.
fn sector_of(tick: Tick, now: Tick) -> Option<i64> {
    let delta = tick as i64 - now as i64;
    match delta {
        d if d < 0 => Some(0),
        d if d < SECTOR_COUNT => Some(d),
        _ => None,
    }
}

/// The DV-penalty ring index (0 = rim/no penalty, `RING_COUNT - 1` = innermost/-5-or-worse). The
/// core rules never cap accumulated penalty, so anything at or past -5 floors into the last ring;
/// the hover card and queue panel still show the exact number (RULES.md has no stated floor here
/// — see `Topic::DvPenaltyFloor`).
fn ring_of(penalty: i32) -> usize {
    penalty.clamp(-(RING_COUNT as i32 - 1), 0).unsigned_abs() as usize
}

fn ring_boundary(index: usize) -> f64 {
    RIM_RADIUS - index as f64 * (RIM_RADIUS - RING_INNER) / RING_COUNT as f64
}

fn ring_mid_radius(ring: usize) -> f64 {
    (ring_boundary(ring) + ring_boundary(ring + 1)) / 2.0
}

fn ring_label(ring: usize) -> String {
    if ring + 1 == RING_COUNT { "-5+".to_string() } else { format!("-{ring}") }
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

/// A divider label reads along its spoke (rotated to `angle`), flipped 180° in the bottom half so
/// it never renders upside down.
fn spoke_label_rotation(angle: f64) -> f64 {
    let normalized = angle.rem_euclid(360.0);
    if (90.0..270.0).contains(&normalized) { angle - 180.0 } else { angle }
}

/// A wheel position: `sector` ticks from now, `ring` rings in from the rim by DV penalty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Cell {
    sector: i64,
    ring: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TokenSlot {
    id: CombatantId,
    x: f64,
    y: f64,
    initial: char,
    color: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
struct OverflowSlot {
    cell: Cell,
    x: f64,
    y: f64,
    count: usize,
    tip: String,
}

#[derive(Debug, Clone, PartialEq)]
enum MarkerShape {
    Dot { x: f64, y: f64 },
    Arc { d: String },
}

#[derive(Debug, Clone, PartialEq)]
struct MarkerSlot {
    id: MarkerId,
    shape: MarkerShape,
    tip: String,
}

/// Everything the wheel draws, computed once per `Battle` change so every token/chip/arc can read
/// its own row back out reactively instead of freezing whatever it was built with.
#[derive(Debug, Clone, PartialEq, Default)]
struct WheelLayout {
    tokens: Vec<TokenSlot>,
    overflow: Vec<OverflowSlot>,
    markers: Vec<MarkerSlot>,
    beyond: Vec<CombatantId>,
}

impl WheelLayout {
    fn token(&self, id: CombatantId) -> Option<TokenSlot> {
        self.tokens.iter().find(|slot| slot.id == id).copied()
    }

    fn overflow(&self, cell: Cell) -> Option<OverflowSlot> {
        self.overflow.iter().find(|slot| slot.cell == cell).cloned()
    }

    fn marker(&self, id: MarkerId) -> Option<MarkerSlot> {
        self.markers.iter().find(|slot| slot.id == id).cloned()
    }
}

fn wheel_layout(battle: &Battle) -> WheelLayout {
    let now = battle.current_tick;

    let mut cells: BTreeMap<Cell, Vec<&Combatant>> = BTreeMap::new();
    let mut beyond: Vec<&Combatant> = Vec::new();
    for combatant in &battle.combatants {
        match sector_of(combatant.next_action_tick, now) {
            Some(sector) => cells.entry(Cell { sector, ring: ring_of(combatant.dv.penalty) }).or_default().push(combatant),
            None => beyond.push(combatant),
        }
    }
    beyond.sort_by_key(|c| c.next_action_tick);
    let beyond = beyond.into_iter().map(|c| c.id).collect();

    let mut tokens = Vec::new();
    let mut overflow = Vec::new();
    for (cell, mut occupants) in cells {
        occupants.sort_by_key(|c| c.id.0);
        let total = occupants.len();
        let show_chip = total > MAX_VISIBLE_TOKENS;
        let visible_count = if show_chip { MAX_VISIBLE_TOKENS } else { total };
        let slot_count = visible_count + usize::from(show_chip);
        let angle_base = cell.sector as f64 * sector_step();
        let radius = ring_mid_radius(cell.ring);
        let slot_position = |index: usize| {
            let fan_offset = (index as f64 - (slot_count.saturating_sub(1)) as f64 / 2.0) * TOKEN_FAN_DEGREES;
            point_on_circle(radius, angle_base + fan_offset)
        };

        for (index, combatant) in occupants.iter().take(visible_count).enumerate() {
            let (x, y) = slot_position(index);
            tokens.push(TokenSlot {
                id: combatant.id,
                x,
                y,
                initial: combatant.name.chars().next().unwrap_or('?'),
                color: side_color_var(&combatant.side.0),
            });
        }
        if show_chip {
            let count = total - visible_count;
            let hidden_names: Vec<String> = occupants.iter().skip(visible_count).map(|c| c.name.clone()).collect();
            let (x, y) = slot_position(visible_count);
            overflow.push(OverflowSlot { cell, x, y, count, tip: format!("+{count} more here: {}", hidden_names.join(", ")) });
        }
    }
    // Ordered by id, not by cell: a keyed `<For>` that reorders its items re-inserts the DOM node
    // to match, which would cancel the token's in-flight `transform` transition every time some
    // other token's position changed its sort order. Screen position lives entirely in the style.
    tokens.sort_by_key(|slot| slot.id.0);

    let horizon_end = now + (SECTOR_COUNT as u32 - 1);
    let mut visible_markers: Vec<&Marker> =
        battle.markers.iter().filter(|marker| marker.last_tick() >= now && marker.at_tick <= horizon_end).collect();
    visible_markers.sort_by_key(|marker| marker.id.0);
    let markers = visible_markers
        .into_iter()
        .enumerate()
        .map(|(index, marker)| {
            let clipped_start = marker.at_tick.max(now);
            let clipped_end = marker.last_tick().min(horizon_end);
            let start_sector = (clipped_start - now) as i64;
            let end_sector = (clipped_end - now) as i64;
            let radius = MARKER_GUTTER[index % MARKER_GUTTER.len()];
            let shape = if start_sector == end_sector {
                let (x, y) = point_on_circle(radius, start_sector as f64 * sector_step());
                MarkerShape::Dot { x, y }
            } else {
                let start_angle = start_sector as f64 * sector_step();
                let end_angle = end_sector as f64 * sector_step();
                let (sx, sy) = point_on_circle(radius, start_angle);
                let (ex, ey) = point_on_circle(radius, end_angle);
                let large_arc = if (end_angle - start_angle).abs() > 180.0 { 1 } else { 0 };
                MarkerShape::Arc { d: format!("M {sx} {sy} A {radius} {radius} 0 {large_arc} 1 {ex} {ey}") }
            };
            let span = if marker.ticks <= 1 {
                format!("tick {}", marker.at_tick)
            } else {
                format!("ticks {}\u{2013}{}", marker.at_tick, marker.last_tick())
            };
            MarkerSlot { id: marker.id, shape, tip: format!("{} ({span})", marker.label) }
        })
        .collect();

    WheelLayout { tokens, overflow, markers, beyond }
}

#[component]
pub fn Wheel() -> impl IntoView {
    let battle = expect_context::<Memo<Battle>>();
    let layout = Memo::new(move |_| wheel_layout(&battle.read()));

    let sectors = move || (0..SECTOR_COUNT).collect::<Vec<_>>();
    let interior_rings = move || (1..RING_COUNT).collect::<Vec<_>>();

    let token_ids = move || layout.read().tokens.iter().map(|slot| slot.id).collect::<Vec<_>>();
    let overflow_cells = move || layout.read().overflow.iter().map(|slot| slot.cell).collect::<Vec<_>>();
    let marker_ids = move || layout.read().markers.iter().map(|slot| slot.id).collect::<Vec<_>>();
    let beyond_ids = move || layout.read().beyond.clone();

    let now_pointer = {
        let (tip_x, tip_y) = point_on_circle(RIM_RADIUS, 0.0);
        let base_y = CENTER - (RIM_RADIUS + NOW_POINTER_GAP);
        format!(
            "M {} {base_y} L {} {base_y} L {tip_x} {tip_y} Z",
            tip_x - NOW_POINTER_BASE_HALF_WIDTH,
            tip_x + NOW_POINTER_BASE_HALF_WIDTH
        )
    };

    view! {
        <div class="wheel-panel">
            <div class="wheel-spine" aria-hidden="true">
                <span>"Exalted 2nd Edition Battlewheel"</span>
            </div>
            <div class="wheel-column-inner">
            <svg viewBox=format!("0 0 {VIEW_SIZE} {VIEW_SIZE}") class="wheel">
                <circle cx=CENTER cy=CENTER r=RIM_RADIUS class="wheel-paper" />
                <circle
                    cx=CENTER
                    cy=CENTER
                    r=RIM_RADIUS
                    class="wheel-rim"
                    tabindex="0"
                    on:pointerenter=on_pointer_enter(Topic::TickWheel)
                    on:pointerleave=on_pointer_leave(Topic::TickWheel)
                    on:focusin=on_focus_in(Topic::TickWheel)
                    on:focusout=on_focus_out(Topic::TickWheel)
                />
                <For each=interior_rings key=|i| *i let:i>
                    <circle cx=CENTER cy=CENTER r=ring_boundary(i) class="wheel-rule" />
                </For>
                <circle cx=CENTER cy=CENTER r=RING_INNER class="wheel-rule" />

                <For each=sectors key=|sector| *sector let:sector>
                    <WheelDivider sector=sector battle=battle />
                </For>

                <circle
                    cx=CENTER
                    cy=CENTER
                    r=MARKER_GUTTER[0]
                    class="wheel-marker-gutter-ring"
                    tabindex="0"
                    on:pointerenter=on_pointer_enter(Topic::MarkerGutter)
                    on:pointerleave=on_pointer_leave(Topic::MarkerGutter)
                    on:focusin=on_focus_in(Topic::MarkerGutter)
                    on:focusout=on_focus_out(Topic::MarkerGutter)
                />
                <For each=marker_ids key=|id| *id let:id>
                    <WheelMarkerArc id=id layout=layout />
                </For>

                <For each=token_ids key=|id| *id let:id>
                    <WheelToken id=id layout=layout />
                </For>
                <For each=overflow_cells key=|cell| *cell let:cell>
                    <WheelOverflowChip cell=cell layout=layout />
                </For>

                <path
                    d=now_pointer
                    class="now-marker"
                    tabindex="0"
                    on:pointerenter=on_pointer_enter(Topic::NowMarker)
                    on:pointerleave=on_pointer_leave(Topic::NowMarker)
                    on:focusin=on_focus_in(Topic::NowMarker)
                    on:focusout=on_focus_out(Topic::NowMarker)
                />

                <circle cx=CENTER cy=CENTER r=HUB_RADIUS class="wheel-hub" />
                <text
                    x=CENTER
                    y=CENTER - 6.0
                    class="center-tick"
                    tabindex="0"
                    on:pointerenter=on_pointer_enter(Topic::CurrentTick)
                    on:pointerleave=on_pointer_leave(Topic::CurrentTick)
                    on:focusin=on_focus_in(Topic::CurrentTick)
                    on:focusout=on_focus_out(Topic::CurrentTick)
                >
                    {move || battle.read().current_tick}
                </text>
                <text
                    x=CENTER
                    y=CENTER + 16.0
                    class="center-tick-noun"
                    tabindex="0"
                    on:pointerenter=on_pointer_enter(Topic::LongTick)
                    on:pointerleave=on_pointer_leave(Topic::LongTick)
                    on:focusin=on_focus_in(Topic::LongTick)
                    on:focusout=on_focus_out(Topic::LongTick)
                >
                    {move || battle.read().mode.tick_noun().to_string()}
                </text>
            </svg>
            <div class="over-horizon">
                <Tip topic=Topic::BeyondHorizon>
                    <h3>"Beyond the horizon"</h3>
                </Tip>
                <For each=beyond_ids key=|id| *id let:id>
                    <OverHorizonEntry id=id battle=battle />
                </For>
            </div>
            </div>
        </div>
    }
}

#[component]
fn OverHorizonEntry(id: CombatantId, battle: Memo<Battle>) -> impl IntoView {
    let name = move || battle.read().find(id).map(|c| c.name.clone()).unwrap_or_default();
    let tick = move || {
        let battle = battle.read();
        battle.find(id).map(|c| ticks::at(battle.mode, c.next_action_tick)).unwrap_or_default()
    };

    view! {
        <div class="over-horizon-entry">
            <Tip topic=Topic::CombatantName>
                <span class="name">{name}</span>
            </Tip>
            <Tip topic=Topic::NextActionTick>
                <span class="tick">{tick}</span>
            </Tip>
        </div>
    }
}

#[component]
fn WheelDivider(sector: i64, battle: Memo<Battle>) -> impl IntoView {
    let angle = sector as f64 * sector_step() + sector_step() / 2.0;
    let (outer_left_x, outer_left_y) = point_on_circle(RIM_RADIUS, angle - DIVIDER_HALF_ANGLE);
    let (outer_right_x, outer_right_y) = point_on_circle(RIM_RADIUS, angle + DIVIDER_HALF_ANGLE);
    let (inner_right_x, inner_right_y) = point_on_circle(RING_INNER, angle + DIVIDER_HALF_ANGLE);
    let (inner_left_x, inner_left_y) = point_on_circle(RING_INNER, angle - DIVIDER_HALF_ANGLE);
    let wedge = format!(
        "M {outer_left_x} {outer_left_y} L {outer_right_x} {outer_right_y} L {inner_right_x} {inner_right_y} L {inner_left_x} {inner_left_y} Z"
    );

    let (arrow_tip_x, arrow_tip_y) = point_on_circle(RIM_RADIUS + RIM_ARROW_LEN, angle);
    let (arrow_left_x, arrow_left_y) = point_on_circle(RIM_RADIUS, angle - RIM_ARROW_HALF_WIDTH);
    let (arrow_right_x, arrow_right_y) = point_on_circle(RIM_RADIUS, angle + RIM_ARROW_HALF_WIDTH);
    let arrow = format!("M {arrow_left_x} {arrow_left_y} L {arrow_right_x} {arrow_right_y} L {arrow_tip_x} {arrow_tip_y} Z");

    let label_rotation = spoke_label_rotation(angle);
    let rings: Vec<usize> = (0..RING_COUNT).collect();

    let label = move || sector as u32 + battle.read().current_tick;
    let (rel_x, rel_y) = point_on_circle(SECTOR_LABEL_RADIUS, sector as f64 * sector_step());
    let (abs_x, abs_y) = point_on_circle(SECTOR_ABS_LABEL_RADIUS, sector as f64 * sector_step());
    let is_now = sector == 0;

    view! {
        <g class="wheel-divider-group">
            <path
                d=wedge
                class="wheel-divider"
                tabindex="0"
                on:pointerenter=on_pointer_enter(Topic::DvPenaltyRing)
                on:pointerleave=on_pointer_leave(Topic::DvPenaltyRing)
                on:focusin=on_focus_in(Topic::DvPenaltyRing)
                on:focusout=on_focus_out(Topic::DvPenaltyRing)
            />
            <path d=arrow class="wheel-rim-arrow" />
            <For each=move || rings.clone() key=|ring| *ring let:ring>
                {
                    let (x, y) = point_on_circle(ring_mid_radius(ring), angle);
                    let topic = if ring + 1 == RING_COUNT { Topic::DvPenaltyFloor } else { Topic::DvPenaltyRing };
                    view! {
                        <text
                            x=x
                            y=y
                            class="ring-label"
                            style=format!("transform: rotate({label_rotation}deg); transform-origin: {x}px {y}px; transform-box: view-box;")
                            tabindex="0"
                            on:pointerenter=on_pointer_enter(topic)
                            on:pointerleave=on_pointer_leave(topic)
                            on:focusin=on_focus_in(topic)
                            on:focusout=on_focus_out(topic)
                        >
                            {ring_label(ring)}
                        </text>
                    }
                }
            </For>
            <text
                x=rel_x
                y=rel_y
                class="sector-label"
                class:sector-label-now=is_now
                tabindex="0"
                on:pointerenter=on_pointer_enter(Topic::TickSlot)
                on:pointerleave=on_pointer_leave(Topic::TickSlot)
                on:focusin=on_focus_in(Topic::TickSlot)
                on:focusout=on_focus_out(Topic::TickSlot)
            >
                {sector}
            </text>
            <text
                x=abs_x
                y=abs_y
                class="sector-abs-label"
                tabindex="0"
                on:pointerenter=on_pointer_enter(Topic::SectorCountdown)
                on:pointerleave=on_pointer_leave(Topic::SectorCountdown)
                on:focusin=on_focus_in(Topic::SectorCountdown)
                on:focusout=on_focus_out(Topic::SectorCountdown)
            >
                {label}
            </text>
        </g>
    }
}

#[component]
fn WheelToken(id: CombatantId, layout: Memo<WheelLayout>) -> impl IntoView {
    let hovered = expect_context::<Hovered>();
    let slot = move || layout.read().token(id);
    let transform = move || match slot() {
        Some(slot) => format!("transform: translate({}px, {}px);", slot.x, slot.y),
        None => String::new(),
    };
    let color = move || slot().map(|slot| slot.color).unwrap_or("var(--side-a)");
    let initial = move || slot().map(|slot| slot.initial).unwrap_or('?').to_string();

    // Deliberately sticky: leaving or blurring the token does not clear `hovered`, so the hover
    // card stays up and its own rows (DV penalty, state, ...) can in turn be hovered for their
    // tooltips. HoverCard carries the dismiss control.
    let show_on_hover = move |_: leptos::ev::PointerEvent| hovered.set(Some(id));
    let show_on_focus = move |_: leptos::ev::FocusEvent| hovered.set(Some(id));

    view! {
        <g
            class="wheel-token"
            style=transform
            tabindex="0"
            on:pointerenter=show_on_hover
            on:focusin=show_on_focus
        >
            <circle r=TOKEN_R style:fill=color />
            <text>{initial}</text>
        </g>
    }
}

#[component]
fn WheelOverflowChip(cell: Cell, layout: Memo<WheelLayout>) -> impl IntoView {
    let slot = move || layout.read().overflow(cell);
    let transform = move || match slot() {
        Some(slot) => format!("transform: translate({}px, {}px);", slot.x, slot.y),
        None => String::new(),
    };
    let count_label = move || slot().map(|slot| format!("+{}", slot.count)).unwrap_or_default();
    let tip_text = Signal::derive(move || slot().map(|slot| slot.tip).unwrap_or_default());

    view! {
        <g
            class="wheel-token wheel-token-overflow"
            style=transform
            tabindex="0"
            on:pointerenter=on_pointer_enter_text(tip_text)
            on:pointerleave=on_pointer_leave_text(tip_text)
            on:focusin=on_focus_in_text(tip_text)
            on:focusout=on_focus_out_text(tip_text)
        >
            <circle r=TOKEN_R />
            <text>{count_label}</text>
        </g>
    }
}

#[component]
fn WheelMarkerArc(id: MarkerId, layout: Memo<WheelLayout>) -> impl IntoView {
    let slot = move || layout.read().marker(id);
    let tip_text = Signal::derive(move || slot().map(|slot| slot.tip).unwrap_or_default());
    let shape = move || {
        slot().map(|slot| match slot.shape {
            MarkerShape::Dot { x, y } => view! { <circle cx=x cy=y r=5.0 class="wheel-marker-dot" /> }.into_any(),
            MarkerShape::Arc { d } => view! { <path d=d class="wheel-marker-arc" fill="none" /> }.into_any(),
        })
    };

    view! {
        <g
            class="wheel-marker"
            tabindex="0"
            on:pointerenter=on_pointer_enter_text(tip_text)
            on:pointerleave=on_pointer_leave_text(tip_text)
            on:focusin=on_focus_in_text(tip_text)
            on:focusout=on_focus_out_text(tip_text)
        >
            {shape}
        </g>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::battle::{CombatantState, DvState, JoinBattleResult, Side};

    fn combatant(id: u32, name: &str, tick: Tick, penalty: i32) -> Combatant {
        Combatant {
            id: CombatantId(id),
            name: name.to_string(),
            side: Side("A".to_string()),
            join_battle: JoinBattleResult::Successes(0),
            next_action_tick: tick,
            state: CombatantState::Normal,
            dv: DvState { penalty, refreshes_at: None },
            commitment: None,
        }
    }

    fn battle_at(tick: Tick, combatants: Vec<Combatant>) -> Battle {
        Battle { current_tick: tick, combatants, ..Battle::genesis() }
    }

    fn marker(id: u32, source: u32, at_tick: Tick, ticks: u32) -> Marker {
        Marker { id: MarkerId(id), label: format!("Marker {id}"), source: CombatantId(source), at_tick, ticks }
    }

    #[test]
    fn tokens_are_ordered_by_id_not_by_position() {
        let battle = battle_at(0, vec![combatant(5, "E", 3, 0), combatant(1, "A", 0, 0)]);
        let layout = wheel_layout(&battle);
        assert_eq!(layout.tokens.iter().map(|t| t.id.0).collect::<Vec<_>>(), vec![1, 5]);
    }

    #[test]
    fn the_reported_bug() {
        // A starts at tick 0 (cell key "sector 0"); adding B and rescheduling everyone moves A
        // onto tick 2 while B lands on A's old cell. Both must render, at distinct positions.
        let battle = battle_at(0, vec![combatant(1, "A", 2, 0), combatant(2, "B", 0, 0)]);
        let layout = wheel_layout(&battle);
        assert_eq!(layout.tokens.len(), 2);
        let a = layout.token(CombatantId(1)).unwrap();
        let b = layout.token(CombatantId(2)).unwrap();
        assert_eq!(a.initial, 'A');
        assert_eq!(b.initial, 'B');
        assert_ne!((a.x, a.y), (b.x, b.y));
    }

    #[test]
    fn a_combatant_due_now_sits_on_the_sector_zero_spoke() {
        let battle = battle_at(10, vec![combatant(1, "A", 10, 0)]);
        let layout = wheel_layout(&battle);
        let slot = layout.token(CombatantId(1)).unwrap();
        assert_eq!((slot.x, slot.y), point_on_circle(ring_mid_radius(0), 0.0));
    }

    #[test]
    fn dv_penalty_chooses_the_ring() {
        let battle = battle_at(0, vec![combatant(1, "A", 0, -2), combatant(2, "B", 0, -9)]);
        let layout = wheel_layout(&battle);
        let a = layout.token(CombatantId(1)).unwrap();
        let b = layout.token(CombatantId(2)).unwrap();
        assert_eq!(a.y, point_on_circle(ring_mid_radius(2), 0.0).1);
        assert_eq!(b.y, point_on_circle(ring_mid_radius(RING_COUNT - 1), 0.0).1);
    }

    #[test]
    fn cell_occupants_fan_symmetrically() {
        let battle = battle_at(0, vec![combatant(1, "A", 0, 0), combatant(2, "B", 0, 0)]);
        let layout = wheel_layout(&battle);
        let a = layout.token(CombatantId(1)).unwrap();
        let b = layout.token(CombatantId(2)).unwrap();
        assert!((a.x - CENTER) < 0.0);
        assert!((b.x - CENTER) > 0.0);
        assert!((a.y - b.y).abs() < 1e-9);
    }

    #[test]
    fn a_fourth_occupant_becomes_an_overflow_chip() {
        let battle =
            battle_at(0, vec![combatant(1, "A", 0, 0), combatant(2, "B", 0, 0), combatant(3, "C", 0, 0), combatant(4, "D", 0, 0)]);
        let layout = wheel_layout(&battle);
        assert_eq!(layout.tokens.len(), 3);
        let chip = layout.overflow(Cell { sector: 0, ring: 0 }).unwrap();
        assert_eq!(chip.count, 1);
        assert_eq!(chip.tip, "+1 more here: D");
    }

    #[test]
    fn overflow_keeps_the_lowest_ids_visible() {
        let battle =
            battle_at(0, vec![combatant(4, "D", 0, 0), combatant(1, "A", 0, 0), combatant(3, "C", 0, 0), combatant(2, "B", 0, 0)]);
        let layout = wheel_layout(&battle);
        let visible: Vec<u32> = layout.tokens.iter().map(|t| t.id.0).collect();
        assert_eq!(visible, vec![1, 2, 3]);
    }

    #[test]
    fn an_overdue_combatant_stays_on_the_now_spoke() {
        // An Inactive combatant is exempt from AdvanceTick's pending check, so her
        // next_action_tick can fall behind `now` without ever being rescheduled.
        let battle = battle_at(10, vec![combatant(1, "A", 4, 0)]);
        let layout = wheel_layout(&battle);
        assert!(layout.beyond.is_empty());
        let slot = layout.token(CombatantId(1)).unwrap();
        assert_eq!((slot.x, slot.y), point_on_circle(ring_mid_radius(0), 0.0));
    }

    #[test]
    fn combatants_past_the_horizon_are_listed_beyond_it_in_tick_order() {
        let battle = battle_at(0, vec![combatant(1, "A", 20, 0), combatant(2, "B", 8, 0), combatant(3, "C", 3, 0)]);
        let layout = wheel_layout(&battle);
        assert_eq!(layout.beyond, vec![CombatantId(2), CombatantId(1)]);
    }

    #[test]
    fn markers_alternate_gutter_rings_in_id_order() {
        let battle = Battle {
            current_tick: 0,
            markers: vec![marker(1, 1, 0, 1), marker(2, 1, 1, 1), marker(3, 1, 2, 1)],
            ..Battle::genesis()
        };
        let layout = wheel_layout(&battle);
        let dot_xy = |id: u32, gutter_index: usize, sector: i64| {
            let expected = point_on_circle(MARKER_GUTTER[gutter_index], sector as f64 * sector_step());
            assert!(matches!(layout.marker(MarkerId(id)).unwrap().shape, MarkerShape::Dot { x, y } if (x, y) == expected));
        };
        dot_xy(1, 0, 0);
        dot_xy(2, 1, 1);
        dot_xy(3, 0, 2);
    }

    #[test]
    fn a_marker_is_clipped_to_the_horizon() {
        // Spans tick 2 through 21; at now=5 with a 7-sector horizon (ticks 5-11), the arc must be
        // clipped to start at "now" and end at the last visible sector rather than run off both ends.
        let battle = Battle { current_tick: 5, markers: vec![marker(1, 1, 2, 20)], ..Battle::genesis() };
        let layout = wheel_layout(&battle);
        let slot = layout.marker(MarkerId(1)).unwrap();
        let (sx, sy) = point_on_circle(MARKER_GUTTER[0], 0.0);
        let (ex, ey) = point_on_circle(MARKER_GUTTER[0], (SECTOR_COUNT - 1) as f64 * sector_step());
        let expected = format!("M {sx} {sy} A {} {} 0 1 1 {ex} {ey}", MARKER_GUTTER[0], MARKER_GUTTER[0]);
        assert_eq!(slot.shape, MarkerShape::Arc { d: expected });
    }

    #[test]
    fn a_one_tick_marker_is_a_dot() {
        let battle = Battle { current_tick: 0, markers: vec![marker(1, 1, 0, 1)], ..Battle::genesis() };
        let layout = wheel_layout(&battle);
        assert!(matches!(layout.marker(MarkerId(1)).unwrap().shape, MarkerShape::Dot { .. }));
    }

    #[test]
    fn an_expired_marker_is_not_drawn() {
        let battle = Battle { current_tick: 10, markers: vec![marker(1, 1, 0, 5)], ..Battle::genesis() };
        let layout = wheel_layout(&battle);
        assert!(layout.marker(MarkerId(1)).is_none());
    }

    #[test]
    fn marker_tip_reads_its_span() {
        let battle =
            Battle { current_tick: 3, markers: vec![marker(1, 1, 8, 3), marker(2, 1, 5, 1)], ..Battle::genesis() };
        let layout = wheel_layout(&battle);
        assert_eq!(layout.marker(MarkerId(1)).unwrap().tip, "Marker 1 (ticks 8\u{2013}10)");
        assert_eq!(layout.marker(MarkerId(2)).unwrap().tip, "Marker 2 (tick 5)");
    }
}

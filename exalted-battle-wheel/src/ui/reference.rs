//! The reference rail: a generated Speed/DV lookup for all three battle modes, styled after
//! Voidstate's printed *Exalted 2nd Edition Battlewheel* poster's own Combat Actions / Mass
//! Combat / Social Combat lists. Unlike the poster — whose numbers disagree with the book in
//! several places (Monologue/Study, social Inactive, Join Debate, Simple Charm) — every row here
//! is read straight from this app's own catalog (`PERSONAL_CATALOG`/`MASS_ONLY_CATALOG`/
//! `SOCIAL_CATALOG`), so it can't drift from what the app actually plays.
//!
//! Rows in the section matching the battle's current mode double as a shortcut: clicking one
//! selects that action in the action panel's dropdown for the targeted "Up now" row
//! (`action_panel::rail_target` — the last row touched, else the topmost). It selects only;
//! declaring stays an explicit Declare click. The other two sections stay inert because an
//! `ActionKind` alone doesn't identify an action — social Aim is Monologue/Study at 3/-2, not
//! personal Aim's 3/-1 — so only the active section's rows can be resolved against `catalog(mode)`
//! without guessing.

use crate::ui::action_panel::{rail_target, RailSelection};
use crate::ui::format::{format_dv_penalty_compact, format_speed_compact};
use crate::ui::glossary::action_topic;
use crate::ui::tip::{on_focus_in, on_focus_out, on_pointer_enter, on_pointer_leave};
use exalted_battle_wheel::battle::{ActionTemplate, Battle, BattleMode, CombatantId, MASS_ONLY_CATALOG, PERSONAL_CATALOG, SOCIAL_CATALOG};
use leptos::prelude::*;

#[component]
pub fn ReferenceRail() -> impl IntoView {
    let battle = expect_context::<Memo<Battle>>();
    let selection = expect_context::<RailSelection>();
    let mode = move || battle.read().mode;
    let target = Memo::new(move |_| rail_target(&battle.read(), selection.touched.get()));

    view! {
        <div class="reference-rail">
            <ReferenceSection
                title="Combat Actions"
                mode_for_topics=BattleMode::Personal
                rows=PERSONAL_CATALOG
                active=Signal::derive(move || matches!(mode(), BattleMode::Personal | BattleMode::Mass))
                target=target
            />
            <ReferenceSection
                title="Mass Combat"
                mode_for_topics=BattleMode::Mass
                rows=MASS_ONLY_CATALOG
                active=Signal::derive(move || mode() == BattleMode::Mass)
                target=target
            />
            <ReferenceSection
                title="Social Combat"
                mode_for_topics=BattleMode::Social
                rows=SOCIAL_CATALOG
                active=Signal::derive(move || mode() == BattleMode::Social)
                target=target
            />
        </div>
    }
}

#[component]
fn ReferenceSection(
    title: &'static str,
    /// Which mode's glossary topics apply to `rows` — the Mass Combat section, for instance,
    /// lists `MASS_ONLY_CATALOG`'s rows but still needs `BattleMode::Mass` to look up their
    /// topics, since a bare `ActionKind` doesn't carry its mode.
    mode_for_topics: BattleMode,
    rows: &'static [ActionTemplate],
    active: Signal<bool>,
    target: Memo<Option<CombatantId>>,
) -> impl IntoView {
    let clickable = Signal::derive(move || active.get() && target.get().is_some());
    view! {
        <section class="reference-section" class:reference-section-active=move || active.get()>
            <h3>{title}</h3>
            <div class="reference-list">
                {rows.iter().map(|template| view! {
                    <ReferenceRow mode_for_topics=mode_for_topics template=*template clickable=clickable target=target />
                }).collect_view()}
            </div>
        </section>
    }
}

#[component]
fn ReferenceRow(mode_for_topics: BattleMode, template: ActionTemplate, clickable: Signal<bool>, target: Memo<Option<CombatantId>>) -> impl IntoView {
    let selection = expect_context::<RailSelection>();
    let kind = template.kind;
    let stat = format!("{}/{}", format_speed_compact(template.speed), format_dv_penalty_compact(template.dv_penalty));

    let pick = move |_| {
        if !clickable.get_untracked() {
            return;
        }
        if let Some(actor) = target.get_untracked() {
            selection.pick.set(Some((actor, kind)));
        }
    };

    // `aria-disabled`, not `disabled`: a disabled button drops out of the tab order and stops
    // receiving pointer events, which would silence the teaching tooltip on every inert row.
    // Inert rows stay focusable and hoverable on purpose — the rail is a reference first, a
    // shortcut second.
    let button = view! {
        <button
            type="button"
            class="reference-row"
            class:reference-row-clickable=move || clickable.get()
            aria-disabled=move || (!clickable.get()).then_some("true")
            on:click=pick
        >
            <span class="reference-name">{template.name}</span>
            <span class="reference-stat">{stat.clone()}</span>
        </button>
    };

    match action_topic(mode_for_topics, kind) {
        Some(topic) => view! {
            <button
                type="button"
                class="reference-row"
                class:reference-row-clickable=move || clickable.get()
                aria-disabled=move || (!clickable.get()).then_some("true")
                on:click=pick
                on:pointerenter=on_pointer_enter(topic)
                on:pointerleave=on_pointer_leave(topic)
                on:focusin=on_focus_in(topic)
                on:focusout=on_focus_out(topic)
            >
                <span class="reference-name">{template.name}</span>
                <span class="reference-stat">{stat.clone()}</span>
            </button>
        }
            .into_any(),
        None => button.into_any(),
    }
}

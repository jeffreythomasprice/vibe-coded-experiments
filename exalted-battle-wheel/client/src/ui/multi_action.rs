use crate::battle_net::Battles;
use crate::ui::Tip;
use crate::ui::format::{format_dv_penalty_compact, format_speed_compact};
use crate::ui::glossary::Topic;
use leptos::prelude::*;
use shared::battle::{
    ActionSpeed, Battle, BattleEvent, CombatantId, DeclaredAction, DvPenaltySpec, FlurryDvRule, FlurryPart, LastDeclared, can_flurry,
    catalog, catalog_index, declare_flurry,
};

fn next_row_id(counter: RwSignal<u32>) -> u32 {
    let id = counter.get_untracked();
    counter.set(id + 1);
    id
}

fn entered_u32(text: &str) -> Option<u32> {
    text.trim().parse().ok()
}

fn entered_i32(text: &str) -> Option<i32> {
    text.trim().parse().ok()
}

/// A row in the builder, keyed like `library.rs`'s `StepRow` — `catalog_index` is a position in
/// `catalog(mode)`, the same convention `action_panel.rs`'s `ChoiceKey::Action` uses.
#[derive(Clone, Copy)]
struct PartRow {
    row_id: u32,
    catalog_index: RwSignal<usize>,
    speed: RwSignal<String>,
    dv_penalty: RwSignal<String>,
}

fn make_row(row_counter: RwSignal<u32>, catalog_index: usize, speed: String, dv_penalty: String) -> PartRow {
    PartRow {
        row_id: next_row_id(row_counter),
        catalog_index: RwSignal::new(catalog_index),
        speed: RwSignal::new(speed),
        dv_penalty: RwSignal::new(dv_penalty),
    }
}

/// Content for the "Multiple actions…" modal (`action_panel.rs` supplies the `Modal` wrapper and
/// its title): lets a GM record several actions resolved on one tick — the escape hatch for a
/// GM-fiat multi-action, or a mundane flurry with per-action DV tracked instead of typed by hand.
/// Commits as one `DeclareAction` carrying a `FlurryBreakdown`, so it undoes in one step
/// (RULES.md §1.4, p. 141: a tick is a transaction).
#[component]
pub fn MultiActionModal(
    actor_id: CombatantId,
    battles: Battles,
    battle: Memo<Battle>,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let mode = move || battle.read().mode;
    let row_counter = RwSignal::new(0u32);

    let eligible_indices = move || -> Vec<usize> {
        catalog(mode())
            .enumerate()
            .filter(|(_, template)| can_flurry(template))
            .map(|(index, _)| index)
            .collect()
    };

    // Seeded once at mount, same as `NormalControls`' `recalled`: either the actor's last
    // multi-action mapped back onto rows, or two blank rows of the first eligible action.
    let (initial_rows, initial_rule, initial_name) = {
        let snapshot = battle.read_untracked();
        let last = snapshot.find(actor_id).and_then(|c| c.last_declared.as_ref());
        match last {
            Some(LastDeclared::Action(action)) if action.flurry.is_some() => {
                let breakdown = action.flurry.clone().expect("checked above");
                let rows: Vec<PartRow> = breakdown
                    .actions
                    .iter()
                    .filter_map(|part| {
                        let index = catalog_index(snapshot.mode, part.kind)?;
                        Some(make_row(row_counter, index, part.speed.to_string(), part.dv_penalty.to_string()))
                    })
                    .collect();
                let joined_default = breakdown
                    .actions
                    .iter()
                    .map(|part| part.label.to_string())
                    .collect::<Vec<_>>()
                    .join(" + ");
                let label = action.label.to_string();
                let name = if label == joined_default { String::new() } else { label };
                (rows, breakdown.rule, name)
            }
            _ => {
                let first = eligible_indices().first().copied().unwrap_or(0);
                (
                    vec![
                        make_row(row_counter, first, String::new(), String::new()),
                        make_row(row_counter, first, String::new(), String::new()),
                    ],
                    FlurryDvRule::Stacked,
                    String::new(),
                )
            }
        }
    };

    let rows = RwSignal::new(initial_rows);
    let rule = RwSignal::new(initial_rule);
    let name = RwSignal::new(initial_name);
    let speed_override = RwSignal::new(String::new());
    let request_error = RwSignal::new(None::<String>);

    let add_row = move |_| {
        let first = eligible_indices().first().copied().unwrap_or(0);
        rows.update(|rows| rows.push(make_row(row_counter, first, String::new(), String::new())));
    };
    let remove_row = move |row_id: u32| rows.update(|rows| rows.retain(|row| row.row_id != row_id));

    let collect_parts = move || -> Vec<FlurryPart> {
        let mode = mode();
        rows.get()
            .iter()
            .filter_map(|row| {
                let template = catalog(mode).nth(row.catalog_index.get())?;
                Some(FlurryPart {
                    template,
                    speed: entered_u32(&row.speed.get()),
                    dv_penalty: entered_i32(&row.dv_penalty.get()),
                })
            })
            .collect()
    };

    let highest_speed = move || {
        collect_parts()
            .iter()
            .filter_map(|part| part.template.resolve_speed(part.speed).ok())
            .max()
            .unwrap_or(0)
    };

    let preview = move || -> Result<DeclaredAction, String> {
        let parts = collect_parts();
        let typed_name = name.get();
        let given_name = (!typed_name.trim().is_empty()).then_some(typed_name);
        declare_flurry(mode(), &parts, rule.get(), entered_u32(&speed_override.get()), given_name).map_err(|error| error.to_string())
    };

    let summary = move || {
        preview().map(|action| {
            let next_tick = battle.read().current_tick + action.speed;
            format!(
                "Speed {}, DV {} \u{2192} acts next on tick {next_tick}",
                action.speed, action.dv_penalty
            )
        })
    };

    let declare = move |_| {
        let Ok(action) = preview() else { return };
        request_error.set(None);
        battles.push_with(BattleEvent::DeclareAction { actor: actor_id, action }, move |result| match result {
            Ok(()) => on_close(),
            Err(error) => {
                tracing::error!(%error, "could not declare multi-action");
                request_error.set(Some(error.to_string()));
            }
        });
    };

    let radio_group_stacked = format!("flurry-dv-rule-{}", actor_id.0);
    let radio_group_worst_only = radio_group_stacked.clone();

    view! {
        <div class="library-editor">
            <Tip topic=Topic::MultipleActions>
                <label class="library-field">
                    "Name"
                    <input
                        placeholder="auto (component names)"
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </label>
            </Tip>
            <div class="library-steps">
                <For each=move || rows.get() key=|row| row.row_id let:row>
                    <div class="library-row">
                        <select
                            prop:value=move || row.catalog_index.get().to_string()
                            on:change=move |ev| {
                                if let Ok(index) = event_target_value(&ev).parse() {
                                    row.catalog_index.set(index);
                                }
                            }
                        >
                            <optgroup label="Flurryable">
                                {move || catalog(mode()).enumerate().filter(|(_, template)| can_flurry(template) && template.flurryable).map(|(index, template)| view! {
                                    <option value=index.to_string()>{template.name}</option>
                                }).collect_view()}
                            </optgroup>
                            <optgroup label="GM discretion">
                                {move || catalog(mode()).enumerate().filter(|(_, template)| can_flurry(template) && !template.flurryable).map(|(index, template)| view! {
                                    <option value=index.to_string()>{template.name}</option>
                                }).collect_view()}
                            </optgroup>
                        </select>
                        {move || {
                            let template = catalog(mode()).nth(row.catalog_index.get()).expect("row index stays within catalog(mode)");
                            let speed_missing = template.speed == ActionSpeed::Required && entered_u32(&row.speed.get()).is_none();
                            view! {
                                <>
                                    {if matches!(template.speed, ActionSpeed::Fixed(_)) {
                                        view! { <span class="action-summary-chip">{format_speed_compact(template.speed)}</span> }.into_any()
                                    } else {
                                        view! {
                                            <input
                                                placeholder=format_speed_compact(template.speed)
                                                prop:value=move || row.speed.get()
                                                on:input=move |ev| row.speed.set(event_target_value(&ev))
                                                class:speed-missing=move || speed_missing
                                            />
                                        }.into_any()
                                    }}
                                    {if matches!(template.dv_penalty, DvPenaltySpec::Variable { .. }) {
                                        view! {
                                            <input
                                                placeholder=format_dv_penalty_compact(template.dv_penalty)
                                                prop:value=move || row.dv_penalty.get()
                                                on:input=move |ev| row.dv_penalty.set(event_target_value(&ev))
                                            />
                                        }.into_any()
                                    } else {
                                        view! { <span class="action-summary-chip">{format_dv_penalty_compact(template.dv_penalty)}</span> }.into_any()
                                    }}
                                </>
                            }
                        }}
                        <button on:click=move |_| remove_row(row.row_id)>"\u{2715}"</button>
                    </div>
                </For>
                <button on:click=add_row>"Add action"</button>
            </div>
            <Tip topic=Topic::EffectiveSpeed>
                <label class="library-field">
                    "Effective speed"
                    <input
                        placeholder=move || format!("highest: {}", highest_speed())
                        prop:value=move || speed_override.get()
                        on:input=move |ev| speed_override.set(event_target_value(&ev))
                    />
                </label>
            </Tip>
            <div class="multi-action-rule">
                <Tip topic=Topic::FlurryDvStacked>
                    <label>
                        <input
                            type="radio"
                            name=radio_group_stacked
                            prop:checked=move || rule.get() == FlurryDvRule::Stacked
                            on:change=move |_| rule.set(FlurryDvRule::Stacked)
                        />
                        "Stack every penalty"
                    </label>
                </Tip>
                <Tip topic=Topic::FlurryDvWorstOnly>
                    <label>
                        <input
                            type="radio"
                            name=radio_group_worst_only
                            prop:checked=move || rule.get() == FlurryDvRule::WorstOnly
                            on:change=move |_| rule.set(FlurryDvRule::WorstOnly)
                        />
                        "Worst DV only"
                    </label>
                </Tip>
            </div>
            {move || (rule.get() == FlurryDvRule::Stacked && !rows.get().is_empty()).then(|| {
                let n = rows.get().len() as i32;
                let dice: Vec<String> = (0..rows.get().len()).map(|i| (-(n + i as i32)).to_string()).collect();
                view! {
                    <Tip topic=Topic::MultipleActionDice>
                        <div class="action-summary-note">"Dice penalty " {dice.join(" / ")}</div>
                    </Tip>
                }
            })}
            {move || match summary() {
                Ok(text) => view! { <div class="action-status">{text}</div> }.into_any(),
                Err(error) => view! { <div class="action-error">{error}</div> }.into_any(),
            }}
            <button on:click=declare disabled=move || battles.read_only().get() || preview().is_err()>"Declare"</button>
            {move || request_error.get().map(|error| view! { <div class="action-error">{error}</div> }.into_any())}
        </div>
    }
}

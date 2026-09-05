use crate::library::{Library, SavedAction, SavedEffect, SavedId, SavedShape};
use crate::prefs::Pref;
use crate::ui::glossary::Topic;
use crate::ui::Tip;
use exalted_battle_wheel::battle::{ActionKind, SequenceStep, SpeedSpec, CATALOG};
use leptos::prelude::*;

/// A step or effect row keyed by a locally-minted id (not a `SavedId` or `MarkerId` — those are
/// assigned only once the row is actually saved), so `<For>` can track rows through inserts and
/// removals without reusing a stale key. Each field is its own signal so a keystroke updates just
/// that input, not the whole row list.
#[derive(Clone, Copy)]
struct StepRow {
    row_id: u32,
    label: RwSignal<String>,
    speed: RwSignal<String>,
    dv_penalty: RwSignal<String>,
}

#[derive(Clone, Copy)]
struct EffectRow {
    row_id: u32,
    label: RwSignal<String>,
    delay: RwSignal<String>,
    ticks: RwSignal<String>,
}

fn next_row_id(counter: RwSignal<u32>) -> u32 {
    let id = counter.get_untracked();
    counter.set(id + 1);
    id
}

#[component]
pub fn SavedActionEditor(
    library: Pref<Library>,
    initial: SavedAction,
    editing_id: Option<SavedId>,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let row_counter = RwSignal::new(0u32);

    let name = RwSignal::new(initial.name.clone());
    let note = RwSignal::new(initial.note.clone());
    let is_sequence = RwSignal::new(matches!(initial.shape, SavedShape::Sequence { .. }));

    let (initial_kind, initial_speed, initial_dv) = match &initial.shape {
        SavedShape::Single { kind, speed, dv_penalty } => (*kind, speed.to_string(), dv_penalty.to_string()),
        SavedShape::Sequence { .. } => (ActionKind::Custom, String::new(), String::new()),
    };
    let kind = RwSignal::new(initial_kind);
    let speed = RwSignal::new(initial_speed);
    let dv_penalty = RwSignal::new(initial_dv);

    let initial_steps: Vec<StepRow> = match &initial.shape {
        SavedShape::Sequence { steps } => steps
            .iter()
            .map(|step| StepRow {
                row_id: next_row_id(row_counter),
                label: RwSignal::new(step.label.clone()),
                speed: RwSignal::new(match step.speed {
                    SpeedSpec::Fixed(speed) => speed.to_string(),
                    SpeedSpec::Variable { .. } => String::new(),
                }),
                dv_penalty: RwSignal::new(step.dv_penalty.to_string()),
            })
            .collect(),
        SavedShape::Single { .. } => Vec::new(),
    };
    let steps = RwSignal::new(initial_steps);

    let initial_effects: Vec<EffectRow> = initial
        .effects
        .iter()
        .map(|effect| EffectRow {
            row_id: next_row_id(row_counter),
            label: RwSignal::new(effect.label.clone()),
            delay: RwSignal::new(effect.delay.to_string()),
            ticks: RwSignal::new(effect.ticks.to_string()),
        })
        .collect();
    let effects = RwSignal::new(initial_effects);

    let error = RwSignal::new(String::new());

    let add_step = move |_| {
        let row_id = next_row_id(row_counter);
        steps.update(|rows| rows.push(StepRow { row_id, label: RwSignal::new(String::new()), speed: RwSignal::new(String::new()), dv_penalty: RwSignal::new("0".to_string()) }));
    };
    let remove_step = move |row_id: u32| steps.update(|rows| rows.retain(|row| row.row_id != row_id));

    let add_effect = move |_| {
        let row_id = next_row_id(row_counter);
        effects.update(|rows| rows.push(EffectRow { row_id, label: RwSignal::new(String::new()), delay: RwSignal::new("0".to_string()), ticks: RwSignal::new("1".to_string()) }));
    };
    let remove_effect = move |row_id: u32| effects.update(|rows| rows.retain(|row| row.row_id != row_id));

    let save = move |_| {
        let shape = if is_sequence.get() {
            let built: Vec<SequenceStep> = steps
                .get()
                .iter()
                .map(|row| SequenceStep {
                    label: row.label.get(),
                    speed: match row.speed.get().trim().parse::<u32>() {
                        Ok(fixed) => SpeedSpec::Fixed(fixed),
                        Err(_) => SpeedSpec::Variable { default: 5 },
                    },
                    dv_penalty: row.dv_penalty.get().trim().parse().unwrap_or(0),
                })
                .collect();
            SavedShape::Sequence { steps: built }
        } else {
            SavedShape::Single {
                kind: kind.get(),
                speed: speed.get().trim().parse().unwrap_or(0),
                dv_penalty: dv_penalty.get().trim().parse().unwrap_or(0),
            }
        };
        let built_effects: Vec<SavedEffect> = effects
            .get()
            .iter()
            .map(|row| SavedEffect { label: row.label.get(), delay: row.delay.get().trim().parse().unwrap_or(0), ticks: row.ticks.get().trim().parse().unwrap_or(1) })
            .collect();

        let mut outcome = Ok(());
        library.update(|library| {
            outcome = match editing_id {
                Some(id) => library.replace(SavedAction { id, name: name.get(), note: note.get(), shape, effects: built_effects }),
                None => library.add(name.get(), note.get(), shape, built_effects).map(|_| ()),
            };
        });
        match outcome {
            Ok(()) => on_close(),
            Err(err) => error.set(err.to_string()),
        }
    };

    view! {
        <div class="library-editor">
            <Tip topic=Topic::SavedActions>
                <label class="library-field">
                    "Name"
                    <input prop:value=move || name.get() on:input=move |ev| name.set(event_target_value(&ev)) />
                </label>
            </Tip>
            <label class="library-field">
                "Note"
                <input prop:value=move || note.get() on:input=move |ev| note.set(event_target_value(&ev)) />
            </label>
            <label class="library-field">
                <input type="checkbox" prop:checked=move || is_sequence.get() on:change=move |ev| is_sequence.set(event_target_checked(&ev)) />
                "Multi-step sorcery sequence"
            </label>

            {move || {
                if is_sequence.get() {
                    view! {
                        <div class="library-steps">
                            <Tip topic=Topic::SavedSequenceStep>
                                <h4>"Steps"</h4>
                            </Tip>
                            <For each=move || steps.get() key=|row| row.row_id let:row>
                                <div class="library-row">
                                    <input placeholder="Label" prop:value=move || row.label.get() on:input=move |ev| row.label.set(event_target_value(&ev)) />
                                    <input placeholder="Speed (blank = rolled)" prop:value=move || row.speed.get() on:input=move |ev| row.speed.set(event_target_value(&ev)) />
                                    <input placeholder="DV" prop:value=move || row.dv_penalty.get() on:input=move |ev| row.dv_penalty.set(event_target_value(&ev)) />
                                    <button on:click=move |_| remove_step(row.row_id)>"\u{2715}"</button>
                                </div>
                            </For>
                            <button on:click=add_step>"Add step"</button>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div class="library-single">
                            <select prop:value=move || format!("{:?}", kind.get()) on:change=move |ev| {
                                if let Some(template) = CATALOG.iter().find(|t| format!("{:?}", t.kind) == event_target_value(&ev)) {
                                    kind.set(template.kind);
                                }
                            }>
                                {CATALOG.iter().map(|template| view! {
                                    <option value=format!("{:?}", template.kind)>{template.name}</option>
                                }).collect_view()}
                            </select>
                            <label class="library-field">
                                "Speed"
                                <input prop:value=move || speed.get() on:input=move |ev| speed.set(event_target_value(&ev)) />
                            </label>
                            <label class="library-field">
                                "DV"
                                <input prop:value=move || dv_penalty.get() on:input=move |ev| dv_penalty.set(event_target_value(&ev)) />
                            </label>
                        </div>
                    }.into_any()
                }
            }}

            <div class="library-effects">
                <Tip topic=Topic::ActionEffects>
                    <h4>"Effects"</h4>
                </Tip>
                <For each=move || effects.get() key=|row| row.row_id let:row>
                    <div class="library-row">
                        <input placeholder="Label" prop:value=move || row.label.get() on:input=move |ev| row.label.set(event_target_value(&ev)) />
                        <label class="library-field">"in" <input placeholder="delay" prop:value=move || row.delay.get() on:input=move |ev| row.delay.set(event_target_value(&ev)) /> "ticks"</label>
                        <label class="library-field">"for" <input placeholder="ticks" prop:value=move || row.ticks.get() on:input=move |ev| row.ticks.set(event_target_value(&ev)) /> "ticks"</label>
                        <button on:click=move |_| remove_effect(row.row_id)>"\u{2715}"</button>
                    </div>
                </For>
                <button on:click=add_effect>"Add effect"</button>
            </div>

            {move || (!error.get().is_empty()).then(|| view! { <div class="library-error">{error.get()}</div> })}
            <button on:click=save class="library-save">"Save"</button>
        </div>
    }
}

#[component]
pub fn SavedActionList(library: Pref<Library>, on_edit: impl Fn(SavedId) + Copy + Send + Sync + 'static) -> impl IntoView {
    let rows = move || library.get().actions().to_vec();
    let remove = move |id: SavedId| {
        library.update(|library| _ = library.remove(id));
    };

    view! {
        <ul class="library-list">
            <For each=rows key=|action| action.id let:action>
                <li>
                    <span class="name">{action.name.clone()}</span>
                    <span class="library-shape">{shape_label(&action.shape)}</span>
                    <button on:click=move |_| on_edit(action.id)>"Edit"</button>
                    <button on:click=move |_| remove(action.id)>"Delete"</button>
                </li>
            </For>
        </ul>
        {move || rows().is_empty().then(|| view! { <p class="library-empty">"No saved actions yet."</p> })}
    }
}

fn shape_label(shape: &SavedShape) -> String {
    match shape {
        SavedShape::Single { .. } => "Action".to_string(),
        SavedShape::Sequence { steps } => format!("Sequence \u{00d7}{}", steps.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_label_distinguishes_single_and_sequence() {
        assert_eq!(shape_label(&SavedShape::Single { kind: ActionKind::Attack, speed: 4, dv_penalty: -1 }), "Action");
        assert_eq!(
            shape_label(&SavedShape::Sequence { steps: vec![SequenceStep { label: "Shape".to_string(), speed: SpeedSpec::Fixed(5), dv_penalty: -2 }] }),
            "Sequence \u{00d7}1"
        );
    }
}

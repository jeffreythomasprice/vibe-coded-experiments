use crate::library::{Library, SavedAction, SavedDeclaration, SavedId, SavedShape};
use crate::prefs::Prefs;
use crate::ui::glossary::{action_topic, sequence_topic, Topic};
use crate::ui::{DetailTip, Modal, SavedActionEditor, SavedActionList, Tip};
use exalted_battle_wheel::battle::{
    ActionTemplate, Battle, BattleEvent, BattleLog, CombatantId, CombatantState, Declaration, DvPenaltySpec,
    InterruptReason, JoinBattleResult, Phase, Sequence, SequenceTemplate, SpeedSpec, Tick, CATALOG, SEQUENCE_CATALOG,
};
use leptos::prelude::*;

#[component]
pub fn ActionPanel() -> impl IntoView {
    let log = expect_context::<RwSignal<BattleLog>>();
    let battle = expect_context::<Memo<Battle>>();

    let up_now = move || -> Vec<CombatantId> {
        let battle = battle.read();
        if !matches!(battle.phase, Phase::Running { .. }) {
            return Vec::new();
        }
        battle
            .combatants
            .iter()
            .filter(|c| !matches!(c.state, CombatantState::Inactive) && c.next_action_tick <= battle.current_tick)
            .map(|c| c.id)
            .collect()
    };

    let shaping_but_not_up = move || -> Vec<CombatantId> {
        let battle = battle.read();
        let up: Vec<CombatantId> = up_now();
        battle
            .combatants
            .iter()
            .filter(|c| matches!(c.state, CombatantState::InSequence(_)) && !up.contains(&c.id))
            .map(|c| c.id)
            .collect()
    };

    view! {
        <div class="action-panel">
            <Tip topic=Topic::UpNow>
                <h2>"Up now"</h2>
            </Tip>
            <For each=up_now key=|id| *id let:actor_id>
                <ActorRow actor_id=actor_id log=log battle=battle />
            </For>
            <Tip topic=Topic::ShapingSection>
                <h2>"Shaping (can be interrupted anytime)"</h2>
            </Tip>
            <For each=shaping_but_not_up key=|id| *id let:actor_id>
                <div class="actor-row">
                    <InterruptControls actor_id=actor_id log=log battle=battle />
                </div>
            </For>
        </div>
    }
}

#[component]
fn ActorRow(actor_id: CombatantId, log: RwSignal<BattleLog>, battle: Memo<Battle>) -> impl IntoView {
    let name = move || {
        battle
            .read()
            .find(actor_id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("#{}", actor_id.0))
    };

    let in_sequence = move || matches!(battle.read().find(actor_id).map(|c| &c.state), Some(CombatantState::InSequence(_)));

    view! {
        <div class="actor-row">
            <span class="name">{name}</span>
            {move || {
                if in_sequence() {
                    view! { <SequenceControls actor_id=actor_id log=log battle=battle /> }.into_any()
                } else {
                    view! { <NormalControls actor_id=actor_id log=log battle=battle /> }.into_any()
                }
            }}
        </div>
    }
}

fn format_speed(spec: SpeedSpec) -> String {
    match spec {
        SpeedSpec::Fixed(speed) => speed.to_string(),
        SpeedSpec::Variable { default } => format!("varies (default {default})"),
    }
}

fn format_dv_penalty(spec: DvPenaltySpec) -> String {
    match spec {
        DvPenaltySpec::Fixed(penalty) => penalty.to_string(),
        DvPenaltySpec::Variable { default } => format!("varies (default {default})"),
    }
}

/// Identifies a `<select>` option across all three catalogs by kind and position rather than by a
/// single flat index, so a Saved entry's identity survives edits and deletes elsewhere in the
/// list instead of silently pointing at whatever now sits at that index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChoiceKey {
    Action(usize),
    Sequence(usize),
    Saved(SavedId),
}

impl ChoiceKey {
    fn encode(self) -> String {
        match self {
            ChoiceKey::Action(index) => format!("a:{index}"),
            ChoiceKey::Sequence(index) => format!("s:{index}"),
            ChoiceKey::Saved(id) => format!("v:{id}"),
        }
    }

    fn parse(raw: &str) -> Option<ChoiceKey> {
        let (prefix, rest) = raw.split_once(':')?;
        match prefix {
            "a" => rest.parse().ok().map(ChoiceKey::Action),
            "s" => rest.parse().ok().map(ChoiceKey::Sequence),
            "v" => rest.parse().ok().map(ChoiceKey::Saved),
            _ => None,
        }
    }
}

#[derive(Clone)]
enum Choice {
    Action(&'static ActionTemplate),
    Sequence(&'static SequenceTemplate),
    Saved(SavedAction),
}

fn choice_for(key: ChoiceKey, library: &Library) -> Option<Choice> {
    match key {
        ChoiceKey::Action(index) => CATALOG.get(index).map(Choice::Action),
        ChoiceKey::Sequence(index) => SEQUENCE_CATALOG.get(index).map(Choice::Sequence),
        ChoiceKey::Saved(id) => library.find(id).cloned().map(Choice::Saved),
    }
}

/// Projects the tick each step of `sequence` is taken on, starting from `current_tick`. The final
/// step's own Speed is rolled via Join Battle rather than fixed, so it's flagged rather than timed.
fn sequence_timing(sequence: &Sequence, current_tick: Tick) -> String {
    let mut tick = current_tick;
    let last_index = sequence.steps.len() - 1;
    let mut parts = Vec::with_capacity(sequence.steps.len());
    for (i, step) in sequence.steps.iter().enumerate() {
        let is_cast = i == last_index;
        let label = if is_cast { "Cast" } else { "Shape" };
        let when = if i == 0 { format!("{label} now (tick {tick})") } else { format!("{label} on tick {tick}") };
        parts.push(if is_cast { format!("{when}, Speed rolled via Join Battle") } else { when });
        tick += step.speed.resolve(None);
    }
    parts.join(" → ")
}

/// Whether the Save/Manage overlay above `NormalControls` shows nothing, the saved-action list,
/// or the editor (`None` for a fresh save, `Some(id)` to edit an existing entry in place).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LibraryPanel {
    Closed,
    Manage,
    Edit(Option<SavedId>),
}

/// Snapshots what `NormalControls` currently has selected into a starting point for the Save
/// editor. A blank name falls back to the template's own name, same as `Declaration::declare`.
fn draft_from_choice(choice: Option<Choice>, name: &str, speed_override: &str, dv_override: &str) -> SavedAction {
    match choice {
        Some(Choice::Action(template)) => {
            let resolved_name = if name.trim().is_empty() { template.name.to_string() } else { name.trim().to_string() };
            let speed = template.speed.resolve(speed_override.trim().parse().ok());
            let dv_penalty = template.dv_penalty.resolve(dv_override.trim().parse().ok());
            SavedAction {
                id: 0,
                name: resolved_name,
                note: String::new(),
                shape: SavedShape::Single { kind: template.kind, speed, dv_penalty },
                effects: Vec::new(),
            }
        }
        Some(Choice::Sequence(template)) => SavedAction {
            id: 0,
            name: template.name.to_string(),
            note: String::new(),
            shape: SavedShape::Sequence { steps: template.build().steps },
            effects: Vec::new(),
        },
        Some(Choice::Saved(saved)) => saved,
        None => SavedAction {
            id: 0,
            name: String::new(),
            note: String::new(),
            shape: SavedShape::Single { kind: exalted_battle_wheel::battle::ActionKind::Custom, speed: 5, dv_penalty: 0 },
            effects: Vec::new(),
        },
    }
}

#[component]
fn NormalControls(actor_id: CombatantId, log: RwSignal<BattleLog>, battle: Memo<Battle>) -> impl IntoView {
    let library = expect_context::<Prefs>().library;

    let name = RwSignal::new(String::new());
    let speed_override = RwSignal::new(String::new());
    let dv_override = RwSignal::new(String::new());
    let selected_key = RwSignal::new(ChoiceKey::Action(0));
    let panel = RwSignal::new(LibraryPanel::Closed);
    let draft = RwSignal::new(None::<SavedAction>);

    let declare_topic = Signal::derive(move || match choice_for(selected_key.get(), &library.get()) {
        Some(Choice::Sequence(_)) => Topic::DeclareSequence,
        Some(Choice::Saved(saved)) if matches!(saved.shape, SavedShape::Sequence { .. }) => Topic::DeclareSequence,
        _ => Topic::Declare,
    });
    let declare_detail = Signal::derive(move || match choice_for(selected_key.get(), &library.get()) {
        Some(Choice::Sequence(template)) => sequence_timing(&template.build(), battle.read().current_tick),
        Some(Choice::Saved(saved)) => match saved.build(&[]) {
            SavedDeclaration::Sequence(sequence) => sequence_timing(&sequence, battle.read().current_tick),
            SavedDeclaration::Action(_) => String::new(),
        },
        _ => String::new(),
    });

    // `None` before the first attempt; afterwards holds what that attempt did, so the form can
    // show why a Declare click seemingly did nothing instead of leaving the user guessing — a
    // reflexive action like Move legitimately leaves the actor's tick, DV, and "Up now" slot
    // unchanged on success (RULES.md §4.8, p. 145), and a rejected event needs a reason on screen
    // rather than only in the browser console.
    let declare_result = RwSignal::new(None::<Result<String, String>>);

    let declare = move |_| {
        let mut outcome = Ok(());
        let mut reflexive = false;
        match choice_for(selected_key.get(), &library.get()) {
            Some(Choice::Action(template)) => {
                let declaration = Declaration {
                    name: Some(name.get()),
                    speed: speed_override.get().parse().ok(),
                    dv_penalty: dv_override.get().parse().ok(),
                    ..Default::default()
                };
                let action = template.declare(declaration);
                reflexive = action.reflexive;
                log.update(|log| outcome = log.push(BattleEvent::DeclareAction { actor: actor_id, action }));
            }
            Some(Choice::Sequence(template)) => {
                let sequence = template.build();
                log.update(|log| outcome = log.push(BattleEvent::StartSequence { actor: actor_id, sequence }));
            }
            Some(Choice::Saved(saved)) => {
                log.update(|log| {
                    let ids: Vec<_> = (0..saved.effects.len()).map(|_| log.alloc_marker_id()).collect();
                    let event = match saved.build(&ids) {
                        SavedDeclaration::Action(action) => {
                            reflexive = action.reflexive;
                            BattleEvent::DeclareAction { actor: actor_id, action }
                        }
                        SavedDeclaration::Sequence(sequence) => BattleEvent::StartSequence { actor: actor_id, sequence },
                    };
                    outcome = log.push(event);
                });
            }
            None => {}
        }
        declare_result.set(Some(match outcome {
            Ok(()) if reflexive => {
                Ok("Declared. Reflexive actions don't cost time, so this actor's tick, DV, and \u{201c}Up now\u{201d} slot stay the same \u{2014} check the Event Log to confirm it was recorded.".to_string())
            }
            Ok(()) => Ok("Declared.".to_string()),
            Err(error) => {
                tracing::error!(%error, "could not declare action");
                Err(error.to_string())
            }
        }));
    };

    let open_save = move |_| {
        draft.set(Some(draft_from_choice(choice_for(selected_key.get(), &library.get()), &name.get(), &speed_override.get(), &dv_override.get())));
        panel.set(LibraryPanel::Edit(None));
    };
    let open_manage = move |_| panel.set(LibraryPanel::Manage);
    let close_panel = move || panel.set(LibraryPanel::Closed);
    let edit_existing = move |id: SavedId| {
        if let Some(saved) = library.get().find(id).cloned() {
            draft.set(Some(saved));
            panel.set(LibraryPanel::Edit(Some(id)));
        }
    };

    view! {
        <Tip topic=Topic::ActionSelect>
            <select
                prop:value=move || selected_key.get().encode()
                on:change=move |ev| {
                    if let Some(key) = ChoiceKey::parse(&event_target_value(&ev)) {
                        selected_key.set(key);
                    }
                    name.set(String::new());
                    speed_override.set(String::new());
                    dv_override.set(String::new());
                    declare_result.set(None);
                }
            >
                {CATALOG.iter().enumerate().map(|(i, template)| view! {
                    <option value=ChoiceKey::Action(i).encode()>{template.name}</option>
                }).collect_view()}
                <optgroup label="Sorcery">
                    {SEQUENCE_CATALOG.iter().enumerate().map(|(i, template)| view! {
                        <option value=ChoiceKey::Sequence(i).encode()>{template.name}</option>
                    }).collect_view()}
                </optgroup>
                <optgroup label="Saved">
                    {move || library.get().actions().iter().map(|saved| {
                        let key = ChoiceKey::Saved(saved.id).encode();
                        view! { <option value=key>{saved.name.clone()}</option> }
                    }).collect_view()}
                </optgroup>
            </select>
        </Tip>
        {move || {
            choice_for(selected_key.get(), &library.get()).map(|choice| match choice {
                Choice::Action(template) => {
                    let kind_topic = action_topic(template.kind);
                    let kind_entry = kind_topic.entry();
                    view! {
                        <div class="action-summary">
                            <Tip topic=Topic::Speed>
                                <span class="action-summary-chip">"Speed " {format_speed(template.speed)}</span>
                            </Tip>
                            <Tip topic=Topic::DvPenalty>
                                <span class="action-summary-chip">"DV " {format_dv_penalty(template.dv_penalty)}</span>
                            </Tip>
                            <Tip topic=Topic::Reflexive>
                                <span class="action-summary-chip">
                                    "Reflexive: " {if template.reflexive { "yes" } else { "no" }}
                                </span>
                            </Tip>
                            <Tip topic=Topic::Flurryable>
                                <span class="action-summary-chip">
                                    "Flurryable: " {if template.flurryable { "yes" } else { "no" }}
                                </span>
                            </Tip>
                            <Tip topic=kind_topic>
                                <span class="action-summary-note">{kind_entry.what}</span>
                            </Tip>
                        </div>
                    }
                        .into_any()
                }
                Choice::Sequence(template) => {
                    let kind_topic = sequence_topic(template.kind);
                    let kind_entry = kind_topic.entry();
                    view! {
                        <div class="action-summary">
                            <Tip topic=Topic::SequenceStep>
                                <span class="action-summary-chip">"Steps " {template.shape_actions + 1}</span>
                            </Tip>
                            <Tip topic=Topic::Speed>
                                <span class="action-summary-chip">"Shape Speed 5"</span>
                            </Tip>
                            <Tip topic=Topic::DvPenalty>
                                <span class="action-summary-chip">"DV " {template.shape_dv}</span>
                            </Tip>
                            <Tip topic=Topic::SequenceStep>
                                <span class="action-summary-chip">"Cast Speed varies"</span>
                            </Tip>
                            <Tip topic=kind_topic>
                                <span class="action-summary-note">{kind_entry.what}</span>
                            </Tip>
                        </div>
                    }
                        .into_any()
                }
                Choice::Saved(saved) => {
                    // Extracted as owned, Copy primitives before building any view: `Tip`'s
                    // children close over their content, which must be `'static` and so cannot
                    // hold a reference borrowed from `saved.shape`.
                    let single: Option<(u32, i32)> = match &saved.shape {
                        SavedShape::Single { speed, dv_penalty, .. } => Some((*speed, *dv_penalty)),
                        SavedShape::Sequence { .. } => None,
                    };
                    let step_count: Option<usize> = match &saved.shape {
                        SavedShape::Sequence { steps } => Some(steps.len()),
                        SavedShape::Single { .. } => None,
                    };
                    let shape_chip = match single {
                        Some((speed, dv_penalty)) => view! {
                            <>
                                <Tip topic=Topic::Speed><span class="action-summary-chip">"Speed " {speed}</span></Tip>
                                <Tip topic=Topic::DvPenalty><span class="action-summary-chip">"DV " {dv_penalty}</span></Tip>
                            </>
                        }.into_any(),
                        None => view! {
                            <Tip topic=Topic::SequenceStep><span class="action-summary-chip">"Steps " {step_count.unwrap_or(0)}</span></Tip>
                        }.into_any(),
                    };
                    let effect_count = saved.effects.len();
                    let note = saved.note.clone();
                    view! {
                        <div class="action-summary">
                            {shape_chip}
                            {(effect_count > 0).then(|| view! {
                                <Tip topic=Topic::ActionEffects>
                                    <span class="action-summary-chip">"Effects " {effect_count}</span>
                                </Tip>
                            })}
                            {(!note.is_empty()).then(|| view! {
                                <Tip topic=Topic::SavedActions>
                                    <span class="action-summary-note">{note}</span>
                                </Tip>
                            })}
                        </div>
                    }
                        .into_any()
                }
            })
        }}
        {move || {
            matches!(choice_for(selected_key.get(), &library.get()), Some(Choice::Action(_))).then(|| {
                let Some(Choice::Action(template)) = choice_for(selected_key.get(), &library.get()) else { unreachable!() };
                view! {
                    <Tip topic=Topic::ActionName>
                        <input
                            placeholder=template.name
                            prop:value=move || name.get()
                            on:input=move |ev| name.set(event_target_value(&ev))
                        />
                    </Tip>
                    <Tip topic=Topic::SpeedOverride>
                        <input
                            placeholder="speed override"
                            prop:value=move || speed_override.get()
                            on:input=move |ev| speed_override.set(event_target_value(&ev))
                        />
                    </Tip>
                    <Tip topic=Topic::DvOverride>
                        <input
                            placeholder="DV override"
                            prop:value=move || dv_override.get()
                            on:input=move |ev| dv_override.set(event_target_value(&ev))
                        />
                    </Tip>
                }
            })
        }}
        <DetailTip topic=declare_topic detail=declare_detail>
            <button on:click=declare>"Declare"</button>
        </DetailTip>
        {move || declare_result.get().map(|result| match result {
            Ok(message) => view! { <div class="action-status">{message}</div> }.into_any(),
            Err(message) => view! { <div class="action-error">{message}</div> }.into_any(),
        })}
        <Tip topic=Topic::SaveAction>
            <button on:click=open_save>"Save\u{2026}"</button>
        </Tip>
        <Tip topic=Topic::ManageSavedActions>
            <button on:click=open_manage>"Manage\u{2026}"</button>
        </Tip>
        {move || match panel.get() {
            LibraryPanel::Closed => None,
            LibraryPanel::Manage => Some(view! {
                <Modal title="Saved Actions" on_close=close_panel>
                    <SavedActionList library=library on_edit=edit_existing />
                </Modal>
            }.into_any()),
            LibraryPanel::Edit(editing_id) => draft.get().map(|initial| view! {
                <Modal title="Save Action" on_close=close_panel>
                    <SavedActionEditor library=library initial=initial editing_id=editing_id on_close=close_panel />
                </Modal>
            }.into_any()),
        }}
    }
}

#[component]
fn SequenceControls(actor_id: CombatantId, log: RwSignal<BattleLog>, battle: Memo<Battle>) -> impl IntoView {
    let speed_override = RwSignal::new(String::new());

    let advance = move |_| {
        let speed = speed_override.get().parse().ok();
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::AdvanceSequence { actor: actor_id, speed_override: speed }) {
                tracing::error!(%error, "could not advance sequence");
            }
        });
    };

    view! {
        <InterruptControls actor_id=actor_id log=log battle=battle />
        <Tip topic=Topic::CastSpeedOverride>
            <input
                placeholder="speed override (Cast)"
                prop:value=move || speed_override.get()
                on:input=move |ev| speed_override.set(event_target_value(&ev))
            />
        </Tip>
        <Tip topic=Topic::AdvanceSequence>
            <button on:click=advance>"Advance"</button>
        </Tip>
    }
}

#[component]
fn InterruptControls(actor_id: CombatantId, log: RwSignal<BattleLog>, battle: Memo<Battle>) -> impl IntoView {
    let rejoin_successes = RwSignal::new(String::new());

    let description = move || {
        let battle = battle.read();
        let Some(combatant) = battle.find(actor_id) else { return String::new() };
        let CombatantState::InSequence(sequence) = &combatant.state else { return String::new() };
        format!(
            "{} — step {}/{}: {} (resolves tick {})",
            sequence.name,
            sequence.current + 1,
            sequence.steps.len(),
            sequence.current_step().label,
            combatant.next_action_tick
        )
    };

    let rejoin = move || -> JoinBattleResult {
        match rejoin_successes.get().parse::<u32>() {
            Ok(successes) => JoinBattleResult::Successes(successes),
            Err(_) => JoinBattleResult::Botch,
        }
    };

    let interrupt_voluntary = move |_| {
        log.update(|log| {
            if let Err(error) =
                log.push(BattleEvent::InterruptSequence { actor: actor_id, reason: InterruptReason::Voluntary, rejoin: rejoin() })
            {
                tracing::error!(%error, "could not interrupt sequence");
            }
        });
    };

    let interrupt_distracted = move |_| {
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::InterruptSequence {
                actor: actor_id,
                reason: InterruptReason::FailedOccultCheck,
                rejoin: rejoin(),
            }) {
                tracing::error!(%error, "could not interrupt sequence");
            }
        });
    };

    view! {
        <Tip topic=Topic::SequenceStep>
            <span class="sequence-description">{description}</span>
        </Tip>
        <Tip topic=Topic::RejoinSuccesses>
            <input
                placeholder="rejoin successes"
                prop:value=move || rejoin_successes.get()
                on:input=move |ev| rejoin_successes.set(event_target_value(&ev))
            />
        </Tip>
        <Tip topic=Topic::InterruptSequence>
            <button on:click=interrupt_voluntary class="interrupt-button">"Interrupt"</button>
        </Tip>
        <Tip topic=Topic::InterruptDistracted>
            <button on:click=interrupt_distracted class="interrupt-button">"Distracted"</button>
        </Tip>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exalted_battle_wheel::battle::SequenceKind;

    #[test]
    fn choice_key_round_trips_through_its_encoding() {
        for key in [ChoiceKey::Action(3), ChoiceKey::Sequence(1), ChoiceKey::Saved(12)] {
            assert_eq!(ChoiceKey::parse(&key.encode()), Some(key));
        }
    }

    #[test]
    fn choice_key_parse_rejects_garbage() {
        assert_eq!(ChoiceKey::parse("nonsense"), None);
        assert_eq!(ChoiceKey::parse("a:not-a-number"), None);
        assert_eq!(ChoiceKey::parse("z:0"), None);
    }

    #[test]
    fn choice_for_resolves_every_catalog_range() {
        let library = Library::default();
        assert!(matches!(choice_for(ChoiceKey::Action(0), &library), Some(Choice::Action(_))));
        assert!(matches!(choice_for(ChoiceKey::Action(CATALOG.len()), &library), None));
        assert!(matches!(choice_for(ChoiceKey::Sequence(0), &library), Some(Choice::Sequence(_))));
        assert!(matches!(choice_for(ChoiceKey::Sequence(SEQUENCE_CATALOG.len()), &library), None));
        assert!(matches!(choice_for(ChoiceKey::Saved(0), &library), None));
    }

    #[test]
    fn choice_for_sequence_entries_match_their_catalog_order() {
        let library = Library::default();
        let Some(Choice::Sequence(template)) = choice_for(ChoiceKey::Sequence(0), &library) else { panic!("expected a sequence") };
        assert_eq!(template.kind, SequenceKind::ShapeTerrestrial);
    }

    #[test]
    fn choice_for_finds_a_saved_action_by_id() {
        let mut library = Library::default();
        let id = library
            .add("Sweeping Blow".to_string(), String::new(), SavedShape::Single { kind: exalted_battle_wheel::battle::ActionKind::Attack, speed: 4, dv_penalty: -1 }, Vec::new())
            .unwrap();
        let Some(Choice::Saved(saved)) = choice_for(ChoiceKey::Saved(id), &library) else { panic!("expected a saved action") };
        assert_eq!(saved.name, "Sweeping Blow");
    }

    #[test]
    fn sequence_timing_reports_the_declare_tick_for_every_step() {
        let sequence = Sequence::shape_celestial();
        assert_eq!(
            sequence_timing(&sequence, 7),
            "Shape now (tick 7) → Shape on tick 12 → Cast on tick 17, Speed rolled via Join Battle"
        );
    }

    #[test]
    fn sequence_timing_for_terrestrial_is_shape_then_cast() {
        let sequence = Sequence::shape_terrestrial();
        assert_eq!(sequence_timing(&sequence, 0), "Shape now (tick 0) → Cast on tick 5, Speed rolled via Join Battle");
    }
}

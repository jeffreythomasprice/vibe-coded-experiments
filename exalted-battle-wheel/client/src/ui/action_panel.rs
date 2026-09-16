use crate::battle_net::Battles;
use shared::library::{Library, SavedAction, SavedDeclaration, SavedId, SavedShape};
use crate::prefs::Prefs;
use crate::ui::format::{format_dv_penalty, format_speed};
use crate::ui::glossary::{action_topic, sequence_topic, Topic};
use crate::ui::{DetailTip, Modal, SavedActionEditor, SavedActionList, Tip};
use shared::battle::{
    catalog, catalog_index, ActionKind, ActionTemplate, Battle, BattleEvent, BattleMode, CombatantId, CombatantState,
    Declaration, InterruptReason, JoinBattleResult, MarkerId, Phase, Sequence, SequenceTemplate, Tick,
    SEQUENCE_CATALOG,
};
use leptos::prelude::*;

/// Shared between the action panel and the reference rail: which "Up now" row a rail click lands
/// on, and the pick itself. Lives here, not in `reference.rs`, because the action panel owns the
/// rows a pick can target. A `Copy` newtype so it doesn't collide in context with any other
/// `RwSignal` of the same inner type (context is keyed by type alone).
#[derive(Clone, Copy)]
pub struct RailSelection {
    /// The last selectable row the user touched (click or focus). Advisory only — `rail_target`
    /// re-validates it against the live battle every time, so a stale id (its actor went
    /// Inactive, started shaping, or declared away its tick) falls back automatically.
    pub touched: RwSignal<Option<CombatantId>>,
    /// One-shot: set by a reference-rail row, consumed by the matching actor's dropdown, then
    /// left for that actor to overwrite or clear on its next pick.
    pub pick: RwSignal<Option<(CombatantId, ActionKind)>>,
}

impl RailSelection {
    pub fn new() -> Self {
        Self { touched: RwSignal::new(None), pick: RwSignal::new(None) }
    }
}

impl Default for RailSelection {
    fn default() -> Self {
        Self::new()
    }
}

/// Everyone whose next action tick has arrived and who must declare an action before the tick can
/// advance (RULES.md §4, p. 141 — see `Topic::UpNow`). Empty before `Phase::Running`.
pub fn actors_up_now(battle: &Battle) -> Vec<CombatantId> {
    if !matches!(battle.phase, Phase::Running { .. }) {
        return Vec::new();
    }
    battle
        .combatants
        .iter()
        .filter(|c| !matches!(c.state, CombatantState::Inactive) && c.next_action_tick <= battle.current_tick)
        .map(|c| c.id)
        .collect()
}

/// `actors_up_now` minus anyone mid-sequence: a shaping sorcerer's row offers Advance/Interrupt,
/// not an action dropdown, so there's nowhere for a reference-rail pick to land.
pub fn selectable_actors(battle: &Battle) -> Vec<CombatantId> {
    actors_up_now(battle)
        .into_iter()
        .filter(|id| !matches!(battle.find(*id).map(|c| &c.state), Some(CombatantState::InSequence(_))))
        .collect()
}

/// The actor a reference-rail click targets: `touched` if it's still selectable, else the topmost
/// selectable actor. `None` means the rail is inert — Setup, nobody up, or everyone shaping.
pub fn rail_target(battle: &Battle, touched: Option<CombatantId>) -> Option<CombatantId> {
    let selectable = selectable_actors(battle);
    touched.filter(|id| selectable.contains(id)).or_else(|| selectable.first().copied())
}

#[component]
pub fn ActionPanel() -> impl IntoView {
    let battles = expect_context::<Battles>();
    let battle = expect_context::<Memo<Battle>>();

    let up_now = move || actors_up_now(&battle.read());

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
                <ActorRow actor_id=actor_id battles=battles battle=battle />
            </For>
            <Tip topic=Topic::ShapingSection>
                <h2>"Shaping (can be interrupted anytime)"</h2>
            </Tip>
            <For each=shaping_but_not_up key=|id| *id let:actor_id>
                <div class="actor-row">
                    <InterruptControls actor_id=actor_id battles=battles battle=battle />
                </div>
            </For>
        </div>
    }
}

#[component]
fn ActorRow(actor_id: CombatantId, battles: Battles, battle: Memo<Battle>) -> impl IntoView {
    let selection = expect_context::<RailSelection>();

    let name = move || {
        battle
            .read()
            .find(actor_id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| format!("#{}", actor_id.0))
    };

    let in_sequence = move || matches!(battle.read().find(actor_id).map(|c| &c.state), Some(CombatantState::InSequence(_)));

    // Evaluated when the event fires, not when the handler is attached: this div outlives the
    // swap between sequence and normal controls below, so whether a click should claim the
    // reference rail's target can change without this component remounting.
    let touch = move || {
        if !in_sequence() {
            selection.touched.set(Some(actor_id));
        }
    };

    view! {
        <div
            class="actor-row"
            class:actor-row-targeted=move || rail_target(&battle.read(), selection.touched.get()) == Some(actor_id)
            on:pointerdown=move |_| touch()
            on:focusin=move |_| touch()
        >
            <span class="name">{name}</span>
            {move || {
                if in_sequence() {
                    view! { <SequenceControls actor_id=actor_id battles=battles battle=battle /> }.into_any()
                } else {
                    view! { <NormalControls actor_id=actor_id battles=battles battle=battle /> }.into_any()
                }
            }}
        </div>
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

fn choice_for(mode: BattleMode, key: ChoiceKey, library: &Library) -> Option<Choice> {
    match key {
        ChoiceKey::Action(index) => catalog(mode).nth(index).map(Choice::Action),
        ChoiceKey::Sequence(index) => SEQUENCE_CATALOG.get(index).map(Choice::Sequence),
        ChoiceKey::Saved(id) => library.find(id).cloned().filter(|saved| saved_matches_mode(saved, mode)).map(Choice::Saved),
    }
}

/// A saved sequence is mode-less (sorcery is Personal-only, and its optgroup is already gated),
/// so it always matches; a saved single action only matches the mode it was saved under.
fn saved_matches_mode(saved: &SavedAction, mode: BattleMode) -> bool {
    match &saved.shape {
        SavedShape::Single { mode: saved_mode, .. } => *saved_mode == mode,
        SavedShape::Sequence { .. } => true,
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
fn draft_from_choice(mode: BattleMode, choice: Option<Choice>, name: &str, speed_override: &str, dv_override: &str) -> SavedAction {
    match choice {
        Some(Choice::Action(template)) => {
            let resolved_name = if name.trim().is_empty() { template.name.to_string() } else { name.trim().to_string() };
            let speed = template.speed.resolve(speed_override.trim().parse().ok());
            let dv_penalty = template.dv_penalty.resolve(dv_override.trim().parse().ok());
            SavedAction {
                id: 0,
                name: resolved_name,
                note: String::new(),
                shape: SavedShape::Single { mode, kind: template.kind, speed, dv_penalty },
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
            shape: SavedShape::Single { mode, kind: shared::battle::ActionKind::Custom, speed: 5, dv_penalty: 0 },
            effects: Vec::new(),
        },
    }
}

#[component]
fn NormalControls(actor_id: CombatantId, battles: Battles, battle: Memo<Battle>) -> impl IntoView {
    let library = expect_context::<Prefs>().library;
    let selection = expect_context::<RailSelection>();

    let name = RwSignal::new(String::new());
    let speed_override = RwSignal::new(String::new());
    let dv_override = RwSignal::new(String::new());
    let selected_key = RwSignal::new(ChoiceKey::Action(0));
    let panel = RwSignal::new(LibraryPanel::Closed);
    let draft = RwSignal::new(None::<SavedAction>);

    let mode = move || battle.read().mode;

    // `None` before the first attempt; afterwards holds what that attempt did, so the form can
    // show why a Declare click seemingly did nothing instead of leaving the user guessing — a
    // reflexive action like Move legitimately leaves the actor's tick, DV, and "Up now" slot
    // unchanged on success (RULES.md §4.8, p. 145), and a rejected event needs a reason on screen
    // rather than only in the browser console.
    let declare_result = RwSignal::new(None::<Result<String, String>>);

    let reset_fields = move || {
        name.set(String::new());
        speed_override.set(String::new());
        dv_override.set(String::new());
        declare_result.set(None);
    };

    // The reference rail hands off a pick as a one-shot `(actor, kind)` message rather than
    // writing `selected_key` directly, since every actor's `NormalControls` shares the same
    // signal — each instance ignores picks aimed at a different actor. `immediate: false` so a
    // pick made while this row was showing `SequenceControls` isn't replayed once it remounts
    // here; a plain `RwSignal` still notifies on a repeat `set`, so re-clicking the same rail row
    // re-applies it. `catalog_index` failing (a stale kind from a mode that has since changed)
    // just drops the pick rather than declaring something the user didn't click.
    Effect::watch(
        move || selection.pick.get(),
        move |pick, _, _: Option<()>| {
            let Some((actor, kind)) = *pick else { return };
            if actor != actor_id {
                return;
            }
            let Some(index) = catalog_index(battle.read_untracked().mode, kind) else { return };
            selected_key.set(ChoiceKey::Action(index));
            reset_fields();
        },
        false,
    );

    let declare_topic = Signal::derive(move || match choice_for(mode(), selected_key.get(), &library.get()) {
        Some(Choice::Sequence(_)) => Topic::DeclareSequence,
        Some(Choice::Saved(saved)) if matches!(saved.shape, SavedShape::Sequence { .. }) => Topic::DeclareSequence,
        _ => Topic::Declare,
    });
    let declare_detail = Signal::derive(move || match choice_for(mode(), selected_key.get(), &library.get()) {
        Some(Choice::Sequence(template)) => sequence_timing(&template.build(), battle.read().current_tick),
        Some(Choice::Saved(saved)) => match saved.build(&[]) {
            Ok(SavedDeclaration::Sequence(sequence)) => sequence_timing(&sequence, battle.read().current_tick),
            Ok(SavedDeclaration::Action(_)) | Err(_) => String::new(),
        },
        _ => String::new(),
    });

    let settle = move |result: Result<(), String>, reflexive: bool| {
        declare_result.set(Some(match result {
            Ok(()) if reflexive => {
                Ok("Declared. Reflexive actions don't cost time, so this actor's tick, DV, and \u{201c}Up now\u{201d} slot stay the same \u{2014} check the Event Log to confirm it was recorded.".to_string())
            }
            Ok(()) => Ok("Declared.".to_string()),
            Err(error) => {
                tracing::error!(%error, "could not declare action");
                Err(error)
            }
        }));
    };

    let declare = move |_| {
        match choice_for(mode(), selected_key.get(), &library.get()) {
            Some(Choice::Action(template)) => {
                let declaration = Declaration {
                    name: Some(name.get()),
                    speed: speed_override.get().parse().ok(),
                    dv_penalty: dv_override.get().parse().ok(),
                    ..Default::default()
                };
                let action = template.declare(declaration);
                let reflexive = action.reflexive;
                battles.push_with(BattleEvent::DeclareAction { actor: actor_id, action }, move |result| {
                    settle(result.map_err(|error| error.to_string()), reflexive)
                });
            }
            Some(Choice::Sequence(template)) => {
                let sequence = template.build();
                battles.push_with(BattleEvent::StartSequence { actor: actor_id, sequence }, move |result| {
                    settle(result.map_err(|error| error.to_string()), false)
                });
            }
            Some(Choice::Saved(saved)) => {
                // Placeholders: the real ids are stamped from whichever log is authoritative for
                // the room once this proposal is sequenced (see `Battles::push_minting`), so any
                // value works here as long as there's one per effect.
                let placeholders = vec![MarkerId(0); saved.effects.len()];
                match saved.build(&placeholders) {
                    Ok(SavedDeclaration::Action(action)) => {
                        let reflexive = action.reflexive;
                        battles.push_minting_with(BattleEvent::DeclareAction { actor: actor_id, action }, move |result| {
                            settle(result.map_err(|error| error.to_string()), reflexive)
                        });
                    }
                    Ok(SavedDeclaration::Sequence(sequence)) => {
                        battles.push_minting_with(BattleEvent::StartSequence { actor: actor_id, sequence }, move |result| {
                            settle(result.map_err(|error| error.to_string()), false)
                        });
                    }
                    Err(error) => settle(Err(error.to_string()), false),
                }
            }
            None => {}
        }
    };

    let open_save = move |_| {
        draft.set(Some(draft_from_choice(mode(), choice_for(mode(), selected_key.get(), &library.get()), &name.get(), &speed_override.get(), &dv_override.get())));
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
                    reset_fields();
                }
            >
                {move || catalog(mode()).enumerate().map(|(i, template)| view! {
                    <option value=ChoiceKey::Action(i).encode()>{template.name}</option>
                }).collect_view()}
                {move || (mode() == BattleMode::Personal).then(|| view! {
                    <optgroup label="Sorcery">
                        {SEQUENCE_CATALOG.iter().enumerate().map(|(i, template)| view! {
                            <option value=ChoiceKey::Sequence(i).encode()>{template.name}</option>
                        }).collect_view()}
                    </optgroup>
                })}
                <optgroup label="Saved">
                    {move || library.get().actions().iter().filter(|saved| saved_matches_mode(saved, mode())).map(|saved| {
                        let key = ChoiceKey::Saved(saved.id).encode();
                        view! { <option value=key>{saved.name.clone()}</option> }
                    }).collect_view()}
                </optgroup>
            </select>
        </Tip>
        {move || {
            choice_for(mode(), selected_key.get(), &library.get()).map(|choice| match choice {
                Choice::Action(template) => {
                    let kind_topic = action_topic(mode(), template.kind);
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
                            {kind_topic.map(|topic| { let entry = topic.entry(); view! {
                                <Tip topic=topic>
                                    <span class="action-summary-note">{entry.what}</span>
                                </Tip>
                            }})}
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
            matches!(choice_for(mode(), selected_key.get(), &library.get()), Some(Choice::Action(_))).then(|| {
                let Some(Choice::Action(template)) = choice_for(mode(), selected_key.get(), &library.get()) else { unreachable!() };
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
            <button on:click=declare disabled=move || battles.read_only().get()>"Declare"</button>
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
                    <SavedActionEditor library=library initial=initial editing_id=editing_id mode=mode() on_close=close_panel />
                </Modal>
            }.into_any()),
        }}
    }
}

#[component]
fn SequenceControls(actor_id: CombatantId, battles: Battles, battle: Memo<Battle>) -> impl IntoView {
    let speed_override = RwSignal::new(String::new());

    let advance = move |_| {
        let speed = speed_override.get().parse().ok();
        battles.push(BattleEvent::AdvanceSequence { actor: actor_id, speed_override: speed });
    };

    view! {
        <InterruptControls actor_id=actor_id battles=battles battle=battle />
        <Tip topic=Topic::CastSpeedOverride>
            <input
                placeholder="speed override (Cast)"
                prop:value=move || speed_override.get()
                on:input=move |ev| speed_override.set(event_target_value(&ev))
            />
        </Tip>
        <Tip topic=Topic::AdvanceSequence>
            <button on:click=advance disabled=move || battles.read_only().get()>"Advance"</button>
        </Tip>
    }
}

#[component]
fn InterruptControls(actor_id: CombatantId, battles: Battles, battle: Memo<Battle>) -> impl IntoView {
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
        battles.push(BattleEvent::InterruptSequence { actor: actor_id, reason: InterruptReason::Voluntary, rejoin: rejoin() });
    };

    let interrupt_distracted = move |_| {
        battles.push(BattleEvent::InterruptSequence { actor: actor_id, reason: InterruptReason::FailedOccultCheck, rejoin: rejoin() });
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
            <button on:click=interrupt_voluntary class="interrupt-button" disabled=move || battles.read_only().get()>
                "Interrupt"
            </button>
        </Tip>
        <Tip topic=Topic::InterruptDistracted>
            <button on:click=interrupt_distracted class="interrupt-button" disabled=move || battles.read_only().get()>
                "Distracted"
            </button>
        </Tip>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::battle::{apply, template, SequenceKind, Side};

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
        let personal_len = shared::battle::PERSONAL_CATALOG.len();
        assert!(matches!(choice_for(BattleMode::Personal, ChoiceKey::Action(0), &library), Some(Choice::Action(_))));
        assert!(matches!(choice_for(BattleMode::Personal, ChoiceKey::Action(personal_len), &library), None));
        assert!(matches!(choice_for(BattleMode::Personal, ChoiceKey::Sequence(0), &library), Some(Choice::Sequence(_))));
        assert!(matches!(choice_for(BattleMode::Personal, ChoiceKey::Sequence(SEQUENCE_CATALOG.len()), &library), None));
        assert!(matches!(choice_for(BattleMode::Personal, ChoiceKey::Saved(0), &library), None));
    }

    #[test]
    fn choice_for_sequence_entries_match_their_catalog_order() {
        let library = Library::default();
        let Some(Choice::Sequence(template)) = choice_for(BattleMode::Personal, ChoiceKey::Sequence(0), &library) else { panic!("expected a sequence") };
        assert_eq!(template.kind, SequenceKind::ShapeTerrestrial);
    }

    #[test]
    fn choice_for_finds_a_saved_action_by_id() {
        let mut library = Library::default();
        let id = library
            .add(
                "Sweeping Blow".to_string(),
                String::new(),
                SavedShape::Single { mode: BattleMode::Personal, kind: shared::battle::ActionKind::Attack, speed: 4, dv_penalty: -1 },
                Vec::new(),
            )
            .unwrap();
        let Some(Choice::Saved(saved)) = choice_for(BattleMode::Personal, ChoiceKey::Saved(id), &library) else { panic!("expected a saved action") };
        assert_eq!(saved.name, "Sweeping Blow");
    }

    #[test]
    fn choice_for_hides_a_saved_action_from_a_different_mode() {
        let mut library = Library::default();
        let id = library
            .add(
                "Rally the Line".to_string(),
                String::new(),
                SavedShape::Single { mode: BattleMode::Mass, kind: shared::battle::ActionKind::Rally, speed: 4, dv_penalty: -1 },
                Vec::new(),
            )
            .unwrap();
        assert!(matches!(choice_for(BattleMode::Personal, ChoiceKey::Saved(id), &library), None));
        assert!(matches!(choice_for(BattleMode::Mass, ChoiceKey::Saved(id), &library), Some(Choice::Saved(_))));
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

    #[test]
    fn pick_key_round_trips_through_choice_for_every_catalog_entry() {
        // The whole point of resolving a reference-rail pick through `catalog_index` rather than
        // matching on `ActionKind` alone: the same key must land on the same name in every mode,
        // even where two modes give a shared `ActionKind` different names (personal Aim vs.
        // social "Monologue / Study").
        let library = Library::default();
        for mode in BattleMode::ALL {
            for template in catalog(mode) {
                let index = catalog_index(mode, template.kind).expect("template came from this catalog");
                let Some(Choice::Action(resolved)) = choice_for(mode, ChoiceKey::Action(index), &library) else {
                    panic!("{mode:?} index {index} did not resolve to an action");
                };
                assert_eq!(resolved.name, template.name, "{mode:?} index {index}");
            }
        }
    }

    fn add(battle: &mut Battle, id: u32, successes: u32) -> CombatantId {
        let cid = CombatantId(id);
        apply(
            battle,
            &BattleEvent::AddCombatant {
                id: cid,
                name: format!("C{id}"),
                side: Side("A".to_string()),
                join_battle: JoinBattleResult::Successes(successes),
            },
        )
        .unwrap();
        cid
    }

    #[test]
    fn no_actor_is_selectable_during_setup() {
        let mut battle = Battle::genesis();
        add(&mut battle, 1, 5);
        assert_eq!(actors_up_now(&battle), Vec::new());
        assert_eq!(selectable_actors(&battle), Vec::new());
        assert_eq!(rail_target(&battle, None), None);
    }

    #[test]
    fn selectable_actors_excludes_an_actor_whose_tick_has_not_arrived() {
        let mut battle = Battle::genesis();
        let fast = add(&mut battle, 1, 5);
        let slow = add(&mut battle, 2, 0);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        assert_eq!(selectable_actors(&battle), vec![fast]);
        assert!(!selectable_actors(&battle).contains(&slow));
    }

    #[test]
    fn selectable_actors_excludes_an_inactive_combatant() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();
        let inactive = template(BattleMode::Personal, ActionKind::Inactive).unwrap().declare(Declaration::default());
        apply(&mut battle, &BattleEvent::DeclareAction { actor: cid, action: inactive }).unwrap();

        assert_eq!(selectable_actors(&battle), Vec::new());
    }

    #[test]
    fn selectable_actors_excludes_a_shaping_sorcerer_though_they_stay_up_now() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();
        // `shape_terrestrial`'s one shaping step is Speed 5, so its `next_action_tick` jumps to 5
        // and she leaves `actors_up_now` immediately, same as any other Speed-5 action. Advancing
        // to tick 5 without calling `AdvanceSequence` brings her tick due again while she's still
        // `InSequence` — the case `ActorRow`'s "Up now" / "Shaping" split exists for.
        apply(&mut battle, &BattleEvent::StartSequence { actor: cid, sequence: Sequence::shape_terrestrial() }).unwrap();
        for _ in 0..5 {
            apply(&mut battle, &BattleEvent::AdvanceTick).unwrap();
        }

        // `ActorRow` lists her under "Up now" once her tick is due, even mid-sequence — it's
        // `SequenceControls`, not the action dropdown, that leaves a rail pick with nowhere to
        // land for her.
        assert_eq!(actors_up_now(&battle), vec![cid]);
        assert_eq!(selectable_actors(&battle), Vec::new());
    }

    #[test]
    fn rail_target_defaults_to_the_topmost_selectable_actor() {
        let mut battle = Battle::genesis();
        let first = add(&mut battle, 1, 5);
        add(&mut battle, 2, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        assert_eq!(rail_target(&battle, None), Some(first));
    }

    #[test]
    fn rail_target_keeps_the_last_touched_actor() {
        let mut battle = Battle::genesis();
        let first = add(&mut battle, 1, 5);
        let second = add(&mut battle, 2, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();

        assert_eq!(rail_target(&battle, Some(second)), Some(second));
        assert_eq!(rail_target(&battle, Some(first)), Some(first));
    }

    #[test]
    fn rail_target_falls_back_when_the_touched_actor_stops_being_selectable() {
        let mut battle = Battle::genesis();
        let first = add(&mut battle, 1, 5);
        let second = add(&mut battle, 2, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();
        apply(&mut battle, &BattleEvent::StartSequence { actor: second, sequence: Sequence::shape_terrestrial() }).unwrap();

        assert_eq!(rail_target(&battle, Some(second)), Some(first));
    }

    #[test]
    fn rail_target_is_none_when_nobody_is_selectable() {
        let mut battle = Battle::genesis();
        let cid = add(&mut battle, 1, 5);
        apply(&mut battle, &BattleEvent::StartBattle).unwrap();
        apply(&mut battle, &BattleEvent::StartSequence { actor: cid, sequence: Sequence::shape_terrestrial() }).unwrap();

        assert_eq!(rail_target(&battle, Some(cid)), None);
    }
}

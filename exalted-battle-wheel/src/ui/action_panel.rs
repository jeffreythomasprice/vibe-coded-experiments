use crate::ui::glossary::{action_topic, sequence_topic, Topic};
use crate::ui::{DetailTip, Tip};
use exalted_battle_wheel::battle::{
    ActionTemplate, Battle, BattleEvent, BattleLog, CombatantId, CombatantState, DvPenaltySpec, InterruptReason,
    JoinBattleResult, Phase, Sequence, SequenceTemplate, SpeedSpec, Tick, CATALOG, SEQUENCE_CATALOG,
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

#[derive(Clone, Copy)]
enum Choice {
    Action(&'static ActionTemplate),
    Sequence(&'static SequenceTemplate),
}

/// `CATALOG` occupies indices `0..CATALOG.len()`; `SEQUENCE_CATALOG` follows immediately after.
fn choice_at(index: usize) -> Option<Choice> {
    if let Some(template) = CATALOG.get(index) {
        return Some(Choice::Action(template));
    }
    SEQUENCE_CATALOG.get(index - CATALOG.len()).map(Choice::Sequence)
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

#[component]
fn NormalControls(actor_id: CombatantId, log: RwSignal<BattleLog>, battle: Memo<Battle>) -> impl IntoView {
    let speed_override = RwSignal::new(String::new());
    let dv_override = RwSignal::new(String::new());
    let selected_kind = RwSignal::new(0usize);

    let declare_topic = Signal::derive(move || match choice_at(selected_kind.get()) {
        Some(Choice::Sequence(_)) => Topic::DeclareSequence,
        _ => Topic::Declare,
    });
    let declare_detail = Signal::derive(move || match choice_at(selected_kind.get()) {
        Some(Choice::Sequence(template)) => sequence_timing(&template.build(), battle.read().current_tick),
        _ => String::new(),
    });

    let declare = move |_| match choice_at(selected_kind.get()) {
        Some(Choice::Action(template)) => {
            let speed = speed_override.get().parse().ok();
            let dv = dv_override.get().parse().ok();
            let action = template.declare(speed, dv, None, String::new());
            log.update(|log| {
                if let Err(error) = log.push(BattleEvent::DeclareAction { actor: actor_id, action }) {
                    tracing::warn!(%error, "could not declare action");
                }
            });
        }
        Some(Choice::Sequence(template)) => {
            let sequence = template.build();
            log.update(|log| {
                if let Err(error) = log.push(BattleEvent::StartSequence { actor: actor_id, sequence }) {
                    tracing::warn!(%error, "could not start sequence");
                }
            });
        }
        None => {}
    };

    view! {
        <Tip topic=Topic::ActionSelect>
            <select on:change=move |ev| {
                selected_kind.set(event_target_value(&ev).parse().unwrap_or(0));
                speed_override.set(String::new());
                dv_override.set(String::new());
            }>
                {CATALOG.iter().enumerate().map(|(i, template)| view! {
                    <option value=i.to_string()>{template.name}</option>
                }).collect_view()}
                <optgroup label="Sorcery">
                    {SEQUENCE_CATALOG.iter().enumerate().map(|(i, template)| view! {
                        <option value=(CATALOG.len() + i).to_string()>{template.name}</option>
                    }).collect_view()}
                </optgroup>
            </select>
        </Tip>
        {move || {
            choice_at(selected_kind.get()).map(|choice| match choice {
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
            })
        }}
        {move || {
            matches!(choice_at(selected_kind.get()), Some(Choice::Action(_))).then(|| view! {
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
            })
        }}
        <DetailTip topic=declare_topic detail=declare_detail>
            <button on:click=declare>"Declare"</button>
        </DetailTip>
    }
}

#[component]
fn SequenceControls(actor_id: CombatantId, log: RwSignal<BattleLog>, battle: Memo<Battle>) -> impl IntoView {
    let speed_override = RwSignal::new(String::new());

    let advance = move |_| {
        let speed = speed_override.get().parse().ok();
        log.update(|log| {
            if let Err(error) = log.push(BattleEvent::AdvanceSequence { actor: actor_id, speed_override: speed }) {
                tracing::warn!(%error, "could not advance sequence");
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
                tracing::warn!(%error, "could not interrupt sequence");
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
                tracing::warn!(%error, "could not interrupt sequence");
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
    fn choice_at_covers_the_catalog_then_the_sequence_catalog() {
        assert!(matches!(choice_at(0), Some(Choice::Action(_))));
        assert!(matches!(choice_at(CATALOG.len() - 1), Some(Choice::Action(_))));
        assert!(matches!(choice_at(CATALOG.len()), Some(Choice::Sequence(_))));
        assert!(matches!(
            choice_at(CATALOG.len() + SEQUENCE_CATALOG.len() - 1),
            Some(Choice::Sequence(_))
        ));
        assert!(choice_at(CATALOG.len() + SEQUENCE_CATALOG.len()).is_none());
    }

    #[test]
    fn choice_at_sequence_entries_match_their_catalog_order() {
        let Some(Choice::Sequence(template)) = choice_at(CATALOG.len()) else { panic!("expected a sequence") };
        assert_eq!(template.kind, SequenceKind::ShapeTerrestrial);
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

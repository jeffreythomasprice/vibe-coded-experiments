//! Reading and continuing one conversation: a bubble per content block, each
//! kind formatted per its own rule, followed by a composer.

use std::collections::HashMap;
use std::collections::HashSet;

use leptos::prelude::*;
use shared::agent::{DecidedBy, Decision, ToolDecision};
use shared::conversation::ConversationView as ConversationViewDto;
use shared::ids::ConversationId;
use shared::llm::image::MediaType;
use shared::llm::message::ToolResultContent;
use wasm_bindgen_futures::spawn_local;

use crate::commands;
use crate::composer;
use crate::markdown;
use crate::runs::{Phase, Runs};
use crate::spinner::{LoadingPanel, Spinner};
use crate::transcript::{self, Bubble, BubbleDecision, Rendered, ToolOutcome};

#[component]
pub fn Conversation(id: ConversationId) -> impl IntoView {
    let reload = use_context::<RwSignal<u32>>().expect("reload counter context is provided by App");
    let runs = use_context::<Runs>().expect("Runs context is provided by App");
    // Fixed for this component's whole lifetime — see `Runs::state`'s doc on
    // why this must be read exactly once, here, rather than re-derived
    // inside a closure.
    let run = runs.state(id);

    let view = RwSignal::new(None::<ConversationViewDto>);
    let error = RwSignal::new(None::<String>);
    let message = RwSignal::new(String::new());
    // Only the very first load — an "error and no view" state after that
    // still shows the error (see the render below), but doesn't fall back to
    // the loading panel, the same distinction `views::AllConversations`
    // draws between its own `loading` flag and an empty list.
    let loading = RwSignal::new(true);

    let load = move || {
        spawn_local(async move {
            match commands::get_conversation(id).await {
                Ok(loaded) => {
                    view.set(Some(loaded));
                    error.set(None);
                    // The refetch just landed — if nothing is running, this
                    // is the point where a just-finished draft's text is
                    // superseded by what `loaded.messages` now carries.
                    runs.forget(id);
                }
                Err(err) => error.set(Some(err.message)),
            }
            loading.set(false);
        });
    };

    // Runs once for this mount (a different conversation id means the parent
    // swapped in a whole new `Conversation` component, not a prop update on
    // this one) and again whenever something elsewhere bumps the reload
    // counter — including this conversation's own run settling, see
    // `runs::Runs::follow`.
    Effect::new(move |_| {
        reload.get();
        load();
    });

    // Runs exactly once per mount, deliberately not tied to `reload`: this is
    // the probe that resumes following a run left in flight by a previous
    // mount of this same conversation (switched away and back); a no-op if
    // nothing is running, or if this conversation is already being followed.
    // Tying it to `reload` too would re-probe every time this same
    // conversation's own run settles and bumps the counter, racing the
    // `load` above for `Runs::forget` — see that fn's doc.
    Effect::new(move |ran: Option<()>| {
        if ran.is_none() {
            runs.ensure_following(id);
        }
    });

    // Only recomputed when the persisted view changes — not on every
    // streaming delta, which only touches `run`'s draft.
    let persisted_bubbles = Memo::new(move |_| {
        view.get()
            .map(|v| transcript::flatten(&v.messages, &v.decisions, &HashSet::new()))
            .unwrap_or_default()
    });

    // The decisions gathered so far for the *current* pending turn, keyed by
    // `tool_use_id`. Reset whenever the pending turn itself changes — a
    // fresh suspend, a different turn, or none at all — never partially, so
    // a decision can't survive from one turn into the next (`tool_use_id`s
    // recur across turns; see `transcript::flatten`'s doc). Done in an
    // `Effect`, not inline in the render closure below, so clearing the map
    // is never itself a render-time side effect on a signal the same render
    // also reads.
    let decisions_map = RwSignal::new(HashMap::<String, Decision>::new());
    let pending_turn_id = Memo::new(move |_| view.get().and_then(|v| v.pending.map(|p| p.turn_id)));
    Effect::new(move |_| {
        pending_turn_id.get();
        decisions_map.set(HashMap::new());
    });

    // Records one gated call's decision and, once every pending call in this
    // turn has one, submits them all at once. No separate "already
    // submitted" guard is needed: `runs.approve` flips this conversation's
    // `run` phase to `Attaching` synchronously, and the render below
    // suppresses the whole pending section — buttons included — the instant
    // it's busy, before Leptos can even schedule another render off a
    // further click.
    let on_decide = Callback::new(move |(tool_use_id, decision): (String, Decision)| {
        decisions_map.update(|m| {
            m.insert(tool_use_id, decision);
        });
        let Some(pending) = view.get_untracked().and_then(|v| v.pending) else {
            return;
        };
        let ready = decisions_map.with_untracked(|m| pending.requests.iter().all(|r| m.contains_key(&r.tool_use_id)));
        if ready {
            let decisions: Vec<ToolDecision> = decisions_map
                .get_untracked()
                .into_iter()
                .map(|(tool_use_id, decision)| ToolDecision { tool_use_id, decision })
                .collect();
            runs.approve(id, decisions);
        }
    });

    // `run` is `ArcRwSignal`, not `Copy` (see `Runs::state`'s doc), so every
    // closure that needs it below gets its own clone rather than sharing the
    // one captured here — cloning an `ArcRwSignal` is cheap (it's just two
    // `Arc`s), and `submit` owning its own copy means it doesn't compete with
    // `view!`'s own uses of `run` for which closure gets to consume it.
    let submit = {
        let run = run.clone();
        move |_| {
            // `!= Idle`, not `!is_busy()`: the composer stays enabled during a
            // `Probing` mount (see `run_state.phase.is_busy()` below), but a
            // submit landing in that one-round-trip window is simply dropped
            // here rather than risking two concurrent followers on the same
            // conversation.
            if run.get_untracked().phase != Phase::Idle {
                return;
            }
            let text = message.get();
            if text.trim().is_empty() {
                return;
            }
            message.set(String::new());
            error.set(None);
            runs.send(id, text);
        }
    };

    view! {
        <div class="conversation-view">
            {move || match view.get() {
                // Distinguishing on `loading` (rather than showing this
                // whenever there's no view yet) is what lets a failed fetch
                // report its error instead of spinning forever — see
                // `loading`'s doc.
                None if loading.get() => view! { <LoadingPanel label="Loading conversation…" /> }.into_any(),
                None => view! { <p class="error">{move || error.get().unwrap_or_default()}</p> }.into_any(),
                Some(loaded) => {
                    let run_state = run.get();
                    // `Probing` is deliberately not "busy" — see `Phase::is_busy`'s
                    // doc — so reopening an already-settled conversation
                    // doesn't flash a "Responding…" spinner and a disabled
                    // composer for the one round trip that finds nothing
                    // running.
                    let busy = run_state.phase.is_busy();
                    let mut bubbles = persisted_bubbles.get();
                    if !run_state.draft.is_empty() {
                        bubbles.extend(run_state.draft.bubbles());
                    }
                    let indexed_bubbles: Vec<(usize, Rendered)> = bubbles.into_iter().enumerate().collect();
                    // Suppressed while busy: once `on_decide` submits, the
                    // live draft (already folded into `bubbles` above) is
                    // authoritative for this window — rendering the
                    // persisted pending block alongside it would duplicate
                    // the just-approved calls and show them out of order,
                    // and its buttons must vanish anyway so a further click
                    // can't resubmit.
                    let pending = loaded.pending.as_ref().filter(|_| !busy);
                    let pending_indexed: Option<Vec<(usize, Rendered)>> = pending
                        .map(|p| transcript::flatten_pending(p, &loaded.decisions).into_iter().enumerate().collect());
                    let decided_count = decisions_map.get().len();
                    let total_requests = pending.map(|p| p.requests.len()).unwrap_or(0);
                    let title = loaded
                        .summary
                        .title
                        .clone()
                        .unwrap_or_else(|| loaded.summary.agent_name.clone());
                    let has_pending = loaded.pending.is_some();
                    let composer_disabled = busy || has_pending;
                    view! {
                        <div class="conversation-body">
                            <h1>{title}</h1>
                            <div class="transcript">
                                <For each=move || indexed_bubbles.clone() key=|(index, _)| *index let:item>
                                    <BubbleView rendered=item.1 />
                                </For>
                            </div>
                            {busy.then(|| view! { <Spinner label="Responding…" /> })}
                            {pending_indexed
                                .map(|indexed| {
                                    view! {
                                        <div class="pending-approval">
                                            <p class="notice">
                                                {if total_requests > 1 {
                                                    format!("Waiting on your decision — {decided_count} of {total_requests} decided.")
                                                } else {
                                                    "Waiting on your decision.".to_string()
                                                }}
                                            </p>
                                            <For each=move || indexed.clone() key=|(index, _)| *index let:item>
                                                <BubbleView rendered=item.1 on_decide=on_decide />
                                            </For>
                                        </div>
                                    }
                                })}
                            <form
                                class="composer"
                                on:submit={
                                    // Cloned for the same reason `run` is
                                    // cloned below: this whole arm — and so
                                    // this attribute closure — is rebuilt on
                                    // every reactive re-render, and `submit`
                                    // (capturing a non-`Copy` `ArcRwSignal`)
                                    // is only `Clone`, not `Copy`.
                                    let submit = submit.clone();
                                    move |ev| {
                                        ev.prevent_default();
                                        submit(());
                                    }
                                }
                            >
                                <fieldset disabled=composer_disabled>
                                    <textarea
                                        placeholder="Message…"
                                        prop:value=move || message.get()
                                        on:input=move |ev| message.set(event_target_value(&ev))
                                        on:keydown={
                                            // Cloned for the same reason the
                                            // sibling `on:submit` above
                                            // clones: this arm is rebuilt on
                                            // every reactive re-render, and
                                            // `submit` captures a non-`Copy`
                                            // `ArcRwSignal`.
                                            let submit = submit.clone();
                                            move |ev| composer::keydown(ev, message, || submit(()))
                                        }
                                    ></textarea>
                                    {move || error.get().map(|m| view! { <p class="error">{m}</p> })}
                                    {
                                        // Cloned, not moved: this whole
                                        // `Some(loaded)` arm re-runs on every
                                        // reactive update, reconstructing this
                                        // closure afresh each time — moving
                                        // the outer `run` into it directly
                                        // would consume the copy the *next*
                                        // re-run needs, since `ArcRwSignal`
                                        // isn't `Copy`.
                                        let run = run.clone();
                                        move || run.get().error.map(|m| view! { <p class="error">{m}</p> })
                                    }
                                    <button type="submit" disabled=move || message.get().trim().is_empty()>
                                        <Show when=move || busy fallback=|| "Send">
                                            <Spinner label="Sending…" />
                                        </Show>
                                    </button>
                                </fieldset>
                            </form>
                        </div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

#[component]
fn BubbleView(
    rendered: Rendered,
    /// Only ever `Some` for a bubble rendered from `PendingApproval::requests`
    /// (see `views::conversation::Conversation`'s own `pending_indexed`) —
    /// every other call site passes `None`, and a bubble with `awaiting:
    /// false` never renders the controls even when it's `Some`.
    #[prop(optional)]
    on_decide: Option<Callback<(String, Decision)>>,
) -> impl IntoView {
    let timestamp = rendered.timestamp.clone();
    let timestamp_label = timestamp.unwrap_or_else(|| "streaming…".to_string());

    match rendered.bubble {
        Bubble::Human { text } => {
            let html = markdown::render_with_breaks(&text);
            view! {
                <div class="bubble bubble-human">
                    <div class="bubble-timestamp">{timestamp_label}</div>
                    <div class="bubble-markdown" inner_html=html></div>
                </div>
            }
                .into_any()
        }
        Bubble::Assistant { markdown: source } => {
            let html = markdown::render(&source);
            view! {
                <div class="bubble bubble-assistant">
                    <div class="bubble-timestamp">{timestamp_label}</div>
                    <div class="bubble-markdown" inner_html=html></div>
                </div>
            }
                .into_any()
        }
        Bubble::Thinking { text, redacted } => {
            let body = if redacted { "(redacted by the provider)".to_string() } else { text };
            view! {
                <details class="bubble bubble-thinking">
                    <summary>
                        <span class="bubble-timestamp">{timestamp_label}</span>
                        " Thinking"
                    </summary>
                    <div class="bubble-text">{body}</div>
                </details>
            }
                .into_any()
        }
        Bubble::Tool { tool_use_id, name, input, result, decision, awaiting } => {
            let summary_text = tool_summary(&name, &input, result.as_ref(), awaiting);
            let input_pretty = serde_json::to_string_pretty(&input).unwrap_or_default();
            let result_section = result.map(|outcome| {
                let label = if outcome.is_error { "Error" } else { "Result" };
                let text = tool_result_text(&outcome);
                view! {
                    <h4>{label}</h4>
                    <pre>{text}</pre>
                }
            });
            let decision_line = decision.map(|d| view! { <p class="tool-decision">{decision_provenance_text(&d)}</p> });
            let approval_controls = awaiting.then(|| on_decide).flatten().map(|on_decide| {
                let approve_id = tool_use_id.clone();
                let deny_id = tool_use_id.clone();
                view! {
                    <div class="tool-approval">
                        <button
                            type="button"
                            class="approve"
                            on:click=move |_| on_decide.run((approve_id.clone(), Decision::Approve))
                        >
                            "Approve"
                        </button>
                        <button
                            type="button"
                            class="deny"
                            on:click=move |_| on_decide.run((deny_id.clone(), Decision::Deny { reason: None }))
                        >
                            "Deny"
                        </button>
                    </div>
                }
            });
            view! {
                <details class="bubble bubble-tool" class:bubble-tool-pending=awaiting open=awaiting>
                    <summary>
                        <span class="bubble-timestamp">{timestamp_label}</span>
                        {summary_text}
                    </summary>
                    <div class="tool-detail">
                        <h4>"Input"</h4>
                        <pre>{input_pretty}</pre>
                        {result_section}
                        {decision_line}
                        {approval_controls}
                    </div>
                </details>
            }
                .into_any()
        }
        Bubble::Image { source } => {
            let src = format!("data:{};base64,{}", media_type_str(&source.media_type), source.data);
            view! {
                <div class="bubble bubble-image">
                    <div class="bubble-timestamp">{timestamp_label}</div>
                    <img src=src />
                </div>
            }
                .into_any()
        }
    }
}

/// The always-visible line for a tool bubble — full input/result detail lives
/// in the `<details>` body around it.
fn tool_summary(name: &str, input: &serde_json::Value, result: Option<&ToolOutcome>, awaiting: bool) -> String {
    let compact = serde_json::to_string(input).unwrap_or_default();
    let preview: String = compact.chars().take(80).collect();
    let preview = if compact.chars().count() > 80 { format!("{preview}…") } else { preview };
    match (awaiting, result) {
        (true, _) => format!("{name}({preview}) — awaiting your approval"),
        (false, None) => format!("{name}({preview}) — running…"),
        (false, Some(outcome)) if outcome.is_error => format!("{name}({preview}) — error"),
        (false, Some(_)) => format!("{name}({preview}) — done"),
    }
}

/// A short, human-readable line for a settled gated call: who resolved it,
/// and when, in the vocabulary the approval history requirement asks for —
/// "Approved by you · 14:32:07" / "Denied automatically: <reason>".
fn decision_provenance_text(decision: &BubbleDecision) -> String {
    let verb = match decision.decision {
        Decision::Approve => "Approved",
        Decision::Deny { .. } => "Denied",
    };
    let body = match &decision.decided_by {
        Some(DecidedBy::User) => "by you".to_string(),
        Some(DecidedBy::Policy { reason }) => format!("automatically: {reason}"),
        // The decision is recorded (see `shared::conversation::ToolDecisionView`'s
        // doc), but provenance hasn't settled yet — a brief, self-healing gap
        // right after answering, before the turn is driven forward again.
        None => String::new(),
    };
    let when = decision.decided_at.as_deref().map(|t| format!(" · {t}")).unwrap_or_default();
    format!("{verb} {body}{when}").trim().to_string()
}

fn tool_result_text(outcome: &ToolOutcome) -> String {
    outcome
        .content
        .iter()
        .map(|block| match block {
            ToolResultContent::Text { text } => text.clone(),
            ToolResultContent::Image { .. } => "[image]".to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn media_type_str(media_type: &MediaType) -> &str {
    match media_type {
        MediaType::Png => "image/png",
        MediaType::Jpeg => "image/jpeg",
        MediaType::Webp => "image/webp",
        MediaType::Gif => "image/gif",
        MediaType::Other(other) => other.as_str(),
    }
}

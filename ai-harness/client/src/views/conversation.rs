//! Reading and continuing one conversation: a bubble per content block, each
//! kind formatted per its own rule, followed by a composer.

use leptos::prelude::*;
use shared::conversation::{ConversationView as ConversationViewDto, StoredMessage};
use shared::ids::{ConversationId, MessageId};
use shared::llm::image::MediaType;
use shared::llm::message::ToolResultContent;
use wasm_bindgen_futures::spawn_local;

use crate::commands;
use crate::markdown;
use crate::transcript::{self, Bubble, Draft, Rendered, ToolOutcome};

#[component]
pub fn Conversation(id: ConversationId) -> impl IntoView {
    let reload = use_context::<RwSignal<u32>>().expect("reload counter context is provided by App");

    let view = RwSignal::new(None::<ConversationViewDto>);
    let error = RwSignal::new(None::<String>);
    let draft = RwSignal::new(Draft::new());
    let sending = RwSignal::new(false);
    let message = RwSignal::new(String::new());

    let load = move || {
        spawn_local(async move {
            match commands::get_conversation(id).await {
                Ok(loaded) => {
                    view.set(Some(loaded));
                    error.set(None);
                }
                Err(err) => error.set(Some(err.message)),
            }
        });
    };

    // Runs once for this mount (a different conversation id means the parent
    // swapped in a whole new `Conversation` component, not a prop update on
    // this one) and again whenever something elsewhere bumps the reload
    // counter.
    Effect::new(move |_| {
        reload.get();
        load();
    });

    // Only recomputed when the persisted view changes — not on every
    // streaming delta, which only touches `draft`.
    let persisted_bubbles = Memo::new(move |_| {
        view.get().map(|v| transcript::flatten(&v.messages)).unwrap_or_default()
    });

    let submit = move |_| {
        if sending.get() {
            return;
        }
        let text = message.get();
        if text.trim().is_empty() {
            return;
        }
        message.set(String::new());
        error.set(None);
        sending.set(true);
        draft.set(Draft::new());
        spawn_local(async move {
            let outcome = commands::send_message(id, text, move |event| {
                draft.update(|d| d.apply(&event));
            })
            .await;
            sending.set(false);
            draft.set(Draft::new());
            match outcome {
                Ok(_) => load(),
                Err(err) => error.set(Some(err.message)),
            }
        });
    };

    view! {
        <div class="conversation-view">
            {move || match view.get() {
                None => view! { <p class="loading">"Loading…"</p> }.into_any(),
                Some(loaded) => {
                    let mut bubbles = persisted_bubbles.get();
                    let in_flight = draft.get();
                    if !in_flight.is_empty() {
                        bubbles.extend(in_flight.bubbles());
                    }
                    let indexed_bubbles: Vec<(usize, Rendered)> = bubbles.into_iter().enumerate().collect();
                    let pending_indexed: Option<Vec<(usize, Rendered)>> = loaded
                        .pending
                        .as_ref()
                        .map(|p| flatten_pending(&p.added).into_iter().enumerate().collect());
                    let title = loaded
                        .summary
                        .title
                        .clone()
                        .unwrap_or_else(|| loaded.summary.agent_name.clone());
                    let has_pending = loaded.pending.is_some();
                    view! {
                        <div class="conversation-body">
                            <h1>{title}</h1>
                            <div class="transcript">
                                <For each=move || indexed_bubbles.clone() key=|(index, _)| *index let:item>
                                    <BubbleView rendered=item.1 />
                                </For>
                            </div>
                            {pending_indexed
                                .map(|indexed| {
                                    view! {
                                        <div class="pending-approval">
                                            <p class="notice">
                                                "A tool call here is awaiting approval — this UI doesn't support approving it yet."
                                            </p>
                                            <For each=move || indexed.clone() key=|(index, _)| *index let:item>
                                                <BubbleView rendered=item.1 />
                                            </For>
                                        </div>
                                    }
                                })}
                            <form
                                class="composer"
                                on:submit=move |ev| {
                                    ev.prevent_default();
                                    submit(());
                                }
                            >
                                <textarea
                                    placeholder="Message…"
                                    prop:value=move || message.get()
                                    on:input=move |ev| message.set(event_target_value(&ev))
                                    disabled=move || sending.get() || has_pending
                                ></textarea>
                                {move || error.get().map(|m| view! { <p class="error">{m}</p> })}
                                <button
                                    type="submit"
                                    disabled=move || {
                                        sending.get() || has_pending || message.get().trim().is_empty()
                                    }
                                >
                                    {move || if sending.get() { "Sending…" } else { "Send" }}
                                </button>
                            </form>
                        </div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

/// Wraps a suspended turn's un-persisted `added` messages into throwaway
/// `StoredMessage`s so [`transcript::flatten`] can render them too — they
/// have no row (and so no real id/seq/timestamp) until the turn settles.
fn flatten_pending(added: &[shared::llm::message::Message]) -> Vec<Rendered> {
    let fake: Vec<StoredMessage> = added
        .iter()
        .enumerate()
        .map(|(index, message)| StoredMessage {
            id: MessageId(0),
            seq: index as u32,
            turn_id: None,
            role: message.role,
            content: message.content.clone(),
            created_at: String::new(),
        })
        .collect();
    transcript::flatten(&fake)
        .into_iter()
        .map(|mut rendered| {
            // These never had a real timestamp — show them the same way a
            // still-streaming bubble is shown, rather than a blank string.
            rendered.timestamp = None;
            rendered
        })
        .collect()
}

#[component]
fn BubbleView(rendered: Rendered) -> impl IntoView {
    let timestamp = rendered.timestamp.clone();
    let timestamp_label = timestamp.unwrap_or_else(|| "streaming…".to_string());

    match rendered.bubble {
        Bubble::Human { text } => view! {
            <div class="bubble bubble-human">
                <div class="bubble-timestamp">{timestamp_label}</div>
                <div class="bubble-text">{text}</div>
            </div>
        }
            .into_any(),
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
        Bubble::Tool { name, input, result } => {
            let summary_text = tool_summary(&name, &input, result.as_ref());
            let input_pretty = serde_json::to_string_pretty(&input).unwrap_or_default();
            let result_section = result.map(|outcome| {
                let label = if outcome.is_error { "Error" } else { "Result" };
                let text = tool_result_text(&outcome);
                view! {
                    <h4>{label}</h4>
                    <pre>{text}</pre>
                }
            });
            view! {
                <details class="bubble bubble-tool">
                    <summary>
                        <span class="bubble-timestamp">{timestamp_label}</span>
                        {summary_text}
                    </summary>
                    <div class="tool-detail">
                        <h4>"Input"</h4>
                        <pre>{input_pretty}</pre>
                        {result_section}
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
fn tool_summary(name: &str, input: &serde_json::Value, result: Option<&ToolOutcome>) -> String {
    let compact = serde_json::to_string(input).unwrap_or_default();
    let preview: String = compact.chars().take(80).collect();
    let preview = if compact.chars().count() > 80 { format!("{preview}…") } else { preview };
    match result {
        None => format!("{name}({preview}) — running…"),
        Some(outcome) if outcome.is_error => format!("{name}({preview}) — error"),
        Some(_) => format!("{name}({preview}) — done"),
    }
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

//! The Agents view: every saved agent, each with a Delete button, plus a New
//! button that opens the same [`AgentForm`] used inline from New Conversation.

use leptos::prelude::*;
use shared::agent::AgentConfig;
use shared::ids::AgentConfigId;
use wasm_bindgen_futures::spawn_local;

use crate::commands;
use crate::spinner::LoadingPanel;
use crate::views::AgentForm;

#[component]
pub fn Agents() -> impl IntoView {
    let agents = RwSignal::new(Vec::<AgentConfig>::new());
    let error = RwSignal::new(None::<String>);
    let creating = RwSignal::new(false);
    let confirming_delete = RwSignal::new(None::<AgentConfigId>);
    let refresh = RwSignal::new(0u32);
    // Only the very first load — an empty list after that is a real "no
    // agents yet" state, not a wait.
    let loading = RwSignal::new(true);

    Effect::new(move |_| {
        refresh.get();
        spawn_local(async move {
            match commands::list_agents().await {
                Ok(list) => {
                    agents.set(list);
                    error.set(None);
                }
                Err(err) => error.set(Some(err.message)),
            }
            loading.set(false);
        });
    });

    let delete = move |id: AgentConfigId| {
        spawn_local(async move {
            match commands::delete_agent(id).await {
                Ok(()) => refresh.update(|n| *n += 1),
                Err(err) => error.set(Some(err.message)),
            }
        });
    };

    view! {
        <div class="agents-view">
            <h1>"Agents"</h1>
            {move || {
                if loading.get() {
                    view! { <LoadingPanel label="Loading agents…" /> }.into_any()
                } else {
                    view! {
                        {move || error.get().map(|message| view! { <p class="error">{message}</p> })}
                        <ul class="agent-list">
                            <For each=move || agents.get() key=|a| a.id.get() let:agent>
                                <AgentRow agent=agent confirming_delete=confirming_delete on_delete=delete />
                            </For>
                        </ul>
                        {move || {
                            if creating.get() {
                                view! {
                                    <AgentForm
                                        on_saved=move |_agent| {
                                            creating.set(false);
                                            refresh.update(|n| *n += 1);
                                        }
                                        on_cancel=move |_| creating.set(false)
                                    />
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <button class="new-agent-button" on:click=move |_| creating.set(true)>
                                        "New"
                                    </button>
                                }
                                    .into_any()
                            }
                        }}
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

#[component]
fn AgentRow(
    agent: AgentConfig,
    confirming_delete: RwSignal<Option<AgentConfigId>>,
    on_delete: impl Fn(AgentConfigId) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let id = agent.id;
    let name = agent.input.name.clone();
    let model = format!("{} / {}", agent.input.model.provider, agent.input.model.model);

    view! {
        <li class="agent-row">
            <div class="agent-row-info">
                <strong>{name}</strong>
                <span class="muted">{model}</span>
            </div>
            {move || {
                if confirming_delete.get() == Some(id) {
                    view! {
                        <span class="confirm-delete">
                            "Delete this agent?"
                            <button on:click=move |_| {
                                confirming_delete.set(None);
                                on_delete(id);
                            }>"Yes"</button>
                            <button on:click=move |_| confirming_delete.set(None)>"No"</button>
                        </span>
                    }
                        .into_any()
                } else {
                    view! {
                        <button on:click=move |_| confirming_delete.set(Some(id))>"Delete"</button>
                    }
                        .into_any()
                }
            }}
        </li>
    }
}

//! The "New" flow: pick (or create) an agent, write the first message, send.
//! Once sent, the agent and conversation are locked in — this view hands off
//! to [`crate::views::Conversation`] and never comes back.

use leptos::prelude::*;
use shared::agent::AgentConfig;
use shared::ids::AgentConfigId;
use wasm_bindgen_futures::spawn_local;

use crate::app::Route;
use crate::commands;
use crate::runs::Runs;
use crate::spinner::BusyOverlay;
use crate::views::AgentForm;

#[component]
pub fn NewConversation() -> impl IntoView {
    let route = use_context::<RwSignal<Route>>().expect("Route context is provided by App");
    let reload = use_context::<RwSignal<u32>>().expect("reload counter context is provided by App");
    let runs = use_context::<Runs>().expect("Runs context is provided by App");

    let agents = RwSignal::new(Vec::<AgentConfig>::new());
    let selected_agent = RwSignal::new(None::<AgentConfigId>);
    let creating_agent = RwSignal::new(false);
    let message = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);
    // Only covers the brief create-conversation + start-message handoff
    // below, not the turn itself — once accepted, the first turn streams in
    // `views::Conversation`, which this view routes to right away.
    let sending = RwSignal::new(false);

    let load_agents = move || {
        spawn_local(async move {
            if let Ok(list) = commands::list_agents().await {
                if selected_agent.get_untracked().is_none() {
                    selected_agent.set(list.first().map(|a| a.id));
                }
                agents.set(list);
            }
        });
    };
    Effect::new(move |_| load_agents());

    let submit = move |_| {
        let Some(agent_id) = selected_agent.get() else {
            error.set(Some("choose an agent first".to_string()));
            return;
        };
        let text = message.get();
        if text.trim().is_empty() {
            error.set(Some("write a message first".to_string()));
            return;
        }
        error.set(None);
        sending.set(true);
        spawn_local(async move {
            let created = match commands::create_conversation(agent_id, None).await {
                Ok(created) => created,
                Err(err) => {
                    sending.set(false);
                    error.set(Some(err.message));
                    return;
                }
            };
            // The conversation row now exists regardless of what happens
            // next — bump the sidebar right away so it shows up even if the
            // send below fails.
            reload.update(|n| *n += 1);
            runs.send(created.id, text);
            sending.set(false);
            // The first turn streams into `runs`' store; land on the
            // conversation view to watch it, rather than waiting here for
            // it to finish.
            route.set(Route::Conversation(created.id));
        });
    };

    view! {
        <div class="new-conversation">
            {move || sending.get().then(|| view! { <BusyOverlay label="Starting conversation…" /> })}
            {move || {
                if creating_agent.get() {
                    view! {
                        <h1>"New agent"</h1>
                        <AgentForm
                            on_saved=move |agent: AgentConfig| {
                                creating_agent.set(false);
                                selected_agent.set(Some(agent.id));
                                load_agents();
                            }
                            on_cancel=move |_| creating_agent.set(false)
                        />
                    }
                        .into_any()
                } else {
                    view! {
                        <h1>"New conversation"</h1>

                        <fieldset disabled=move || sending.get()>
                            <label class="agent-picker">
                                <span>"Agent"</span>
                                <select
                                    prop:value=move || {
                                        selected_agent.get().map(|id| id.get().to_string()).unwrap_or_default()
                                    }
                                    on:change=move |ev| {
                                        let value = event_target_value(&ev);
                                        selected_agent.set(value.parse::<i64>().ok().map(AgentConfigId));
                                    }
                                >
                                    <For each=move || agents.get() key=|a| a.id.get() let:agent>
                                        {
                                            let id = agent.id;
                                            view! {
                                                <option
                                                    value=id.get().to_string()
                                                    selected=move || selected_agent.get() == Some(id)
                                                >
                                                    {agent.input.name.clone()}
                                                </option>
                                            }
                                        }
                                    </For>
                                </select>
                            </label>

                            <button
                                type="button"
                                class="new-agent-button"
                                on:click=move |_| {
                                    error.set(None);
                                    creating_agent.set(true);
                                }
                            >
                                "New agent"
                            </button>

                            <textarea
                                class="new-conversation-message"
                                placeholder="Say something to start the conversation…"
                                prop:value=move || message.get()
                                on:input=move |ev| message.set(event_target_value(&ev))
                            ></textarea>

                            {move || error.get().map(|message| view! { <p class="error">{message}</p> })}

                            <button on:click=submit>"Send"</button>
                        </fieldset>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

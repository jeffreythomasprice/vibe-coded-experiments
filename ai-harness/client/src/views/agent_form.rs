//! The agent-creation form, used both standalone (from the Agents view) and
//! inline (from New Conversation).

use std::collections::HashSet;

use leptos::prelude::*;
use shared::agent::{AgentConfig, AgentConfigInput, ToolSelection, ToolSpec};
use shared::llm::model::ModelRef;
use shared::llm::tool::{Effort, Thinking};
use wasm_bindgen_futures::spawn_local;

use crate::models::CatalogState;
use crate::spinner::BusyOverlay;
use crate::{commands, models};

const DEFAULT_MAX_TOKENS: &str = "4096";
const DEFAULT_MAX_STEPS: &str = "8";

#[component]
pub fn AgentForm(
    #[prop(into)] on_saved: Callback<AgentConfig>,
    #[prop(into)] on_cancel: Callback<()>,
) -> impl IntoView {
    let catalog = use_context::<RwSignal<CatalogState>>().expect("Catalog context is provided by App");

    let name = RwSignal::new(String::new());
    let provider = RwSignal::new(String::new());
    let model = RwSignal::new(String::new());
    let system_prompt = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let max_tokens = RwSignal::new(DEFAULT_MAX_TOKENS.to_string());
    let max_steps = RwSignal::new(DEFAULT_MAX_STEPS.to_string());
    let thinking_choice = RwSignal::new("off".to_string());
    let available_tools = RwSignal::new(Vec::<ToolSpec>::new());
    let selected_tools = RwSignal::new(HashSet::<String>::new());
    let error = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(tools) = commands::tool_catalog().await {
                available_tools.set(tools);
            }
        });
    });

    let provider_options = move || match catalog.get() {
        CatalogState::Ready(c) => models::provider_names(&c),
        CatalogState::Loading | CatalogState::Failed(_) => Vec::new(),
    };
    let model_options = move || match catalog.get() {
        CatalogState::Ready(c) => models::chat_model_ids(&c, &provider.get()),
        CatalogState::Loading | CatalogState::Failed(_) => Vec::new(),
    };
    // While the catalog is loading, the whole form waits — its
    // provider/model suggestions aren't ready. A *failed* fetch does not
    // count as busy: provider and model are free-text inputs, so the form
    // stays usable without suggestions (see the muted note rendered below).
    let busy = move || matches!(catalog.get(), CatalogState::Loading) || saving.get();
    let busy_label = move || {
        if saving.get() {
            "Saving agent…".to_string()
        } else {
            "Loading models…".to_string()
        }
    };

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);

        let trimmed_name = name.get().trim().to_string();
        let trimmed_provider = provider.get().trim().to_string();
        let trimmed_model = model.get().trim().to_string();
        if trimmed_name.is_empty() || trimmed_provider.is_empty() || trimmed_model.is_empty() {
            error.set(Some("name, provider, and model are all required".to_string()));
            return;
        }
        let Ok(parsed_max_tokens) = max_tokens.get().trim().parse::<u32>() else {
            error.set(Some("max tokens must be a whole number".to_string()));
            return;
        };
        let Ok(parsed_max_steps) = max_steps.get().trim().parse::<u32>() else {
            error.set(Some("max steps must be a whole number".to_string()));
            return;
        };

        let prompt_text = system_prompt.get();
        let system = if prompt_text.trim().is_empty() { Vec::new() } else { vec![prompt_text] };
        let description_text = description.get();
        let description_value = if description_text.trim().is_empty() { None } else { Some(description_text) };
        let thinking = match thinking_choice.get().as_str() {
            "low" => Thinking::Adaptive { effort: Effort::Low },
            "medium" => Thinking::Adaptive { effort: Effort::Medium },
            "high" => Thinking::Adaptive { effort: Effort::High },
            _ => Thinking::Off,
        };
        let tools: Vec<ToolSelection> =
            selected_tools.get().into_iter().map(|name| ToolSelection { name, approval: None }).collect();

        let input = AgentConfigInput {
            name: trimmed_name,
            description: description_value,
            model: ModelRef::new(trimmed_provider, trimmed_model),
            system,
            max_tokens: parsed_max_tokens,
            tools,
            tool_choice: None,
            thinking,
            stop_sequences: Vec::new(),
            max_steps: parsed_max_steps,
        };

        saving.set(true);
        spawn_local(async move {
            match commands::create_agent(input).await {
                Ok(agent) => {
                    saving.set(false);
                    on_saved.run(agent);
                }
                Err(err) => {
                    saving.set(false);
                    error.set(Some(err.message));
                }
            }
        });
    };

    view! {
        <form class="agent-form" on:submit=submit>
            {move || busy().then(|| view! { <BusyOverlay label=busy_label() /> })}
            <fieldset disabled=busy>
                <label class="agent-form-field">
                    <span>"Name"</span>
                    <input
                        type="text"
                        required=true
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </label>

                <label class="agent-form-field">
                    <span>"Provider"</span>
                    <input
                        type="text"
                        list="agent-form-providers"
                        required=true
                        prop:value=move || provider.get()
                        on:input=move |ev| provider.set(event_target_value(&ev))
                    />
                </label>
                <datalist id="agent-form-providers">
                    <For each=provider_options key=|p| p.clone() let:p>
                        <option value=p></option>
                    </For>
                </datalist>

                <label class="agent-form-field">
                    <span>"Model"</span>
                    <input
                        type="text"
                        list="agent-form-models"
                        required=true
                        prop:value=move || model.get()
                        on:input=move |ev| model.set(event_target_value(&ev))
                    />
                </label>
                <datalist id="agent-form-models">
                    <For each=model_options key=|m| m.clone() let:m>
                        <option value=m></option>
                    </For>
                </datalist>

                <label class="agent-form-field">
                    <span>"System prompt"</span>
                    <textarea
                        prop:value=move || system_prompt.get()
                        on:input=move |ev| system_prompt.set(event_target_value(&ev))
                    ></textarea>
                </label>

                <details class="agent-form-advanced">
                    <summary>"Advanced"</summary>

                    <label class="agent-form-field">
                        <span>"Description"</span>
                        <input
                            type="text"
                            prop:value=move || description.get()
                            on:input=move |ev| description.set(event_target_value(&ev))
                        />
                    </label>

                    <label class="agent-form-field">
                        <span>"Max tokens per model call"</span>
                        <input
                            type="number"
                            min="1"
                            prop:value=move || max_tokens.get()
                            on:input=move |ev| max_tokens.set(event_target_value(&ev))
                        />
                    </label>

                    <label class="agent-form-field">
                        <span>"Max steps per turn"</span>
                        <input
                            type="number"
                            min="1"
                            prop:value=move || max_steps.get()
                            on:input=move |ev| max_steps.set(event_target_value(&ev))
                        />
                    </label>

                    <label class="agent-form-field">
                        <span>"Thinking"</span>
                        <select on:change=move |ev| thinking_choice.set(event_target_value(&ev))>
                            <option value="off">"Off"</option>
                            <option value="low">"Adaptive: low"</option>
                            <option value="medium">"Adaptive: medium"</option>
                            <option value="high">"Adaptive: high"</option>
                        </select>
                    </label>

                    <div class="agent-form-tools">
                        <span>"Tools"</span>
                        {move || {
                            if available_tools.get().is_empty() {
                                view! { <p class="muted">"No tools are registered in this build."</p> }
                                    .into_any()
                            } else {
                                view! {
                                    <ul class="tool-checklist">
                                        <For
                                            each=move || available_tools.get()
                                            key=|t| t.def.name.clone()
                                            let:tool
                                        >
                                            <ToolCheckbox tool=tool selected_tools=selected_tools />
                                        </For>
                                    </ul>
                                }
                                    .into_any()
                            }
                        }}
                    </div>
                </details>

                {move || match catalog.get() {
                    CatalogState::Failed(message) => {
                        Some(
                            view! {
                                <p class="muted">
                                    {format!(
                                        "Model suggestions unavailable ({message}) — provider and model can still be typed by hand.",
                                    )}
                                </p>
                            },
                        )
                    }
                    CatalogState::Loading | CatalogState::Ready(_) => None,
                }}

                {move || error.get().map(|message| view! { <p class="error">{message}</p> })}
            </fieldset>

            <div class="agent-form-actions">
                <button type="submit" disabled=busy>
                    "Save"
                </button>
                <button type="button" on:click=move |_| on_cancel.run(())>
                    "Cancel"
                </button>
            </div>
        </form>
    }
}

#[component]
fn ToolCheckbox(tool: ToolSpec, selected_tools: RwSignal<HashSet<String>>) -> impl IntoView {
    let name = tool.def.name.clone();
    let label = tool.def.name.clone();
    view! {
        <li>
            <label>
                <input
                    type="checkbox"
                    on:change=move |ev| {
                        let checked = event_target_checked(&ev);
                        let name = name.clone();
                        selected_tools
                            .update(|set| {
                                if checked {
                                    set.insert(name);
                                } else {
                                    set.remove(&name);
                                }
                            });
                    }
                />
                {label}
            </label>
        </li>
    }
}

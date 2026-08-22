//! The model-catalog page: on mount, fetches every provider's models via the
//! `model_catalog` Tauri command (`lib::catalog::load`) and renders one
//! collapsible, initially-expanded section per provider.

use leptos::prelude::*;
use serde_wasm_bindgen::from_value;
use shared::llm::{CatalogEntry, ModelCatalog, ModelDetails, ProviderModels};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use crate::ipc::invoke;

#[component]
pub fn App() -> impl IntoView {
    let (catalog, set_catalog) = signal(None::<ModelCatalog>);
    let (load_error, set_load_error) = signal(None::<String>);

    spawn_local(async move {
        tracing::debug!("invoking model_catalog");
        match invoke("model_catalog", JsValue::UNDEFINED).await {
            Ok(value) => match from_value::<ModelCatalog>(value) {
                Ok(catalog) => set_catalog.set(Some(catalog)),
                Err(err) => {
                    tracing::error!(%err, "failed to deserialize ModelCatalog");
                    set_load_error.set(Some(format!("failed to parse model catalog: {err}")));
                }
            },
            Err(err) => {
                let message = err.as_string().unwrap_or_else(|| format!("{err:?}"));
                tracing::error!(message = %message, "model_catalog invoke failed");
                set_load_error.set(Some(format!("failed to load models: {message}")));
            }
        }
    });

    view! {
        <main>
            <h1>"ai-harness — models"</h1>
            <p class="legend">"* = pulled locally"</p>
            {move || {
                if let Some(message) = load_error.get() {
                    view! { <p class="error">{message}</p> }.into_any()
                } else if let Some(catalog) = catalog.get() {
                    view! {
                        <div class="providers">
                            <For
                                each=move || catalog.providers.clone()
                                key=|p| p.provider.clone()
                                let:provider
                            >
                                <ProviderSection provider=provider />
                            </For>
                        </div>
                    }
                        .into_any()
                } else {
                    view! { <p class="loading">"Loading models…"</p> }.into_any()
                }
            }}
        </main>
    }
}

#[component]
fn ProviderSection(provider: ProviderModels) -> impl IntoView {
    let count = provider.models.len();
    let name = provider.provider.clone();
    let errors = provider.errors.clone();
    let models = provider.models.clone();

    view! {
        <details open=true class="provider">
            <summary>{format!("{name} ({count})")}</summary>
            {(!errors.is_empty())
                .then(|| {
                    view! {
                        <ul class="errors">
                            <For each=move || errors.clone() key=|e| e.clone() let:message>
                                <li class="error">{message}</li>
                            </For>
                        </ul>
                    }
                })}
            <ul class="models">
                <For each=move || models.clone() key=|m| m.info.id.clone() let:entry>
                    <ModelRow entry=entry />
                </For>
            </ul>
        </details>
    }
}

#[component]
fn ModelRow(entry: CatalogEntry) -> impl IntoView {
    let starred = !entry.local_tags.is_empty();
    let id = entry.info.id.clone();
    let secondary = describe_details(&entry.info.details);

    view! {
        <li class="model">
            <span class="star">{if starred { "*" } else { "" }}</span>
            <code class="id">{id}</code>
            {secondary.map(|text| view! { <span class="meta">{text}</span> })}
        </li>
    }
}

/// One human-readable line summarizing a model's provider-specific details,
/// or `None` if there's nothing worth showing.
fn describe_details(details: &ModelDetails) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    match details {
        ModelDetails::Anthropic {
            max_input_tokens,
            max_output_tokens,
        } => {
            if let Some(tokens) = max_input_tokens {
                parts.push(format!("{tokens} in"));
            }
            if let Some(tokens) = max_output_tokens {
                parts.push(format!("{tokens} out"));
            }
        }
        ModelDetails::OpenAi { owned_by } => {
            parts.push(owned_by.clone());
        }
        ModelDetails::OllamaLocal {
            size_bytes,
            parameter_size,
            quantization_level,
            capabilities,
            ..
        } => {
            if let Some(size) = parameter_size {
                parts.push(size.clone());
            }
            if let Some(quant) = quantization_level {
                parts.push(quant.clone());
            }
            parts.push(format!("{:.1}GB", *size_bytes as f64 / 1e9));
            if !capabilities.is_empty() {
                parts.push(capabilities.join(", "));
            }
        }
        ModelDetails::OllamaRemote {
            description,
            capabilities,
            parameter_sizes,
            pulls,
        } => {
            if !parameter_sizes.is_empty() {
                parts.push(parameter_sizes.join(" · "));
            }
            if !capabilities.is_empty() {
                parts.push(capabilities.join(", "));
            }
            if let Some(pulls) = pulls {
                parts.push(format!("{pulls} pulls"));
            }
            if let Some(description) = description {
                parts.push(description.clone());
            }
        }
        // Not produced by `model_catalog` — a per-model tag scrape
        // (`OllamaClient::list_remote_tags`) is out of scope for this
        // catalog view — but the match needs to stay exhaustive.
        ModelDetails::OllamaRemoteTag {
            size,
            context_window,
            ..
        } => {
            if let Some(size) = size {
                parts.push(size.clone());
            }
            if let Some(context_window) = context_window {
                parts.push(context_window.clone());
            }
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

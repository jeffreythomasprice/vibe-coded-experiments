//! The model catalog: fetching it, and the lookups an agent form's
//! provider/model autocomplete fields need.
//!
//! There used to be a whole page built on this (the model-browser `App` this
//! module replaces); that page is gone, but the fetch and the filtering it
//! did are exactly what powers the agent form's dropdowns now.

use shared::error::ErrorReport;
use shared::llm::model::{CatalogEntry, ModelCatalog};

use crate::ipc;

/// The model catalog's fetch lifecycle, as `App` tracks it in context.
///
/// A bare `RwSignal<Option<ModelCatalog>>` can't tell "still loading" apart
/// from "the fetch failed" — both read as `None`. That distinction matters
/// to `views::AgentForm`: while loading, the whole form should wait (its
/// provider/model suggestions aren't ready yet); once *failed*, the form
/// must not stay disabled forever — provider and model are free-text
/// inputs, so the form is still usable without suggestions, just with a
/// note that they're unavailable.
#[derive(Debug, Clone)]
pub enum CatalogState {
    Loading,
    Ready(ModelCatalog),
    Failed(String),
}

/// Fetch every provider's models via the `model_catalog` Tauri command
/// (`lib::catalog::load`). Cheap to call once and share — `lib::catalog::load`
/// makes four sequential, disk-cached HTTP calls, so this should be fetched
/// once at app startup (see `app::App`) rather than per form open.
pub async fn fetch() -> Result<ModelCatalog, ErrorReport> {
    ipc::call0("model_catalog").await
}

/// Every provider name the catalog reports, in the order it returned them.
pub fn provider_names(catalog: &ModelCatalog) -> Vec<String> {
    catalog.providers.iter().map(|p| p.provider.clone()).collect()
}

/// Chat model ids for one provider — excludes embedding and image models,
/// neither of which an agent (a chat loop) can be built on.
///
/// For Ollama specifically, prefers ids that are actually pulled
/// (`local_tags` non-empty) over merely-pullable ones from the remote
/// library index; falls back to the full list only if nothing is pulled yet,
/// so the dropdown is never empty just because nothing's been pulled.
pub fn chat_model_ids(catalog: &ModelCatalog, provider: &str) -> Vec<String> {
    let Some(entry) = catalog.providers.iter().find(|p| p.provider == provider) else {
        return Vec::new();
    };
    let chat_models: Vec<&CatalogEntry> = entry
        .models
        .iter()
        .filter(|m| !m.info.details.is_embedding() && !m.info.details.is_image())
        .collect();

    if provider == "ollama" {
        let pulled: Vec<String> = chat_models
            .iter()
            .filter(|m| !m.local_tags.is_empty())
            .map(|m| m.info.id.clone())
            .collect();
        if !pulled.is_empty() {
            return pulled;
        }
    }

    chat_models.iter().map(|m| m.info.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::llm::model::{ModelDetails, ModelInfo, ProviderModels};

    fn entry(id: &str, details: ModelDetails, local_tags: Vec<&str>) -> CatalogEntry {
        CatalogEntry {
            info: ModelInfo {
                id: id.to_string(),
                display_name: None,
                created_at: None,
                details,
            },
            local_tags: local_tags.into_iter().map(str::to_string).collect(),
        }
    }

    fn catalog() -> ModelCatalog {
        ModelCatalog {
            providers: vec![
                ProviderModels {
                    provider: "anthropic".to_string(),
                    models: vec![entry(
                        "claude-opus-5",
                        ModelDetails::Anthropic {
                            max_input_tokens: None,
                            max_output_tokens: None,
                        },
                        vec![],
                    )],
                    errors: vec![],
                },
                ProviderModels {
                    provider: "ollama".to_string(),
                    models: vec![
                        entry(
                            "llama3.1:8b",
                            ModelDetails::OllamaLocal {
                                size_bytes: 1,
                                digest: "d".to_string(),
                                family: None,
                                families: vec![],
                                parameter_size: None,
                                quantization_level: None,
                                format: None,
                                context_length: None,
                                capabilities: vec![],
                            },
                            vec!["llama3.1:8b"],
                        ),
                        entry(
                            "mistral",
                            ModelDetails::OllamaRemote {
                                description: None,
                                capabilities: vec![],
                                parameter_sizes: vec![],
                                pulls: None,
                            },
                            vec![],
                        ),
                        entry(
                            "nomic-embed-text",
                            ModelDetails::OllamaLocalEmbedding {
                                size_bytes: 1,
                                digest: "d".to_string(),
                                family: None,
                                families: vec![],
                                parameter_size: None,
                                quantization_level: None,
                                format: None,
                                context_length: None,
                                embedding_length: None,
                                capabilities: vec![],
                            },
                            vec!["nomic-embed-text"],
                        ),
                    ],
                    errors: vec![],
                },
            ],
        }
    }

    #[test]
    fn provider_names_lists_every_provider_in_order() {
        assert_eq!(provider_names(&catalog()), vec!["anthropic", "ollama"]);
    }

    #[test]
    fn chat_model_ids_excludes_embedding_models() {
        let ids = chat_model_ids(&catalog(), "anthropic");
        assert_eq!(ids, vec!["claude-opus-5"]);
    }

    #[test]
    fn chat_model_ids_prefers_pulled_ollama_tags_over_remote_only_entries() {
        let ids = chat_model_ids(&catalog(), "ollama");
        assert_eq!(ids, vec!["llama3.1:8b"], "the unpulled 'mistral' entry should not show up first");
    }

    #[test]
    fn chat_model_ids_is_empty_for_an_unknown_provider() {
        assert!(chat_model_ids(&catalog(), "made-up").is_empty());
    }
}

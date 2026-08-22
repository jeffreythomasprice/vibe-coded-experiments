//! Model-listing types, common across `lib::llm::ModelProvider` and Ollama's
//! two library-specific methods.

use serde::{Deserialize, Serialize};

/// One model, as reported by a provider's model-listing endpoint (or, for
/// Ollama's remote library, scraped from its web index).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Exactly what you'd put in `ChatOptions.model`. For an
    /// [`ModelDetails::OllamaRemote`] entry this is the bare model name
    /// (`"llama3.1"`), not yet a pullable tag — see
    /// [`ModelDetails::OllamaRemoteTag`] for those.
    pub id: String,
    pub display_name: Option<String>,
    /// Unix seconds. Normalized here so lists from different providers sort
    /// together; each provider reports this differently (Anthropic sends
    /// RFC 3339, OpenAI already sends unix seconds, Ollama's local list sends
    /// RFC 3339 and its remote library gives only a relative age).
    pub created_at: Option<i64>,
    pub details: ModelDetails,
}

/// Provider-specific detail. The variant doubles as a capability marker: a
/// caller can tell at a glance whether a model is ready to run
/// ([`ModelDetails::OllamaLocal`]) or would need to be pulled first
/// ([`ModelDetails::OllamaRemote`] / [`ModelDetails::OllamaRemoteTag`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ModelDetails {
    Anthropic {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_input_tokens: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_output_tokens: Option<u64>,
    },
    OpenAi {
        owned_by: String,
    },
    /// Pulled and ready to run — from `GET {base_url}/api/tags`.
    OllamaLocal {
        size_bytes: u64,
        digest: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        family: Option<String>,
        #[serde(default)]
        families: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parameter_size: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        quantization_level: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_length: Option<u64>,
        #[serde(default)]
        capabilities: Vec<String>,
    },
    /// Available to pull — one card from `ollama.com/library`.
    OllamaRemote {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// e.g. `"tools"`, `"vision"`, `"thinking"`.
        #[serde(default)]
        capabilities: Vec<String>,
        /// e.g. `"8b"`, `"70b"`, `"405b"`, as displayed on the card.
        #[serde(default)]
        parameter_sizes: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pulls: Option<u64>,
    },
    /// One concrete pullable tag — from
    /// `ollama.com/library/<model>/tags`. `id` on the enclosing
    /// [`ModelInfo`] is the full `<model>:<tag>` string.
    OllamaRemoteTag {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        digest: Option<String>,
        /// e.g. `"4.9GB"`, as displayed — not parsed to bytes, since the
        /// library page never gives an exact byte count.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<String>,
        /// e.g. `"128K context window"`, as displayed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_window: Option<String>,
        #[serde(default)]
        is_latest: bool,
    },
}

/// One row in the model-catalog UI: a [`ModelInfo`] plus, for Ollama, which
/// locally-pulled tags it corresponds to.
///
/// Ollama's two sources don't share an id shape — the local `/api/tags` list
/// reports concrete tags (`"llama3.1:8b"`) while the `ollama.com/library`
/// index reports bare model names (`"llama3.1"`) — so a caller wanting one
/// merged Ollama list needs a place to record the join. `lib::catalog::merge_ollama`
/// builds these; every other provider always sets `local_tags` empty, since
/// nothing there is ever "pulled" in the first place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub info: ModelInfo,
    /// Ollama only: the locally-pulled tags for this model, e.g.
    /// `["llama3.1:8b"]`. Non-empty means the UI should mark the row as
    /// already pulled.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_tags: Vec<String>,
}

/// One provider's slice of a [`ModelCatalog`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderModels {
    /// `ModelProvider::name()` — `"anthropic"`, `"ollama"`, `"openai"`.
    pub provider: String,
    #[serde(default)]
    pub models: Vec<CatalogEntry>,
    /// What went wrong fetching this provider's models, if anything. A
    /// failure here rides alongside `models` rather than failing the whole
    /// command, so one misconfigured provider (a missing API key, an
    /// unreachable Ollama) doesn't blank out the others. Ollama can report
    /// up to two — local and remote are fetched independently — and can
    /// still carry models from whichever source succeeded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

/// Every provider's models, as returned by the `model_catalog` Tauri command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCatalog {
    pub providers: Vec<ProviderModels>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_entry_omits_empty_local_tags() {
        let entry = CatalogEntry {
            info: ModelInfo {
                id: "llama3.1".to_string(),
                display_name: None,
                created_at: None,
                details: ModelDetails::OllamaRemote {
                    description: None,
                    capabilities: vec![],
                    parameter_sizes: vec![],
                    pulls: None,
                },
            },
            local_tags: vec![],
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert!(
            json.get("local_tags").is_none(),
            "expected local_tags to be omitted when empty, got {json}"
        );
    }

    #[test]
    fn catalog_entry_round_trips_with_local_tags() {
        let entry = CatalogEntry {
            info: ModelInfo {
                id: "llama3.1".to_string(),
                display_name: None,
                created_at: None,
                details: ModelDetails::OllamaRemote {
                    description: None,
                    capabilities: vec![],
                    parameter_sizes: vec![],
                    pulls: None,
                },
            },
            local_tags: vec!["llama3.1:8b".to_string()],
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: CatalogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn catalog_entry_deserializes_without_local_tags_field() {
        let json = r#"{
            "info": {
                "id": "gpt-5.6",
                "display_name": null,
                "created_at": null,
                "details": { "source": "open_ai", "owned_by": "openai" }
            }
        }"#;
        let entry: CatalogEntry = serde_json::from_str(json).unwrap();
        assert!(entry.local_tags.is_empty());
    }

    #[test]
    fn model_catalog_round_trips() {
        let catalog = ModelCatalog {
            providers: vec![
                ProviderModels {
                    provider: "anthropic".to_string(),
                    models: vec![],
                    errors: vec!["missing ANTHROPIC_API_KEY".to_string()],
                },
                ProviderModels {
                    provider: "ollama".to_string(),
                    models: vec![CatalogEntry {
                        info: ModelInfo {
                            id: "llama3.1".to_string(),
                            display_name: None,
                            created_at: None,
                            details: ModelDetails::OllamaRemote {
                                description: None,
                                capabilities: vec![],
                                parameter_sizes: vec!["8b".to_string()],
                                pulls: None,
                            },
                        },
                        local_tags: vec!["llama3.1:8b".to_string()],
                    }],
                    errors: vec![],
                },
            ],
        };
        let json = serde_json::to_string(&catalog).unwrap();
        let back: ModelCatalog = serde_json::from_str(&json).unwrap();
        assert_eq!(back, catalog);
        // errors omitted entirely for the provider that has none
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value["providers"][1].get("errors").is_none());
    }
}

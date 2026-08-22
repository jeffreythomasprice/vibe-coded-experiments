//! Model-listing types, common across `lib::llm::ModelProvider` and Ollama's
//! two library-specific methods.

use serde::{Deserialize, Serialize};

/// The only way to address a model: an exact provider and an exact model id on
/// that provider. There is deliberately no "just the provider" form (a bare
/// provider name has no model to send) and no "just the model id" form (a bare
/// id would have to be guessed back to a provider by prefix, which goes stale,
/// and which Ollama — whose ids are arbitrary local tags — breaks outright).
/// See `lib::llm::router::Router`, which resolves one of these to a live
/// provider client.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelRef {
    /// A key registered with the router — `"anthropic"`, `"ollama"`,
    /// `"openai"`, or a caller-registered name (a gateway, a proxy, a test
    /// double). Not necessarily the same as `ChatProvider::name()`.
    pub provider: String,
    /// Exactly what you'd put in `ChatOptions.model`.
    pub model: String,
}

impl ModelRef {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

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
/// ([`ModelDetails::OllamaRemote`] / [`ModelDetails::OllamaRemoteTag`]) — and,
/// since embeddings support landed, whether a model is for chat or for
/// embedding at all. See [`ModelDetails::is_embedding`] and friends below
/// rather than matching the variant set directly at call sites.
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
    /// An OpenAI `text-embedding-*` model — classified from its id prefix,
    /// since `GET /v1/models` reports nothing beyond `{id, created,
    /// owned_by}`. No dimensions or input limit here on purpose: OpenAI
    /// never reports either, and a hardcoded table would go stale silently.
    /// See `lib::llm::openai::wire::classify`.
    OpenAiEmbedding {
        owned_by: String,
    },
    /// An OpenAI `gpt-image-*` / `dall-e*` model — classified the same way
    /// as [`ModelDetails::OpenAiEmbedding`], so it doesn't fall into the
    /// chat bucket.
    OpenAiImage {
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
    /// The `OllamaLocal` twin for a model whose `capabilities` includes
    /// `"embedding"` — pulled and ready to run, from `GET
    /// {base_url}/api/tags`.
    OllamaLocalEmbedding {
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
        /// Doubles as the model's max input, in tokens — Ollama has no
        /// separate embedding-specific input limit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_length: Option<u64>,
        /// The size of the vector this model produces. Reported by
        /// `/api/tags` as `details.embedding_length`; absent on Ollama
        /// versions that predate it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embedding_length: Option<u32>,
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
    /// The `OllamaRemote` twin for a library card whose capability chips
    /// include `"embedding"`. Same fields — the library index never gives
    /// numeric dimensions or limits on any card, chat or embedding.
    OllamaRemoteEmbedding {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default)]
        capabilities: Vec<String>,
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

impl ModelDetails {
    /// Whether this model turns text into vectors, rather than carrying on a
    /// conversation or generating images. The one place the embedding
    /// variant set is named — callers (the demo's filter, `probably_fits`,
    /// future providers) should use this rather than matching variants
    /// directly.
    pub fn is_embedding(&self) -> bool {
        matches!(
            self,
            ModelDetails::OpenAiEmbedding { .. }
                | ModelDetails::OllamaLocalEmbedding { .. }
                | ModelDetails::OllamaRemoteEmbedding { .. }
        )
    }

    /// Whether this model generates images rather than text or vectors.
    pub fn is_image(&self) -> bool {
        matches!(self, ModelDetails::OpenAiImage { .. })
    }

    /// The size of the vector this model produces, when the provider reports
    /// it. `None` for a chat/image model, and also `None` for an embedding
    /// model whose provider doesn't report dimensions (OpenAI, and an Ollama
    /// model pulled before `/api/tags` started sending `embedding_length`).
    pub fn embedding_dimensions(&self) -> Option<u32> {
        match self {
            ModelDetails::OllamaLocalEmbedding {
                embedding_length, ..
            } => *embedding_length,
            _ => None,
        }
    }

    /// The biggest input this model accepts, in tokens, when known. Reuses
    /// `OllamaLocalEmbedding::context_length` — Ollama has no separate
    /// embedding-specific input limit.
    pub fn max_input_tokens(&self) -> Option<u64> {
        match self {
            ModelDetails::Anthropic {
                max_input_tokens, ..
            } => *max_input_tokens,
            ModelDetails::OllamaLocalEmbedding { context_length, .. } => *context_length,
            _ => None,
        }
    }

    /// A rough opt-in check of whether `text` fits this model's input limit,
    /// estimated at [`estimated_tokens`]. `None` when the limit isn't known
    /// (e.g. any OpenAI embedding model) — meaning no claim is made, not that
    /// it fits. Approximate by construction: never used to refuse a request
    /// anywhere in this crate, only offered for a caller who wants to chunk
    /// ahead of time.
    pub fn probably_fits(&self, text: &str) -> Option<bool> {
        let max = self.max_input_tokens()?;
        Some((estimated_tokens(text) as u64) <= max)
    }
}

/// A rough, provider-agnostic token estimate: one token per four characters.
/// This is a rule of thumb, not a tokenizer — it will sometimes be wrong in
/// both directions. Nothing in this crate uses it to refuse a request; it
/// exists for a caller who wants to chunk input ahead of time. See
/// [`ModelDetails::probably_fits`].
pub fn estimated_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
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
    fn model_ref_round_trips_through_json() {
        let model_ref = ModelRef::new("anthropic", "claude-opus-5");
        let json = serde_json::to_value(&model_ref).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"provider": "anthropic", "model": "claude-opus-5"})
        );
        let back: ModelRef = serde_json::from_value(json).unwrap();
        assert_eq!(back, model_ref);
    }

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

    #[test]
    fn old_open_ai_json_still_deserializes_after_the_embedding_and_image_variants_landed() {
        let json = r#"{ "source": "open_ai", "owned_by": "openai" }"#;
        let details: ModelDetails = serde_json::from_str(json).unwrap();
        assert_eq!(
            details,
            ModelDetails::OpenAi {
                owned_by: "openai".to_string()
            }
        );
    }

    #[test]
    fn is_embedding_is_true_only_for_the_three_embedding_variants() {
        let embedding_variants = [
            ModelDetails::OpenAiEmbedding {
                owned_by: "openai".to_string(),
            },
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
            ModelDetails::OllamaRemoteEmbedding {
                description: None,
                capabilities: vec![],
                parameter_sizes: vec![],
                pulls: None,
            },
        ];
        for details in &embedding_variants {
            assert!(details.is_embedding(), "expected {details:?} to be an embedding model");
            assert!(!details.is_image());
        }

        let non_embedding_variants = [
            ModelDetails::Anthropic {
                max_input_tokens: None,
                max_output_tokens: None,
            },
            ModelDetails::OpenAi {
                owned_by: "openai".to_string(),
            },
            ModelDetails::OpenAiImage {
                owned_by: "openai".to_string(),
            },
        ];
        for details in &non_embedding_variants {
            assert!(!details.is_embedding(), "expected {details:?} not to be an embedding model");
        }
        assert!(ModelDetails::OpenAiImage {
            owned_by: "openai".to_string()
        }
        .is_image());
    }

    #[test]
    fn embedding_dimensions_reads_ollama_local_embedding_length_only() {
        let ollama = ModelDetails::OllamaLocalEmbedding {
            size_bytes: 1,
            digest: "d".to_string(),
            family: None,
            families: vec![],
            parameter_size: None,
            quantization_level: None,
            format: None,
            context_length: None,
            embedding_length: Some(768),
            capabilities: vec![],
        };
        assert_eq!(ollama.embedding_dimensions(), Some(768));

        let openai = ModelDetails::OpenAiEmbedding {
            owned_by: "openai".to_string(),
        };
        assert_eq!(
            openai.embedding_dimensions(),
            None,
            "OpenAI never reports dimensions, so this must stay None rather than guess"
        );
    }

    #[test]
    fn max_input_tokens_reads_anthropic_and_ollama_local_embedding_only() {
        let anthropic = ModelDetails::Anthropic {
            max_input_tokens: Some(200_000),
            max_output_tokens: Some(16_000),
        };
        assert_eq!(anthropic.max_input_tokens(), Some(200_000));

        let ollama = ModelDetails::OllamaLocalEmbedding {
            size_bytes: 1,
            digest: "d".to_string(),
            family: None,
            families: vec![],
            parameter_size: None,
            quantization_level: None,
            format: None,
            context_length: Some(2048),
            embedding_length: Some(768),
            capabilities: vec![],
        };
        assert_eq!(ollama.max_input_tokens(), Some(2048));

        let openai = ModelDetails::OpenAiEmbedding {
            owned_by: "openai".to_string(),
        };
        assert_eq!(openai.max_input_tokens(), None);
    }

    #[test]
    fn probably_fits_is_none_when_the_limit_is_unknown() {
        let openai = ModelDetails::OpenAiEmbedding {
            owned_by: "openai".to_string(),
        };
        assert_eq!(
            openai.probably_fits("hello"),
            None,
            "an unknown limit must never be reported as fitting or not fitting"
        );
    }

    #[test]
    fn probably_fits_compares_the_estimate_against_the_known_limit() {
        let ollama = ModelDetails::OllamaLocalEmbedding {
            size_bytes: 1,
            digest: "d".to_string(),
            family: None,
            families: vec![],
            parameter_size: None,
            quantization_level: None,
            format: None,
            context_length: Some(10),
            embedding_length: Some(768),
            capabilities: vec![],
        };
        // 40 chars / 4 = 10 estimated tokens, exactly at the limit: fits.
        assert_eq!(ollama.probably_fits(&"a".repeat(40)), Some(true));
        // One more char pushes the estimate past the limit.
        assert_eq!(ollama.probably_fits(&"a".repeat(41)), Some(false));
    }

    #[test]
    fn estimated_tokens_rounds_up_to_a_whole_token() {
        assert_eq!(estimated_tokens(""), 0);
        assert_eq!(estimated_tokens("abc"), 1);
        assert_eq!(estimated_tokens("abcd"), 1);
        assert_eq!(estimated_tokens("abcde"), 2);
    }
}

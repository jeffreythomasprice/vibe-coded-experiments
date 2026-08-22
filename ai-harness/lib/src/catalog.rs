//! Aggregates `ModelProvider::list_models` (plus, for Ollama,
//! `list_models_remote`) across all three providers into one payload the
//! `model_catalog` Tauri command can hand the frontend as-is.
//!
//! Kept in `lib` rather than `server` so the merge logic — the part worth
//! testing — never needs a running Tauri app: see this crate's module doc.

use std::time::Duration;

use shared::llm::{CatalogEntry, ModelCatalog, ModelInfo, ProviderModels};

use crate::cache::Cache;
use crate::config::Config;
use crate::llm::{AnthropicClient, Freshness, ModelProvider, OllamaClient, OpenAiClient};

/// Build the full catalog: one [`ProviderModels`] per provider, in
/// `[llm]`'s field order (anthropic, ollama, openai).
///
/// A provider failing to build or to list its models never blanks out the
/// others — the failure rides along in that provider's
/// [`ProviderModels::errors`] instead. Ollama in particular can fail its
/// local list, its remote list, or both independently, and still report
/// whichever side succeeded.
pub async fn load(config: &Config) -> ModelCatalog {
    let timeout = config.llm.request_timeout();
    let max_retries = config.llm.max_retries;
    let cache = || Cache::new(&config.cache);

    // Sequential, not concurrent: four requests total, most of them served
    // from the disk cache or failing fast on a missing key, so it's not
    // worth pulling in a join/spawn dependency for a once-per-launch call.
    let anthropic = load_anthropic(config, timeout, max_retries, cache()).await;
    let ollama = load_ollama(config, timeout, max_retries, cache()).await;
    let openai = load_openai(config, timeout, max_retries, cache()).await;

    ModelCatalog {
        providers: vec![anthropic, ollama, openai],
    }
}

async fn load_anthropic(
    config: &Config,
    timeout: Duration,
    max_retries: u32,
    cache: Cache,
) -> ProviderModels {
    let mut errors = Vec::new();
    let models = match AnthropicClient::new(config.llm.anthropic.clone(), timeout, max_retries) {
        Ok(client) => {
            let client = client.with_cache(cache);
            match client.list_models(Freshness::Cached).await {
                Ok(models) => models.into_iter().map(without_local_tag).collect(),
                Err(err) => {
                    tracing::warn!(provider = "anthropic", %err, "failed to list models");
                    errors.push(err.to_string());
                    Vec::new()
                }
            }
        }
        Err(err) => {
            tracing::warn!(provider = "anthropic", %err, "failed to build client");
            errors.push(err.to_string());
            Vec::new()
        }
    };
    ProviderModels {
        provider: "anthropic".to_string(),
        models,
        errors,
    }
}

async fn load_openai(
    config: &Config,
    timeout: Duration,
    max_retries: u32,
    cache: Cache,
) -> ProviderModels {
    let mut errors = Vec::new();
    let models = match OpenAiClient::new(config.llm.openai.clone(), timeout, max_retries) {
        Ok(client) => {
            let client = client.with_cache(cache);
            match client.list_models(Freshness::Cached).await {
                Ok(models) => models.into_iter().map(without_local_tag).collect(),
                Err(err) => {
                    tracing::warn!(provider = "openai", %err, "failed to list models");
                    errors.push(err.to_string());
                    Vec::new()
                }
            }
        }
        Err(err) => {
            tracing::warn!(provider = "openai", %err, "failed to build client");
            errors.push(err.to_string());
            Vec::new()
        }
    };
    ProviderModels {
        provider: "openai".to_string(),
        models,
        errors,
    }
}

async fn load_ollama(
    config: &Config,
    timeout: Duration,
    max_retries: u32,
    cache: Cache,
) -> ProviderModels {
    let mut errors = Vec::new();

    let client = match OllamaClient::new(config.llm.ollama.clone(), timeout, max_retries) {
        Ok(client) => client.with_cache(cache),
        Err(err) => {
            tracing::warn!(provider = "ollama", %err, "failed to build client");
            return ProviderModels {
                provider: "ollama".to_string(),
                models: Vec::new(),
                errors: vec![err.to_string()],
            };
        }
    };

    let local = match client.list_models(Freshness::Cached).await {
        Ok(models) => models,
        Err(err) => {
            tracing::warn!(provider = "ollama", %err, "failed to list local models");
            errors.push(err.to_string());
            Vec::new()
        }
    };
    let remote = match client.list_models_remote(Freshness::Cached).await {
        Ok(models) => models,
        Err(err) => {
            tracing::warn!(provider = "ollama", %err, "failed to list remote models");
            errors.push(err.to_string());
            Vec::new()
        }
    };

    ProviderModels {
        provider: "ollama".to_string(),
        models: merge_ollama(local, remote),
        errors,
    }
}

/// Join Ollama's two sources into one list: every library card, annotated
/// with whichever locally-pulled tags belong to it, plus any locally-pulled
/// model the library index doesn't (or doesn't yet) list — a `:cloud` entry,
/// a custom build, or simply a library page that failed to scrape.
///
/// The two sources don't share an id shape — `local` reports concrete tags
/// (`"llama3.1:8b"`), `remote` reports bare model names (`"llama3.1"`) — so
/// matching splits each local id on its first `:` and compares the base name.
///
/// Sorted by id so the UI doesn't have to.
pub fn merge_ollama(local: Vec<ModelInfo>, remote: Vec<ModelInfo>) -> Vec<CatalogEntry> {
    let mut by_base_name: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for model in &local {
        let base = model
            .id
            .split_once(':')
            .map_or(model.id.as_str(), |(base, _)| base);
        by_base_name
            .entry(base.to_string())
            .or_default()
            .push(model.id.clone());
    }

    let mut claimed: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut entries: Vec<CatalogEntry> = remote
        .into_iter()
        .map(|info| {
            let local_tags = by_base_name.get(&info.id).cloned().unwrap_or_default();
            claimed.extend(local_tags.iter().cloned());
            CatalogEntry { info, local_tags }
        })
        .collect();

    // Anything pulled locally that no library card claimed — a `:cloud`
    // entry, a custom build, or a library scrape that came back empty —
    // still needs to show up, starred, rather than silently disappearing.
    for model in local {
        if claimed.contains(&model.id) {
            continue;
        }
        entries.push(CatalogEntry {
            local_tags: vec![model.id.clone()],
            info: model,
        });
    }

    entries.sort_by(|a, b| a.info.id.cmp(&b.info.id));
    entries
}

/// An entry from a provider where "pulled locally" doesn't apply — every
/// provider but Ollama.
fn without_local_tag(info: ModelInfo) -> CatalogEntry {
    CatalogEntry {
        info,
        local_tags: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::llm::ModelDetails;

    const LIBRARY_HTML: &str = include_str!("llm/ollama/fixtures/library.html");
    const TAGS_JSON: &str = include_str!("llm/ollama/fixtures/tags.json");

    fn fixture_local() -> Vec<ModelInfo> {
        crate::llm::ollama::wire::parse_tags(TAGS_JSON).unwrap()
    }

    fn fixture_remote() -> Vec<ModelInfo> {
        crate::llm::ollama::library::parse_library(LIBRARY_HTML).unwrap()
    }

    #[test]
    fn merges_a_matching_local_tag_onto_its_library_card() {
        let entries = merge_ollama(fixture_local(), fixture_remote());
        let llama = entries.iter().find(|e| e.info.id == "llama3.1").unwrap();
        assert_eq!(llama.local_tags, vec!["llama3.1:8b".to_string()]);
        assert!(
            matches!(llama.info.details, ModelDetails::OllamaRemote { .. }),
            "a matched card keeps its OllamaRemote details, got {:?}",
            llama.info.details
        );
    }

    #[test]
    fn a_library_card_with_nothing_pulled_stars_nothing() {
        let entries = merge_ollama(fixture_local(), fixture_remote());
        let minilm = entries.iter().find(|e| e.info.id == "all-minilm").unwrap();
        assert!(minilm.local_tags.is_empty());
    }

    #[test]
    fn local_only_models_are_appended_and_starred() {
        let entries = merge_ollama(fixture_local(), fixture_remote());

        let qwen = entries
            .iter()
            .find(|e| e.info.id == "qwen3.8:latest")
            .unwrap();
        assert_eq!(qwen.local_tags, vec!["qwen3.8:latest".to_string()]);
        assert!(matches!(
            qwen.info.details,
            ModelDetails::OllamaLocal { .. }
        ));

        let cloud = entries
            .iter()
            .find(|e| e.info.id == "glm-5.2:cloud")
            .unwrap();
        assert_eq!(cloud.local_tags, vec!["glm-5.2:cloud".to_string()]);
    }

    #[test]
    fn every_model_from_both_sources_survives_the_merge() {
        let local = fixture_local();
        let remote = fixture_remote();
        let expected_ids: std::collections::HashSet<String> = remote
            .iter()
            .map(|m| m.id.clone())
            .chain(vec![
                "qwen3.8:latest".to_string(),
                "glm-5.2:cloud".to_string(),
            ])
            .collect();
        let entries = merge_ollama(local, remote);
        let got_ids: std::collections::HashSet<String> =
            entries.iter().map(|e| e.info.id.clone()).collect();
        assert_eq!(got_ids, expected_ids);
    }

    #[test]
    fn result_is_sorted_by_id() {
        let entries = merge_ollama(fixture_local(), fixture_remote());
        let ids: Vec<&str> = entries.iter().map(|e| e.info.id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn an_empty_remote_list_still_returns_every_local_model() {
        let entries = merge_ollama(fixture_local(), Vec::new());
        assert_eq!(entries.len(), fixture_local().len());
        for entry in &entries {
            assert_eq!(entry.local_tags, vec![entry.info.id.clone()]);
        }
    }

    #[test]
    fn an_empty_local_list_stars_nothing() {
        let entries = merge_ollama(Vec::new(), fixture_remote());
        assert_eq!(entries.len(), fixture_remote().len());
        for entry in &entries {
            assert!(entry.local_tags.is_empty());
        }
    }
}

use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Embedded copy of `config.example.toml`. Used as the source of defaults so
/// the binary can run with no on-disk config and so missing keys/sections in
/// the user's config fall back to known-good values.
const DEFAULT_CONFIG_TOML: &str = include_str!("../config.example.toml");

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Config {
    /// Path to the turso/SQLite DB file. When loaded from a config file,
    /// relative paths are resolved against the directory containing that
    /// file. When running from embedded defaults only, the path is left
    /// relative and resolved against the CWD at use time.
    pub db_path: PathBuf,

    pub ollama: OllamaConfig,

    pub anthropic: AnthropicConfig,

    pub llm: LlmConfig,

    pub summarize: SummarizeConfig,

    pub chunking: ChunkingConfig,

    pub server: ServerConfig,

    pub logging: LoggingConfig,

    pub search: SearchConfig,

    pub ocr: OcrConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct OllamaConfig {
    pub url: String,
    pub embedding_model: String,
    pub chat_model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AnthropicConfig {
    pub base_url: String,
    pub api_key_env: String,
    pub model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    Ollama,
    Anthropic,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub max_output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SummarizeConfig {
    pub group_size: usize,
    pub max_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ChunkingConfig {
    pub chunk_size_bytes: usize,
    pub overlap_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ServerConfig {
    pub socket_path: PathBuf,
    pub idle_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LoggingConfig {
    pub file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SearchConfig {
    pub default_results_per_document: usize,
    pub relevancy_cutoff: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct OcrConfig {
    pub enabled: bool,
    pub min_decodable_ratio: f32,
    pub min_chars_for_check: usize,
    pub dpi: u32,
    pub language: String,
    pub psm: u32,
}

#[derive(Debug, Default, Deserialize)]
struct PartialConfig {
    #[serde(default)]
    db_path: Option<PathBuf>,
    #[serde(default)]
    ollama: Option<PartialOllamaConfig>,
    #[serde(default)]
    anthropic: Option<PartialAnthropicConfig>,
    #[serde(default)]
    llm: Option<PartialLlmConfig>,
    #[serde(default)]
    summarize: Option<PartialSummarizeConfig>,
    #[serde(default)]
    chunking: Option<PartialChunkingConfig>,
    #[serde(default)]
    server: Option<PartialServerConfig>,
    #[serde(default)]
    logging: Option<PartialLoggingConfig>,
    #[serde(default)]
    search: Option<PartialSearchConfig>,
    #[serde(default)]
    ocr: Option<PartialOcrConfig>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialOllamaConfig {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    embedding_model: Option<String>,
    #[serde(default)]
    chat_model: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialAnthropicConfig {
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    api_key_env: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialLlmConfig {
    #[serde(default)]
    provider: Option<LlmProvider>,
    #[serde(default)]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialSummarizeConfig {
    #[serde(default)]
    group_size: Option<usize>,
    #[serde(default)]
    max_depth: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialChunkingConfig {
    #[serde(default)]
    chunk_size_bytes: Option<usize>,
    #[serde(default)]
    overlap_bytes: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialServerConfig {
    #[serde(default)]
    socket_path: Option<PathBuf>,
    #[serde(default)]
    idle_timeout_secs: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialLoggingConfig {
    #[serde(default)]
    file: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialSearchConfig {
    #[serde(default)]
    default_results_per_document: Option<usize>,
    #[serde(default)]
    relevancy_cutoff: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct PartialOcrConfig {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    min_decodable_ratio: Option<f32>,
    #[serde(default)]
    min_chars_for_check: Option<usize>,
    #[serde(default)]
    dpi: Option<u32>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    psm: Option<u32>,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("explicit --config path not found: {0}")]
    ExplicitNotFound(PathBuf),

    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error(
        "invalid [chunking] config: overlap_bytes ({overlap}) must be less than chunk_size_bytes ({size})"
    )]
    InvalidChunking { size: usize, overlap: usize },

    #[error("invalid [chunking] config: chunk_size_bytes must be > 0")]
    ZeroChunkSize,

    #[error("rendering config to TOML: {0}")]
    Render(#[from] toml::ser::Error),
}

/// Search order (first existing wins): `./config.toml`, then
/// `~/.config/document-search/config.toml` (XDG).
pub fn default_search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("config.toml")];
    if let Some(project) = ProjectDirs::from("", "", "document-search") {
        paths.push(project.config_dir().join("config.toml"));
    }
    paths
}

/// Resolve and load the config.
///
/// - If `explicit` is `Some`, the file must exist or we error.
/// - Otherwise we look in `default_search_paths()`. If a file is found it is
///   parsed as a partial overlay on top of the embedded defaults.
/// - If no file is found at all, the embedded defaults are used as-is and
///   `Ok((None, _))` is returned.
pub fn load(explicit: Option<&Path>) -> Result<(Option<PathBuf>, Config), ConfigError> {
    load_with_paths(explicit, &default_search_paths())
}

fn load_with_paths(
    explicit: Option<&Path>,
    search_paths: &[PathBuf],
) -> Result<(Option<PathBuf>, Config), ConfigError> {
    let path = match explicit {
        Some(p) => {
            if !p.exists() {
                return Err(ConfigError::ExplicitNotFound(p.to_path_buf()));
            }
            Some(p.to_path_buf())
        }
        None => search_paths.iter().find(|p| p.exists()).cloned(),
    };

    let cfg = match &path {
        Some(p) => {
            let raw = std::fs::read_to_string(p).map_err(|source| ConfigError::Read {
                path: p.clone(),
                source,
            })?;
            let partial: PartialConfig =
                toml::from_str(&raw).map_err(|source| ConfigError::Parse {
                    path: p.clone(),
                    source,
                })?;
            let mut merged = merge(embedded_defaults(), partial);
            resolve_relative_db_path(&mut merged, p);
            merged
        }
        None => embedded_defaults(),
    };

    validate_chunking(&cfg.chunking)?;
    Ok((path, cfg))
}

/// Render the assembled config back out as TOML. Used by the `print-config`
/// subcommand to show what's actually in effect.
pub fn render_toml(cfg: &Config) -> Result<String, ConfigError> {
    Ok(toml::to_string_pretty(cfg)?)
}

fn embedded_defaults() -> Config {
    toml::from_str(DEFAULT_CONFIG_TOML)
        .expect("embedded config.example.toml must parse as a complete Config")
}

fn merge(base: Config, overlay: PartialConfig) -> Config {
    Config {
        db_path: overlay.db_path.unwrap_or(base.db_path),
        ollama: merge_ollama(base.ollama, overlay.ollama),
        anthropic: merge_anthropic(base.anthropic, overlay.anthropic),
        llm: merge_llm(base.llm, overlay.llm),
        summarize: merge_summarize(base.summarize, overlay.summarize),
        chunking: merge_chunking(base.chunking, overlay.chunking),
        server: merge_server(base.server, overlay.server),
        logging: merge_logging(base.logging, overlay.logging),
        search: merge_search(base.search, overlay.search),
        ocr: merge_ocr(base.ocr, overlay.ocr),
    }
}

fn merge_ollama(base: OllamaConfig, overlay: Option<PartialOllamaConfig>) -> OllamaConfig {
    let Some(overlay) = overlay else {
        return base;
    };
    OllamaConfig {
        url: overlay.url.unwrap_or(base.url),
        embedding_model: overlay.embedding_model.unwrap_or(base.embedding_model),
        chat_model: overlay.chat_model.unwrap_or(base.chat_model),
    }
}

fn merge_anthropic(
    base: AnthropicConfig,
    overlay: Option<PartialAnthropicConfig>,
) -> AnthropicConfig {
    let Some(overlay) = overlay else {
        return base;
    };
    AnthropicConfig {
        base_url: overlay.base_url.unwrap_or(base.base_url),
        api_key_env: overlay.api_key_env.unwrap_or(base.api_key_env),
        model: overlay.model.unwrap_or(base.model),
    }
}

fn merge_llm(base: LlmConfig, overlay: Option<PartialLlmConfig>) -> LlmConfig {
    let Some(overlay) = overlay else {
        return base;
    };
    LlmConfig {
        provider: overlay.provider.unwrap_or(base.provider),
        max_output_tokens: overlay.max_output_tokens.unwrap_or(base.max_output_tokens),
    }
}

fn merge_summarize(
    base: SummarizeConfig,
    overlay: Option<PartialSummarizeConfig>,
) -> SummarizeConfig {
    let Some(overlay) = overlay else {
        return base;
    };
    SummarizeConfig {
        group_size: overlay.group_size.unwrap_or(base.group_size),
        max_depth: overlay.max_depth.unwrap_or(base.max_depth),
    }
}

fn merge_chunking(base: ChunkingConfig, overlay: Option<PartialChunkingConfig>) -> ChunkingConfig {
    let Some(overlay) = overlay else {
        return base;
    };
    ChunkingConfig {
        chunk_size_bytes: overlay.chunk_size_bytes.unwrap_or(base.chunk_size_bytes),
        overlap_bytes: overlay.overlap_bytes.unwrap_or(base.overlap_bytes),
    }
}

fn merge_server(base: ServerConfig, overlay: Option<PartialServerConfig>) -> ServerConfig {
    let Some(overlay) = overlay else {
        return base;
    };
    ServerConfig {
        socket_path: overlay.socket_path.unwrap_or(base.socket_path),
        idle_timeout_secs: overlay.idle_timeout_secs.unwrap_or(base.idle_timeout_secs),
    }
}

fn merge_logging(base: LoggingConfig, overlay: Option<PartialLoggingConfig>) -> LoggingConfig {
    let Some(overlay) = overlay else {
        return base;
    };
    LoggingConfig {
        file: overlay.file.unwrap_or(base.file),
    }
}

fn merge_search(base: SearchConfig, overlay: Option<PartialSearchConfig>) -> SearchConfig {
    let Some(overlay) = overlay else {
        return base;
    };
    SearchConfig {
        default_results_per_document: overlay
            .default_results_per_document
            .unwrap_or(base.default_results_per_document),
        relevancy_cutoff: overlay.relevancy_cutoff.unwrap_or(base.relevancy_cutoff),
    }
}

fn merge_ocr(base: OcrConfig, overlay: Option<PartialOcrConfig>) -> OcrConfig {
    let Some(overlay) = overlay else {
        return base;
    };
    OcrConfig {
        enabled: overlay.enabled.unwrap_or(base.enabled),
        min_decodable_ratio: overlay
            .min_decodable_ratio
            .unwrap_or(base.min_decodable_ratio),
        min_chars_for_check: overlay
            .min_chars_for_check
            .unwrap_or(base.min_chars_for_check),
        dpi: overlay.dpi.unwrap_or(base.dpi),
        language: overlay.language.unwrap_or(base.language),
        psm: overlay.psm.unwrap_or(base.psm),
    }
}

fn validate_chunking(c: &ChunkingConfig) -> Result<(), ConfigError> {
    if c.chunk_size_bytes == 0 {
        return Err(ConfigError::ZeroChunkSize);
    }
    if c.overlap_bytes >= c.chunk_size_bytes {
        return Err(ConfigError::InvalidChunking {
            size: c.chunk_size_bytes,
            overlap: c.overlap_bytes,
        });
    }
    Ok(())
}

fn resolve_relative_db_path(cfg: &mut Config, config_path: &Path) {
    if cfg.db_path.is_absolute() {
        return;
    }
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));
    cfg.db_path = base.join(&cfg.db_path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn embedded_defaults_parse() {
        let cfg = embedded_defaults();
        assert_eq!(cfg.db_path, PathBuf::from("document-search.db"));
        assert_eq!(cfg.ollama.url, "http://localhost:11434");
        assert_eq!(cfg.ollama.embedding_model, "qwen3-embedding:8b");
        assert_eq!(cfg.ollama.chat_model, "qwen2.5:14b");
        assert_eq!(cfg.anthropic.base_url, "https://api.anthropic.com");
        assert_eq!(cfg.anthropic.api_key_env, "ANTHROPIC_API_KEY");
        assert_eq!(cfg.anthropic.model, "claude-haiku-4-5-20251001");
        assert_eq!(cfg.llm.provider, LlmProvider::Ollama);
        assert_eq!(cfg.llm.max_output_tokens, 800);
        assert_eq!(cfg.summarize.group_size, 6);
        assert_eq!(cfg.summarize.max_depth, 4);
        assert_eq!(cfg.chunking.chunk_size_bytes, 5000);
        assert_eq!(cfg.chunking.overlap_bytes, 1000);
        assert_eq!(
            cfg.server.socket_path,
            PathBuf::from("/tmp/document-search.sock")
        );
        assert_eq!(cfg.server.idle_timeout_secs, 15);
        assert_eq!(cfg.logging.file, PathBuf::from("/tmp/document-search.log"));
        assert_eq!(cfg.search.default_results_per_document, 5);
        assert_eq!(cfg.search.relevancy_cutoff, 0.3);
        assert!(cfg.ocr.enabled);
        assert_eq!(cfg.ocr.min_decodable_ratio, 0.7);
        assert_eq!(cfg.ocr.min_chars_for_check, 40);
        assert_eq!(cfg.ocr.dpi, 300);
        assert_eq!(cfg.ocr.language, "eng");
        assert_eq!(cfg.ocr.psm, 3);
    }

    #[test]
    fn load_with_no_file_uses_embedded_defaults() {
        let (path, cfg) = load_with_paths(None, &[]).unwrap();
        assert!(path.is_none());
        assert_eq!(cfg, embedded_defaults());
    }

    #[test]
    fn load_resolves_relative_db_path() {
        let dir = tempdir();
        let cfg_path = dir.join("config.toml");
        fs::write(
            &cfg_path,
            r#"
db_path = "data.db"

[ollama]
url = "http://localhost:11434"
embedding_model = "nomic-embed-text"

[chunking]
chunk_size_bytes = 5000
overlap_bytes = 1000
"#,
        )
        .unwrap();

        let (resolved, cfg) = load_with_paths(Some(&cfg_path), &[]).unwrap();
        assert_eq!(resolved, Some(cfg_path));
        assert_eq!(cfg.db_path, dir.join("data.db"));
    }

    #[test]
    fn load_keeps_absolute_db_path() {
        let dir = tempdir();
        let cfg_path = dir.join("config.toml");
        let abs_db = if cfg!(windows) {
            "C:/tmp/x.db"
        } else {
            "/tmp/x.db"
        };
        fs::write(
            &cfg_path,
            format!(
                r#"
db_path = "{abs_db}"

[ollama]
url = "http://localhost:11434"
embedding_model = "nomic-embed-text"

[chunking]
chunk_size_bytes = 5000
overlap_bytes = 1000
"#
            ),
        )
        .unwrap();

        let (_, cfg) = load_with_paths(Some(&cfg_path), &[]).unwrap();
        assert_eq!(cfg.db_path, PathBuf::from(abs_db));
    }

    #[test]
    fn explicit_missing_errors() {
        let dir = tempdir();
        let missing = dir.join("nope.toml");
        let err = load_with_paths(Some(&missing), &[]).unwrap_err();
        assert!(matches!(err, ConfigError::ExplicitNotFound(_)));
    }

    #[test]
    fn partial_config_overlays_defaults() {
        let dir = tempdir();
        let cfg_path = dir.join("config.toml");
        fs::write(
            &cfg_path,
            r#"
[ollama]
embedding_model = "custom-model"
"#,
        )
        .unwrap();

        let (_, cfg) = load_with_paths(Some(&cfg_path), &[]).unwrap();
        let defaults = embedded_defaults();
        assert_eq!(cfg.ollama.embedding_model, "custom-model");
        assert_eq!(cfg.ollama.url, defaults.ollama.url);
        assert_eq!(cfg.chunking, defaults.chunking);
        // db_path comes from defaults but is now resolved against cfg dir.
        assert_eq!(cfg.db_path, dir.join(&defaults.db_path));
    }

    #[test]
    fn partial_missing_section_uses_defaults() {
        let dir = tempdir();
        let cfg_path = dir.join("config.toml");
        fs::write(&cfg_path, r#"db_path = "x.db""#).unwrap();

        let (_, cfg) = load_with_paths(Some(&cfg_path), &[]).unwrap();
        let defaults = embedded_defaults();
        assert_eq!(cfg.db_path, dir.join("x.db"));
        assert_eq!(cfg.ollama, defaults.ollama);
        assert_eq!(cfg.chunking, defaults.chunking);
    }

    #[test]
    fn render_toml_round_trips() {
        let cfg = embedded_defaults();
        let rendered = render_toml(&cfg).unwrap();
        let parsed: Config = toml::from_str(&rendered).unwrap();
        assert_eq!(cfg, parsed);
    }

    fn tempdir() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "document-search-cfgtest-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}

use std::{
    io,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use audio_recorder_schema::{
    SchemaError,
    config::{CompressionConfig, Config, LlmConfig, SampleFormat, SummarizationConfig, TranscriptionConfig},
    from_toml_layers,
};

pub use audio_recorder_schema::config::LlmProvider;

use crate::paths::{self, PathsError};

const FILE_NAME: &str = "config.toml";

pub const DEFAULTS_TOML: &str = include_str!("../../../config.example.toml");

static DEFAULTS: LazyLock<Config> = LazyLock::new(|| {
    audio_recorder_schema::from_toml_str(DEFAULTS_TOML).expect("config.example.toml is valid")
});

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("invalid config {}: {source}", path.display())]
    Invalid { path: PathBuf, source: SchemaError },
    #[error(transparent)]
    Paths(#[from] PathsError),
    #[error("config.example.toml has no value for {0}")]
    MissingDefault(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelPath {
    pub path: PathBuf,
    pub explicit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveSettings {
    pub enabled: bool,
    pub whisper_model: ModelPath,
    pub partials: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptionSettings {
    pub auto: bool,
    pub whisper_model: ModelPath,
    pub vad_model: ModelPath,
    pub segmentation_model: ModelPath,
    pub embedding_model: ModelPath,
    pub language: String,
    pub diarize: bool,
    pub gpu: bool,
    pub num_speakers: Option<i32>,
    pub cluster_threshold: Option<f32>,
    pub live: LiveSettings,
}

impl Default for TranscriptionSettings {
    fn default() -> Self {
        let dir = paths::default_models_dir().unwrap_or_else(|_| std::env::temp_dir().join("audio-recorder-models"));
        let config = DEFAULTS.transcription.clone().unwrap_or_default();
        Self::resolve_in(&dir, &config).expect("default model names are plain file names")
    }
}

impl TranscriptionSettings {
    fn resolve(config: &TranscriptionConfig) -> Result<Self, ConfigError> {
        let dir = match &config.models_dir {
            Some(dir) => paths::expand_tilde(dir)?,
            None => paths::default_models_dir()?,
        };
        Self::resolve_in(&dir, config)
    }

    fn resolve_in(dir: &Path, config: &TranscriptionConfig) -> Result<Self, ConfigError> {
        let defaults = DEFAULTS.transcription.clone().unwrap_or_default();
        let live = config.live.clone().unwrap_or_default();
        let default_live = defaults.live.clone().unwrap_or_default();
        let model = |name: &'static str, value: &Option<String>, default: &Option<String>| -> Result<ModelPath, ConfigError> {
            let default = default.as_deref().ok_or(ConfigError::MissingDefault(name))?;
            let value = value.as_deref().unwrap_or(default);
            let path = paths::expand_tilde(value)?;
            let path = if path.is_absolute() { path } else { dir.join(path) };
            Ok(ModelPath { path, explicit: value != default })
        };
        Ok(Self {
            auto: config.auto,
            whisper_model: model("transcription.whisper_model", &config.whisper_model, &defaults.whisper_model)?,
            vad_model: model("transcription.vad_model", &config.vad_model, &defaults.vad_model)?,
            segmentation_model: model("transcription.segmentation_model", &config.segmentation_model, &defaults.segmentation_model)?,
            embedding_model: model("transcription.embedding_model", &config.embedding_model, &defaults.embedding_model)?,
            language: config.language.clone(),
            diarize: config.diarize,
            gpu: config.gpu,
            num_speakers: config.num_speakers.map(|n| n.get().min(i32::MAX as u64) as i32),
            cluster_threshold: config.cluster_threshold.map(|t| t as f32),
            live: LiveSettings {
                enabled: live.enabled,
                whisper_model: model("transcription.live.whisper_model", &live.whisper_model, &default_live.whisper_model)?,
                partials: live.partials,
            },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressionSettings {
    pub auto: bool,
    pub live: bool,
    pub codec: audio_recorder_schema::config::Codec,
    pub opus_bitrate_kbps: u32,
}

impl Default for CompressionSettings {
    fn default() -> Self {
        Self::resolve(&DEFAULTS.compression.clone().unwrap_or_default())
    }
}

impl CompressionSettings {
    fn resolve(config: &CompressionConfig) -> Self {
        Self { auto: config.auto, live: config.live, codec: config.codec, opus_bitrate_kbps: config.opus_bitrate_kbps as u32 }
    }
}

pub const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";

#[derive(Debug, Clone, PartialEq)]
pub struct OllamaSettings {
    pub host: String,
    pub model: String,
    pub num_ctx: u32,
}

#[derive(Clone, PartialEq)]
pub struct AnthropicSettings {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
}

impl std::fmt::Debug for AnthropicSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnthropicSettings")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmSettings {
    pub provider: LlmProvider,
    pub ollama: OllamaSettings,
    pub anthropic: AnthropicSettings,
    pub max_output_tokens: u32,
    pub timeout: std::time::Duration,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self::resolve(&DEFAULTS.llm.clone().unwrap_or_default(), std::env::var(ANTHROPIC_API_KEY_ENV).ok())
    }
}

impl LlmSettings {
    fn resolve(config: &LlmConfig, env_api_key: Option<String>) -> Self {
        let ollama = config.ollama.clone().unwrap_or_default();
        let anthropic = config.anthropic.clone().unwrap_or_default();
        let non_empty = |key: Option<String>| key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
        Self {
            provider: config.provider,
            ollama: OllamaSettings {
                host: ollama.host.trim_end_matches('/').to_string(),
                model: ollama.model,
                num_ctx: ollama.num_ctx.clamp(1, u32::MAX as i64) as u32,
            },
            anthropic: AnthropicSettings {
                api_key: non_empty(anthropic.api_key).or_else(|| non_empty(env_api_key)),
                base_url: anthropic.base_url.trim_end_matches('/').to_string(),
                model: anthropic.model,
            },
            max_output_tokens: config.max_output_tokens.clamp(1, u32::MAX as i64) as u32,
            timeout: std::time::Duration::from_secs(config.timeout_secs.get()),
        }
    }

    pub fn model(&self) -> &str {
        match self.provider {
            LlmProvider::Ollama => &self.ollama.model,
            LlmProvider::Anthropic => &self.anthropic.model,
        }
    }

    pub fn provider_name(&self) -> &'static str {
        match self.provider {
            LlmProvider::Ollama => "ollama",
            LlmProvider::Anthropic => "anthropic",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SummarizationSettings {
    pub auto: bool,
    pub language: Option<String>,
    pub prompt: Option<String>,
}

impl Default for SummarizationSettings {
    fn default() -> Self {
        Self::resolve(&DEFAULTS.summarization.clone().unwrap_or_default())
    }
}

impl SummarizationSettings {
    fn resolve(config: &SummarizationConfig) -> Self {
        let non_empty = |value: &Option<String>| value.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        Self { auto: config.auto, language: non_empty(&config.language), prompt: non_empty(&config.prompt) }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub sessions_dir: PathBuf,
    pub sample_format: SampleFormat,
    pub log_file: PathBuf,
    pub transcription: TranscriptionSettings,
    pub compression: CompressionSettings,
    pub llm: LlmSettings,
    pub summarization: SummarizationSettings,
    pub source: Option<PathBuf>,
}

pub fn candidate_paths() -> Result<Vec<PathBuf>, ConfigError> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(FILE_NAME));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        candidates.push(dir.join(FILE_NAME));
    }
    candidates.push(paths::user_config_file()?);
    Ok(candidates)
}

impl ResolvedConfig {
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(&candidate_paths()?)
    }

    pub fn load_from(candidates: &[PathBuf]) -> Result<Self, ConfigError> {
        for path in candidates {
            let text = match std::fs::read_to_string(path) {
                Ok(text) => text,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(source) => return Err(ConfigError::Read { path: path.clone(), source }),
            };
            let config: Config = from_toml_layers(&[DEFAULTS_TOML, &text])
                .map_err(|source| ConfigError::Invalid { path: path.clone(), source })?;
            return Self::resolve(config, Some(path.clone()));
        }
        Self::resolve(DEFAULTS.clone(), None)
    }

    fn resolve(config: Config, source: Option<PathBuf>) -> Result<Self, ConfigError> {
        let sessions_dir = match config.sessions_dir {
            Some(dir) => paths::expand_tilde(&dir)?,
            None => paths::default_sessions_dir()?,
        };
        let log_file = match config.log_file {
            Some(file) => paths::expand_tilde(&file)?,
            None => paths::default_log_file(),
        };
        let transcription = TranscriptionSettings::resolve(&config.transcription.clone().unwrap_or_default())?;
        let compression = config.compression.as_ref().map(CompressionSettings::resolve).unwrap_or_default();
        let llm = LlmSettings::resolve(&config.llm.clone().unwrap_or_default(), std::env::var(ANTHROPIC_API_KEY_ENV).ok());
        let summarization = config.summarization.as_ref().map(SummarizationSettings::resolve).unwrap_or_default();
        Ok(Self { sessions_dir, sample_format: config.sample_format, log_file, transcription, compression, llm, summarization, source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let sub = dir.join(name);
        std::fs::create_dir_all(&sub).unwrap();
        let path = sub.join(FILE_NAME);
        std::fs::write(&path, body).unwrap();
        path
    }

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("audio-recorder-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn defaults_when_no_file() {
        let dir = tmp("defaults");
        let cfg = ResolvedConfig::load_from(&[dir.join("missing.toml")]).unwrap();
        assert!(cfg.source.is_none());
        assert!(cfg.sessions_dir.ends_with("audio-recorder/sessions"));
        assert_eq!(cfg.sample_format, SampleFormat::F32);
    }

    #[test]
    fn first_existing_wins_without_merging() {
        let dir = tmp("priority");
        let first = write(&dir, "a", "sessions_dir = \"/a\"\n");
        let second = write(&dir, "b", "sessions_dir = \"/b\"\nsample_format = \"i16\"\n");
        let cfg = ResolvedConfig::load_from(&[dir.join("none.toml"), first.clone(), second]).unwrap();
        assert_eq!(cfg.source, Some(first));
        assert_eq!(cfg.sessions_dir, PathBuf::from("/a"));
        assert_eq!(cfg.sample_format, SampleFormat::F32);
    }

    #[test]
    fn expands_tilde() {
        let dir = tmp("tilde");
        let path = write(&dir, "a", "sessions_dir = \"~/x/y\"\n");
        let cfg = ResolvedConfig::load_from(&[path]).unwrap();
        assert_eq!(cfg.sessions_dir, paths::home_dir().unwrap().join("x/y"));
    }

    #[test]
    fn log_file_default_and_override() {
        let dir = tmp("logfile");
        let cfg = ResolvedConfig::load_from(&[dir.join("missing.toml")]).unwrap();
        assert_eq!(cfg.log_file, std::env::temp_dir().join("audio-recorder.log"));
        let path = write(&dir, "a", "log_file = \"~/x.log\"\n");
        let cfg = ResolvedConfig::load_from(&[path]).unwrap();
        assert_eq!(cfg.log_file, paths::home_dir().unwrap().join("x.log"));
    }

    #[test]
    fn transcription_defaults_and_explicit_paths() {
        let dir = tmp("transcription");
        let cfg = ResolvedConfig::load_from(&[dir.join("missing.toml")]).unwrap();
        assert!(cfg.transcription.auto && cfg.transcription.diarize && cfg.transcription.gpu);
        assert_eq!(cfg.transcription.language, "auto");
        assert!(!cfg.transcription.whisper_model.explicit);
        assert!(cfg.transcription.whisper_model.path.ends_with("audio-recorder/models/ggml-large-v3-turbo.bin"));
        let path = write(&dir, "a", "[transcription]\nauto = false\nmodels_dir = \"/m\"\nvad_model = \"/v.bin\"\nnum_speakers = 2\n");
        let cfg = ResolvedConfig::load_from(&[path]).unwrap();
        assert!(!cfg.transcription.auto);
        assert_eq!(cfg.transcription.whisper_model, ModelPath { path: PathBuf::from("/m/ggml-large-v3-turbo.bin"), explicit: false });
        assert_eq!(cfg.transcription.vad_model, ModelPath { path: PathBuf::from("/v.bin"), explicit: true });
        assert_eq!(cfg.transcription.num_speakers, Some(2));
        assert!(cfg.transcription.live.enabled && cfg.transcription.live.partials);
        assert_eq!(cfg.transcription.live.whisper_model, ModelPath { path: PathBuf::from("/m/ggml-small.bin"), explicit: false });
        let path = write(&dir, "b", "[transcription.live]\nenabled = false\nwhisper_model = \"/s.bin\"\n");
        let cfg = ResolvedConfig::load_from(&[path]).unwrap();
        assert!(!cfg.transcription.live.enabled && cfg.transcription.live.partials);
        assert_eq!(cfg.transcription.live.whisper_model, ModelPath { path: PathBuf::from("/s.bin"), explicit: true });
    }

    #[test]
    fn llm_defaults_and_api_key_precedence() {
        let settings = LlmSettings::resolve(&DEFAULTS.llm.clone().unwrap_or_default(), None);
        assert_eq!(settings.provider, LlmProvider::Ollama);
        assert_eq!(settings.ollama, OllamaSettings { host: "http://localhost:11434".into(), model: "qwen3.5:9b".into(), num_ctx: 32768 });
        assert_eq!(settings.anthropic.base_url, "https://api.anthropic.com");
        assert_eq!(settings.anthropic.model, "claude-sonnet-5-5");
        assert_eq!(settings.anthropic.api_key, None);
        assert_eq!(settings.max_output_tokens, 8000);

        let from_env = LlmSettings::resolve(&DEFAULTS.llm.clone().unwrap_or_default(), Some("env-key".into()));
        assert_eq!(from_env.anthropic.api_key.as_deref(), Some("env-key"));

        let dir = tmp("llm");
        let path = write(&dir, "a", "[llm]\nprovider = \"anthropic\"\n[llm.anthropic]\napi_key = \"cfg-key\"\nbase_url = \"http://proxy/\"\n[llm.ollama]\nhost = \"http://box:11434/\"\n");
        let text = std::fs::read_to_string(path).unwrap();
        let config: Config = from_toml_layers(&[DEFAULTS_TOML, &text]).unwrap();
        let settings = LlmSettings::resolve(config.llm.as_ref().unwrap(), Some("env-key".into()));
        assert_eq!(settings.provider, LlmProvider::Anthropic);
        assert_eq!(settings.anthropic.api_key.as_deref(), Some("cfg-key"));
        assert_eq!(settings.anthropic.base_url, "http://proxy");
        assert_eq!(settings.ollama.host, "http://box:11434");
        assert!(!format!("{settings:?}").contains("cfg-key"));
    }

    #[test]
    fn summarization_defaults() {
        let dir = tmp("summarization");
        let cfg = ResolvedConfig::load_from(&[dir.join("missing.toml")]).unwrap();
        assert!(cfg.summarization.auto);
        assert_eq!(cfg.summarization.language, None);
        let path = write(&dir, "a", "[summarization]\nauto = false\nlanguage = \" \"\n");
        let cfg = ResolvedConfig::load_from(&[path]).unwrap();
        assert!(!cfg.summarization.auto);
        assert_eq!(cfg.summarization.language, None);
    }

    #[test]
    fn example_file_matches_schema_defaults() {
        let from_schema: Config = audio_recorder_schema::from_toml_str("").unwrap();
        assert_eq!(DEFAULTS.sample_format, from_schema.sample_format);
        assert_eq!(CompressionSettings::default(), CompressionSettings::resolve(&CompressionConfig::default()));
        assert_eq!(SummarizationSettings::default(), SummarizationSettings::resolve(&SummarizationConfig::default()));
        assert_eq!(
            LlmSettings::resolve(&DEFAULTS.llm.clone().unwrap(), None),
            LlmSettings::resolve(&LlmConfig::default(), None)
        );
        let schema_transcription = TranscriptionConfig::default();
        let example = DEFAULTS.transcription.clone().unwrap();
        assert_eq!(
            (example.auto, example.language.as_str(), example.diarize, example.gpu),
            (schema_transcription.auto, schema_transcription.language.as_str(), schema_transcription.diarize, schema_transcription.gpu)
        );
    }

    #[test]
    fn rejects_unknown_key() {
        let dir = tmp("unknown");
        let path = write(&dir, "a", "sesions_dir = \"/a\"\n");
        let err = ResolvedConfig::load_from(&[path]).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid { .. }), "{err}");
    }

    #[test]
    fn rejects_bad_value() {
        let dir = tmp("badvalue");
        let path = write(&dir, "a", "sample_format = \"mp3\"\n");
        let err = ResolvedConfig::load_from(&[path]).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid { .. }), "{err}");
    }
}

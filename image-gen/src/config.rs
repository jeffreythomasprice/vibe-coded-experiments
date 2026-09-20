use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::llm::LlmConfig;
use crate::preset::Preset;

pub const DEFAULT_LOG_FILTER: &str = "warn,image_gen=trace";
pub const DEFAULT_LOG_DIR: &str = "/tmp/image-gen/logs";
pub const DEFAULT_LOG_MAX_BYTES: u64 = 100 * 1024 * 1024;
pub const DEFAULT_LOG_MAX_FILES: usize = 15;
pub const DEFAULT_MODELS_DIR: &str = "/tmp/image-gen";
const FILE_NAME: &str = "config.toml";

#[derive(Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub log_filter: String,
    pub log_dir: PathBuf,
    pub log_max_bytes: u64,
    pub log_max_files: usize,
    pub models_dir: PathBuf,
    pub presets: BTreeMap<String, Preset>,
    pub llm: LlmConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_filter: DEFAULT_LOG_FILTER.to_owned(),
            log_dir: PathBuf::from(DEFAULT_LOG_DIR),
            log_max_bytes: DEFAULT_LOG_MAX_BYTES,
            log_max_files: DEFAULT_LOG_MAX_FILES,
            models_dir: PathBuf::from(DEFAULT_MODELS_DIR),
            presets: BTreeMap::new(),
            llm: LlmConfig::default(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Loaded {
    pub config: Config,
    pub source: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file not found: {}", .0.display())]
    NotFound(PathBuf),

    #[error("failed to read config file {}: {source}", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config file {}: {source}", .path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

pub fn load(explicit: Option<&Path>) -> Result<Loaded, ConfigError> {
    if let Some(path) = explicit {
        if !path.is_file() {
            return Err(ConfigError::NotFound(path.to_path_buf()));
        }
        return read(path).map(|config| Loaded {
            config,
            source: Some(path.to_path_buf()),
        });
    }

    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(FILE_NAME));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join(FILE_NAME));
    }
    if let Some(dir) = dirs::config_dir() {
        candidates.push(dir.join("image-gen").join(FILE_NAME));
    }

    load_from_candidates(&candidates)
}

fn load_from_candidates(candidates: &[PathBuf]) -> Result<Loaded, ConfigError> {
    for path in candidates {
        if path.is_file() {
            return read(path).map(|config| Loaded {
                config,
                source: Some(path.clone()),
            });
        }
    }
    Ok(Loaded {
        config: Config::default(),
        source: None,
    })
}

fn read(path: &Path) -> Result<Config, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_existing_candidate_wins() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.toml");
        let second = dir.path().join("second.toml");
        std::fs::write(&first, "models_dir = \"/tmp/first\"\n").unwrap();
        std::fs::write(&second, "models_dir = \"/tmp/second\"\n").unwrap();

        let loaded = load_from_candidates(&[first.clone(), second]).unwrap();

        assert_eq!(loaded.source, Some(first));
        assert_eq!(loaded.config.models_dir, PathBuf::from("/tmp/first"));
    }

    #[test]
    fn absent_candidates_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");
        let present = dir.path().join("present.toml");
        std::fs::write(&present, "models_dir = \"/tmp/present\"\n").unwrap();

        let loaded = load_from_candidates(&[missing, present.clone()]).unwrap();

        assert_eq!(loaded.source, Some(present));
        assert_eq!(loaded.config.models_dir, PathBuf::from("/tmp/present"));
    }

    #[test]
    fn no_candidates_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");

        let loaded = load_from_candidates(&[missing]).unwrap();

        assert_eq!(loaded.source, None);
        assert_eq!(loaded.config, Config::default());
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "log_filter = \"info\"\n").unwrap();

        let loaded = load_from_candidates(&[path]).unwrap();

        assert_eq!(loaded.config.log_filter, "info");
        assert_eq!(loaded.config.models_dir, PathBuf::from(DEFAULT_MODELS_DIR));
    }

    #[test]
    fn log_dir_can_be_overridden() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "log_dir = \"/var/log/image-gen\"\n").unwrap();

        let loaded = load_from_candidates(&[path]).unwrap();

        assert_eq!(loaded.config.log_dir, PathBuf::from("/var/log/image-gen"));
        assert_eq!(loaded.config.models_dir, PathBuf::from(DEFAULT_MODELS_DIR));
    }

    #[test]
    fn preset_table_is_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            concat!(
                "[presets.turbo]\n",
                "model = \"stabilityai/sd-turbo\"\n",
                "steps = 4\n",
                "cfg_scale = 1.0\n",
                "guidance = 0.0\n",
            ),
        )
        .unwrap();

        let loaded = load_from_candidates(&[path]).unwrap();

        let turbo = loaded.config.presets.get("turbo").unwrap();
        assert_eq!(turbo.model, Some("stabilityai/sd-turbo".parse().unwrap()));
        assert_eq!(turbo.steps, Some(4));
        assert_eq!(turbo.cfg_scale, Some(1.0));
        assert_eq!(turbo.guidance, Some(0.0));
    }

    #[test]
    fn preset_eval_and_rewrite_fields_are_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            concat!(
                "[presets.eval]\n",
                "eval = [\"vqa\", \"tit\"]\n",
                "rewrite = \"always\"\n",
                "rewrite_threshold = 300\n",
                "vqa_model = \"qwen3.8:latest\"\n",
                "eval_max_px = 512\n",
            ),
        )
        .unwrap();

        let loaded = load_from_candidates(&[path]).unwrap();

        let preset = loaded.config.presets.get("eval").unwrap();
        assert_eq!(
            preset.eval,
            Some(vec![crate::cli::EvalMetric::Vqa, crate::cli::EvalMetric::Tit])
        );
        assert_eq!(preset.rewrite, Some(crate::cli::RewriteMode::Always));
        assert_eq!(preset.rewrite_threshold, Some(300));
        assert_eq!(preset.vqa_model, Some("qwen3.8:latest".to_owned()));
        assert_eq!(preset.eval_max_px, Some(512));
    }

    #[test]
    fn preset_invalid_eval_metric_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[presets.eval]\neval = [\"nope\"]\n").unwrap();

        let err = load_from_candidates(std::slice::from_ref(&path)).unwrap_err();

        match err {
            ConfigError::Parse { source, .. } => {
                let message = source.to_string();
                assert!(message.contains("vqa"), "message was: {message}");
            }
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn preset_typo_key_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[presets.turbo]\nstep = 4\n").unwrap();

        let err = load_from_candidates(std::slice::from_ref(&path)).unwrap_err();

        assert!(matches!(err, ConfigError::Parse { .. }));
    }

    #[test]
    fn preset_invalid_weight_type_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[presets.turbo]\nweight_type = \"not_a_type\"\n").unwrap();

        let err = load_from_candidates(std::slice::from_ref(&path)).unwrap_err();

        match err {
            ConfigError::Parse { source, .. } => {
                let message = source.to_string();
                assert!(message.contains("q8_0"), "message was: {message}");
            }
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn no_presets_table_still_matches_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "log_filter = \"info\"\n").unwrap();

        let loaded = load_from_candidates(&[path]).unwrap();

        assert!(loaded.config.presets.is_empty());
    }

    #[test]
    fn unknown_key_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "not_a_real_key = 1\n").unwrap();

        let err = load_from_candidates(std::slice::from_ref(&path)).unwrap_err();

        match err {
            ConfigError::Parse { path: err_path, .. } => assert_eq!(err_path, path),
            other => panic!("expected Parse error, got {other:?}"),
        }
    }

    #[test]
    fn malformed_toml_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "this is not valid toml\n").unwrap();

        let err = load_from_candidates(std::slice::from_ref(&path)).unwrap_err();

        assert!(matches!(err, ConfigError::Parse { .. }));
    }

    #[test]
    fn llm_section_is_parsed_and_defaults_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            concat!("[llm]\n", "model = \"llama3.2\"\n", "[llm.ollama]\n", "port = 9999\n"),
        )
        .unwrap();

        let loaded = load_from_candidates(&[path]).unwrap();

        assert_eq!(loaded.config.llm.model, Some("llama3.2".to_owned()));
        assert_eq!(loaded.config.llm.ollama.port, 9999);
        assert_eq!(loaded.config.llm.ollama.host, "localhost");

        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");
        let defaulted = load_from_candidates(&[missing]).unwrap();
        assert_eq!(defaulted.config.llm, crate::llm::LlmConfig::default());
    }

    #[test]
    fn explicit_missing_path_is_hard_error() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");

        let err = load(Some(&missing)).unwrap_err();

        assert!(matches!(err, ConfigError::NotFound(p) if p == missing));
    }
}

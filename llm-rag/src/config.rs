use std::path::{Path, PathBuf};

use secrecy::SecretString;
use serde::Deserialize;

use crate::error::ConfigError;
use crate::paths::{default_config_search_paths, default_secrets_search_paths};

/// Top-level config. The `secrets` field is populated separately by
/// [`load_secrets`] after the main config is parsed; `secrets.toml` keys are
/// disjoint from `config.toml` keys, so there is no merge to perform.
///
/// Loader semantics:
/// - `--config` missing → `ConfigError::NotFound` (exit 13).
/// - Default config search exhausted → `ConfigError::NotFound`.
/// - `--secrets` missing → `ConfigError::SecretsNotFound` (exit 13).
/// - Default secrets search exhausted → silently yields a default
///   `SecretsConfig` (all `None`). Features that need a secret error at
///   use-site, not at load.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub server_idle_timeout_secs: u64,
    pub client_request_timeout_secs: u64,
    pub socket_dir: PathBuf,
    pub log_dir: PathBuf,
    /// Path to the local Turso/libSQL database file. If written as a relative
    /// path in `config.toml`, the loader resolves it against the directory of
    /// the config file that was actually loaded (see [`load_from`]).
    pub db_path: PathBuf,
    pub llm: LlmConfig,

    #[serde(skip, default)]
    pub secrets: SecretsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmConfig {
    pub chat: ChatProviderConfig,
    pub embeddings: EmbeddingsProviderConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum ChatProviderConfig {
    Ollama { url: String, model: String },
    Anthropic { model: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum EmbeddingsProviderConfig {
    Ollama {
        url: String,
        model: String,
        dimensions: usize,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecretsConfig {
    pub anthropic_api_key: Option<SecretString>,

    /// Set by the loader if `secrets.toml` was readable but had insecure
    /// permissions on Unix (group/world bits). `main` emits a deferred warning
    /// once the logger is initialized.
    #[serde(skip)]
    pub loaded_from_insecure_path: Option<PathBuf>,
}

pub fn load(
    config_override: Option<&Path>,
    secrets_override: Option<&Path>,
) -> Result<Config, ConfigError> {
    let mut cfg = load_config_only(config_override)?;
    cfg.secrets = load_secrets(secrets_override)?;
    Ok(cfg)
}

fn load_config_only(override_path: Option<&Path>) -> Result<Config, ConfigError> {
    if let Some(path) = override_path {
        return load_from(path).and_then(|opt| {
            opt.ok_or_else(|| ConfigError::NotFound {
                searched: vec![path.to_path_buf()],
            })
        });
    }

    let candidates = default_config_search_paths();
    for candidate in &candidates {
        if let Some(cfg) = load_from(candidate)? {
            return Ok(cfg);
        }
    }
    Err(ConfigError::NotFound {
        searched: candidates,
    })
}

fn load_from(path: &Path) -> Result<Option<Config>, ConfigError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ConfigError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let mut cfg = toml::from_str::<Config>(&contents).map_err(|source| ConfigError::Parse {
        path: path.to_path_buf(),
        source,
    })?;
    resolve_relative_db_path(&mut cfg, path);
    Ok(Some(cfg))
}

/// Resolve a relative `db_path` against the directory of the loaded config
/// file so that e.g. `./local.db` in `~/.config/llm-rag/config.toml` points at
/// `~/.config/llm-rag/local.db` regardless of the process's CWD. Absolute
/// `db_path` values are left untouched.
fn resolve_relative_db_path(cfg: &mut Config, config_path: &Path) {
    if cfg.db_path.is_absolute() {
        return;
    }
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));
    cfg.db_path = base.join(&cfg.db_path);
}

fn load_secrets(override_path: Option<&Path>) -> Result<SecretsConfig, ConfigError> {
    if let Some(path) = override_path {
        return load_secrets_from_paths(&[path.to_path_buf()], /*hard_fail=*/ true);
    }
    load_secrets_from_paths(&default_secrets_search_paths(), /*hard_fail=*/ false)
}

fn load_secrets_from_paths(
    candidates: &[PathBuf],
    hard_fail: bool,
) -> Result<SecretsConfig, ConfigError> {
    for candidate in candidates {
        if let Some(secrets) = load_secrets_from(candidate)? {
            return Ok(secrets);
        }
    }
    if hard_fail {
        Err(ConfigError::SecretsNotFound {
            searched: candidates.to_vec(),
        })
    } else {
        Ok(SecretsConfig::default())
    }
}

fn load_secrets_from(path: &Path) -> Result<Option<SecretsConfig>, ConfigError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ConfigError::SecretsRead {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let mut secrets =
        toml::from_str::<SecretsConfig>(&contents).map_err(|source| ConfigError::SecretsParse {
            path: path.to_path_buf(),
            source,
        })?;
    if permissions::is_world_or_group_accessible(path) {
        secrets.loaded_from_insecure_path = Some(path.to_path_buf());
    }
    Ok(Some(secrets))
}

mod permissions {
    use std::path::Path;

    #[cfg(unix)]
    pub fn is_world_or_group_accessible(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(meta) => (meta.permissions().mode() & 0o077) != 0,
            Err(_) => false,
        }
    }

    #[cfg(not(unix))]
    pub fn is_world_or_group_accessible(_path: &Path) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &TempDir, name: &str, contents: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    fn valid_config_toml() -> &'static str {
        r#"
server_idle_timeout_secs = 10
client_request_timeout_secs = 30
socket_dir = "/tmp/llm-rag/sock"
log_dir = "/tmp/llm-rag/logs"
db_path = "/tmp/llm-rag/local.db"

[llm.chat]
provider = "ollama"
url = "http://localhost:11434"
model = "llama3.2:3b"

[llm.embeddings]
provider = "ollama"
url = "http://localhost:11434"
model = "nomic-embed-text"
dimensions = 768
"#
    }

    #[test]
    fn secrets_file_present_merges_into_config() {
        let dir = TempDir::new().unwrap();
        let cfg_path = write(&dir, "config.toml", valid_config_toml());
        let sec_path = write(&dir, "secrets.toml", r#"anthropic_api_key = "sk-test""#);

        let cfg = load(Some(&cfg_path), Some(&sec_path)).unwrap();
        let key = cfg.secrets.anthropic_api_key.as_ref().unwrap();
        assert_eq!(key.expose_secret(), "sk-test");
    }

    #[test]
    fn secrets_default_search_missing_is_silent() {
        let dir = TempDir::new().unwrap();
        let cfg_path = write(&dir, "config.toml", valid_config_toml());

        let cfg = load_config_only(Some(&cfg_path)).unwrap();
        let secrets = load_secrets_from_paths(&[], false).unwrap();
        assert!(secrets.anthropic_api_key.is_none());
        assert!(secrets.loaded_from_insecure_path.is_none());
        // The orchestrator should also work with no override (uses defaults
        // which won't exist in this tempdir).
        let _ = cfg;
    }

    #[test]
    fn secrets_explicit_override_missing_hard_fails() {
        let dir = TempDir::new().unwrap();
        let cfg_path = write(&dir, "config.toml", valid_config_toml());
        let bogus = dir.path().join("nope.toml");

        let err = load(Some(&cfg_path), Some(&bogus)).unwrap_err();
        match err {
            ConfigError::SecretsNotFound { searched } => {
                assert_eq!(searched, vec![bogus]);
            }
            other => panic!("expected SecretsNotFound, got {other:?}"),
        }
    }

    #[test]
    fn malformed_secrets_file_returns_secrets_parse() {
        let dir = TempDir::new().unwrap();
        let cfg_path = write(&dir, "config.toml", valid_config_toml());
        let sec_path = write(&dir, "secrets.toml", "anthropic_api_key = 42");

        let err = load(Some(&cfg_path), Some(&sec_path)).unwrap_err();
        match err {
            ConfigError::SecretsParse { path, .. } => assert_eq!(path, sec_path),
            other => panic!("expected SecretsParse, got {other:?}"),
        }
    }

    #[test]
    fn secrets_debug_redacts_value() {
        let secrets = SecretsConfig {
            anthropic_api_key: Some(SecretString::from("sk-super-secret-DO-NOT-LEAK")),
            loaded_from_insecure_path: None,
        };
        let s = format!("{secrets:?}");
        assert!(
            !s.contains("sk-super-secret-DO-NOT-LEAK"),
            "debug leaked: {s}"
        );
        assert!(
            s.to_ascii_uppercase().contains("REDACTED"),
            "debug missing REDACTED marker: {s}"
        );
    }

    #[test]
    fn secrets_field_in_config_toml_is_ignored() {
        let dir = TempDir::new().unwrap();
        let cfg_text = format!(
            "{}\n[secrets]\nanthropic_api_key = \"sk-should-be-ignored\"\n",
            valid_config_toml()
        );
        let cfg_path = write(&dir, "config.toml", &cfg_text);
        let cfg = load_config_only(Some(&cfg_path)).unwrap();
        assert!(cfg.secrets.anthropic_api_key.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn world_readable_perms_sets_warning_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let cfg_path = write(&dir, "config.toml", valid_config_toml());
        let sec_path = write(&dir, "secrets.toml", r#"anthropic_api_key = "x""#);
        fs::set_permissions(&sec_path, fs::Permissions::from_mode(0o644)).unwrap();

        let cfg = load(Some(&cfg_path), Some(&sec_path)).unwrap();
        assert_eq!(cfg.secrets.loaded_from_insecure_path, Some(sec_path));
    }

    #[test]
    fn llm_chat_ollama_variant_parses() {
        let dir = TempDir::new().unwrap();
        let cfg_path = write(&dir, "config.toml", valid_config_toml());
        let cfg = load_config_only(Some(&cfg_path)).unwrap();
        match cfg.llm.chat {
            ChatProviderConfig::Ollama { url, model } => {
                assert_eq!(url, "http://localhost:11434");
                assert_eq!(model, "llama3.2:3b");
            }
            other => panic!("expected Ollama, got {other:?}"),
        }
    }

    #[test]
    fn llm_chat_anthropic_variant_parses() {
        let dir = TempDir::new().unwrap();
        let cfg_text = r#"
server_idle_timeout_secs = 10
client_request_timeout_secs = 30
socket_dir = "/tmp/x"
log_dir = "/tmp/x"
db_path = "/tmp/x/local.db"

[llm.chat]
provider = "anthropic"
model = "claude-sonnet-4-5"

[llm.embeddings]
provider = "ollama"
url = "http://localhost:11434"
model = "nomic-embed-text"
dimensions = 768
"#;
        let cfg_path = write(&dir, "config.toml", cfg_text);
        let cfg = load_config_only(Some(&cfg_path)).unwrap();
        match cfg.llm.chat {
            ChatProviderConfig::Anthropic { model } => {
                assert_eq!(model, "claude-sonnet-4-5");
            }
            other => panic!("expected Anthropic, got {other:?}"),
        }
    }

    #[test]
    fn llm_embeddings_ollama_variant_parses() {
        let dir = TempDir::new().unwrap();
        let cfg_path = write(&dir, "config.toml", valid_config_toml());
        let cfg = load_config_only(Some(&cfg_path)).unwrap();
        match cfg.llm.embeddings {
            EmbeddingsProviderConfig::Ollama {
                url,
                model,
                dimensions,
            } => {
                assert_eq!(url, "http://localhost:11434");
                assert_eq!(model, "nomic-embed-text");
                assert_eq!(dimensions, 768);
            }
        }
    }

    #[test]
    fn llm_unknown_provider_rejected() {
        let dir = TempDir::new().unwrap();
        let cfg_text = r#"
server_idle_timeout_secs = 10
client_request_timeout_secs = 30
socket_dir = "/tmp/x"
log_dir = "/tmp/x"
db_path = "/tmp/x/local.db"

[llm.chat]
provider = "openai"
model = "gpt-4"

[llm.embeddings]
provider = "ollama"
url = "http://localhost:11434"
model = "nomic-embed-text"
dimensions = 768
"#;
        let cfg_path = write(&dir, "config.toml", cfg_text);
        let err = load_config_only(Some(&cfg_path)).unwrap_err();
        assert!(matches!(err, ConfigError::Parse { .. }));
    }

    #[test]
    fn db_path_relative_resolves_against_config_dir() {
        let dir = TempDir::new().unwrap();
        let cfg_text = r#"
server_idle_timeout_secs = 10
client_request_timeout_secs = 30
socket_dir = "/tmp/x"
log_dir = "/tmp/x"
db_path = "./local.db"

[llm.chat]
provider = "ollama"
url = "http://localhost:11434"
model = "llama3.2:3b"

[llm.embeddings]
provider = "ollama"
url = "http://localhost:11434"
model = "nomic-embed-text"
dimensions = 768
"#;
        let cfg_path = write(&dir, "config.toml", cfg_text);
        let cfg = load_config_only(Some(&cfg_path)).unwrap();
        assert_eq!(cfg.db_path, dir.path().join("./local.db"));
    }

    #[test]
    fn db_path_absolute_is_preserved() {
        let dir = TempDir::new().unwrap();
        let abs = if cfg!(windows) {
            r"C:\tmp\explicit.db"
        } else {
            "/tmp/explicit.db"
        };
        let cfg_text = format!(
            r#"
server_idle_timeout_secs = 10
client_request_timeout_secs = 30
socket_dir = "/tmp/x"
log_dir = "/tmp/x"
db_path = "{abs}"

[llm.chat]
provider = "ollama"
url = "http://localhost:11434"
model = "llama3.2:3b"

[llm.embeddings]
provider = "ollama"
url = "http://localhost:11434"
model = "nomic-embed-text"
dimensions = 768
"#
        );
        let cfg_path = write(&dir, "config.toml", &cfg_text);
        let cfg = load_config_only(Some(&cfg_path)).unwrap();
        assert_eq!(cfg.db_path, PathBuf::from(abs));
    }

    #[test]
    fn db_path_parent_with_dotdot_resolves_against_config_dir_not_cwd() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let cfg_path = sub.join("config.toml");
        fs::write(
            &cfg_path,
            r#"
server_idle_timeout_secs = 10
client_request_timeout_secs = 30
socket_dir = "/tmp/x"
log_dir = "/tmp/x"
db_path = "../other.db"

[llm.chat]
provider = "ollama"
url = "http://localhost:11434"
model = "llama3.2:3b"

[llm.embeddings]
provider = "ollama"
url = "http://localhost:11434"
model = "nomic-embed-text"
dimensions = 768
"#,
        )
        .unwrap();
        let cfg = load_config_only(Some(&cfg_path)).unwrap();
        assert_eq!(cfg.db_path, sub.join("../other.db"));
    }

    #[cfg(unix)]
    #[test]
    fn restrictive_perms_no_warning_path() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let cfg_path = write(&dir, "config.toml", valid_config_toml());
        let sec_path = write(&dir, "secrets.toml", r#"anthropic_api_key = "x""#);
        fs::set_permissions(&sec_path, fs::Permissions::from_mode(0o600)).unwrap();

        let cfg = load(Some(&cfg_path), Some(&sec_path)).unwrap();
        assert!(cfg.secrets.loaded_from_insecure_path.is_none());
    }
}

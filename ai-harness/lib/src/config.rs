//! TOML configuration, loaded from the filesystem (see `config.example.toml`).
//!
//! Every field has a `#[serde(default = "…")]` backed by the same function the
//! `Default` impl uses, so a missing config file, an empty one, and one that
//! spells out the defaults all behave identically.

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::llm::LlmConfig;

/// Directory name under `~/.config` and the basename of the config file.
const APP_NAME: &str = "ai-harness";
const CONFIG_FILE: &str = "config.toml";

/// Root of the config file.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub sandbox: SandboxConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
}

/// The `[database]` table: the local SQLite file `lib::db::Db::open` reads
/// and migrates on startup.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    /// The SQLite file. Created, along with its parent directory, on first
    /// run. A leading `~` is expanded (see [`Config::expand_paths`]).
    ///
    /// Deliberately the only key here: pool size and busy timeout are
    /// constants in `lib::db`, not deployment settings — a single-user local
    /// file has no tuning story worth a config key that could go wrong.
    #[serde(default = "default_database_path")]
    pub path: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_database_path(),
        }
    }
}

fn default_database_path() -> PathBuf {
    PathBuf::from("/tmp/ai-harness/ai-harness.db")
}

/// The `[cache]` table: a small disk cache for provider responses that are
/// expensive or rate-limited to refetch. The one consumer that actually
/// needs it is `OllamaClient::list_models_remote`/`list_remote_tags` — see
/// `crate::cache`'s module doc for why that scrape in particular needs one —
/// but `catalog::load` wires this cache into all three providers' clients,
/// since fetching every provider's models together on every launch benefits
/// even though no single `list_models` call would on its own.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    /// Directory holding cached response bodies. Created on first write if
    /// absent. A leading `~` is expanded (see [`Config::expand_paths`]).
    #[serde(default = "default_cache_dir")]
    pub dir: PathBuf,
    /// How long a cached response stays fresh. A bare integer is seconds;
    /// `"6h"`/`"30m"` suffixes work too. `0` disables caching — every call
    /// refetches and nothing is ever read back from disk.
    #[serde(
        default = "default_cache_ttl_secs",
        deserialize_with = "crate::llm::config::deserialize_duration_secs"
    )]
    pub ttl_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            dir: default_cache_dir(),
            ttl_secs: default_cache_ttl_secs(),
        }
    }
}

fn default_cache_dir() -> PathBuf {
    PathBuf::from("/tmp/ai-harness/cache")
}

fn default_cache_ttl_secs() -> u64 {
    6 * 3_600
}

/// The `[sandbox]` table: how `lib::sandbox` confines a project's
/// subprocesses. See that module's doc for the argv this builds.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxConfig {
    /// `false` disables sandboxing outright — `lib::sandbox::detect` then
    /// never probes for `bwrap` and every sandboxed command is a hard error,
    /// never a silent unconfined run. The explicit, logged opt-out.
    #[serde(default = "default_sandbox_enabled")]
    pub enabled: bool,
    /// The `bwrap` binary: a bare name is looked up on `PATH`, or an
    /// absolute path pins a specific one. A leading `~` is expanded (see
    /// [`Config::expand_paths`]).
    #[serde(default = "default_bwrap_path")]
    pub bwrap_path: PathBuf,
    /// Whether a sandboxed subprocess can reach the network. On by default;
    /// this is the whole policy today, and the knob is here so a future
    /// per-command policy has somewhere to live.
    #[serde(default = "default_sandbox_network")]
    pub network: bool,
    /// Read-only paths layered under every sandbox so a shell has an OS to
    /// run in — `/usr`, `/bin`, and enough of `/etc` for name resolution and
    /// TLS. **Not** part of a project's virtual filesystem; see
    /// `lib::sandbox`'s module doc for why the sandbox's filesystem and
    /// `lib::vfs`'s in-process one deliberately differ by exactly this
    /// layer. An entry that doesn't exist on this machine is skipped rather
    /// than failing the sandbox.
    #[serde(default = "default_system_paths")]
    pub system_paths: Vec<PathBuf>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: default_sandbox_enabled(),
            bwrap_path: default_bwrap_path(),
            network: default_sandbox_network(),
            system_paths: default_system_paths(),
        }
    }
}

fn default_sandbox_enabled() -> bool {
    true
}

fn default_bwrap_path() -> PathBuf {
    PathBuf::from("bwrap")
}

fn default_sandbox_network() -> bool {
    true
}

fn default_system_paths() -> Vec<PathBuf> {
    [
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/etc/ssl",
        "/etc/ca-certificates",
        "/etc/resolv.conf",
        "/etc/passwd",
        "/etc/group",
        "/etc/nsswitch.conf",
        "/etc/alternatives",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

/// The `[tools]` table: limits the built-in filesystem/bash tools consult at
/// call time — see `lib::agent::builtin`. Not a tool *selection* (that's
/// per-agent, in `AgentConfigInput.tools`) — this is the resource budget
/// every agent's tools share.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolsConfig {
    /// How long `bash` lets a command run before killing it.
    #[serde(default = "default_bash_timeout_secs")]
    pub bash_timeout_secs: u64,
    /// `bash` caps its captured stdout+stderr at this many bytes, appending
    /// an explicit truncation marker rather than silently dropping the rest
    /// — read while the child is still running, not after, so an unbounded
    /// command can never exhaust memory before the cap applies.
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
    /// `read_file` refuses a file larger than this rather than reading it
    /// into memory first and truncating after.
    #[serde(default = "default_max_read_bytes")]
    pub max_read_bytes: usize,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            bash_timeout_secs: default_bash_timeout_secs(),
            max_output_bytes: default_max_output_bytes(),
            max_read_bytes: default_max_read_bytes(),
        }
    }
}

fn default_bash_timeout_secs() -> u64 {
    120
}

fn default_max_output_bytes() -> usize {
    30_000
}

fn default_max_read_bytes() -> usize {
    262_144
}

/// The `[log]` table.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogConfig {
    /// Directory holding the rotating log files. Created if absent.
    #[serde(default = "default_dir")]
    pub dir: PathBuf,
    /// `EnvFilter` directive. `RUST_LOG`, if set, overrides this.
    #[serde(default = "default_filter")]
    pub filter: String,
    #[serde(default)]
    pub rotation: RotationConfig,
}

/// The `[log.rotation]` table.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RotationConfig {
    /// Rotate at least this often, regardless of size.
    #[serde(default)]
    pub interval: Interval,
    /// Rotate once the active file would exceed this many bytes.
    ///
    /// Accepts a bare integer (bytes) or a suffixed string — `"100MB"`,
    /// `"100MiB"`. Always re-serialized as the resolved byte count.
    #[serde(default = "default_max_size", deserialize_with = "deserialize_size")]
    pub max_size: u64,
    /// How many rotated files to retain. The active file is not counted.
    #[serde(default = "default_keep")]
    pub keep: usize,
}

/// How often to rotate irrespective of file size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Interval {
    Hourly,
    #[default]
    Daily,
    Weekly,
    /// Size-based rotation only.
    Never,
}

fn default_dir() -> PathBuf {
    PathBuf::from("/tmp/ai-harness/logs")
}

/// Our own crates at TRACE, every third-party crate at WARN.
fn default_filter() -> String {
    "warn,server=trace,lib=trace,shared=trace,client=trace".to_string()
}

fn default_max_size() -> u64 {
    100 * 1_000_000
}

fn default_keep() -> usize {
    10
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            dir: default_dir(),
            filter: default_filter(),
            rotation: RotationConfig::default(),
        }
    }
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            interval: Interval::default(),
            max_size: default_max_size(),
            keep: default_keep(),
        }
    }
}

impl Config {
    /// Load configuration from the first location that exists, or the built-in
    /// defaults with a `None` source if none do. See [`search_paths`].
    ///
    /// Errors if an explicit CLI path is missing, or if a found file fails to
    /// parse — both indicate user intent we shouldn't silently ignore.
    pub fn load(cli: Option<PathBuf>) -> anyhow::Result<(Self, Option<PathBuf>)> {
        match find_config(cli)? {
            Some(path) => {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading config file {}", path.display()))?;
                let config: Config = toml::from_str(&text)
                    .with_context(|| format!("parsing config file {}", path.display()))?;
                Ok((config, Some(path)))
            }
            None => Ok((Config::default(), None)),
        }
    }

    /// Tilde-expand paths in place. Call once, right after [`Config::load`].
    pub fn expand_paths(&mut self) -> anyhow::Result<()> {
        self.log.dir = expand_tilde(&self.log.dir)?;
        self.cache.dir = expand_tilde(&self.cache.dir)?;
        self.database.path = expand_tilde(&self.database.path)?;
        self.sandbox.bwrap_path = expand_tilde(&self.sandbox.bwrap_path)?;
        Ok(())
    }
}

/// The implicit locations [`Config::load`] consults, in priority order. Used
/// for the "no config found" diagnostic; excludes the explicit CLI path.
pub fn search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(CONFIG_FILE)];
    if let Some(dir) = dirs::config_dir() {
        paths.push(dir.join(APP_NAME).join(CONFIG_FILE));
    }
    paths
}

/// Apply the config search order. An explicit CLI path that doesn't exist is an
/// error (the user asked for a specific file); the implicit locations are simply
/// skipped when absent.
fn find_config(cli: Option<PathBuf>) -> anyhow::Result<Option<PathBuf>> {
    if let Some(path) = cli {
        anyhow::ensure!(path.exists(), "config file not found: {}", path.display());
        return Ok(Some(path));
    }
    Ok(search_paths().into_iter().find(|p| p.exists()))
}

/// Expand a leading `~` or `~/` to the user's home directory. Other paths pass
/// through unchanged. A `~` with no resolvable home is a hard error.
fn expand_tilde(path: &Path) -> anyhow::Result<PathBuf> {
    let s = path.to_string_lossy();
    if s == "~" || s.starts_with("~/") {
        let home =
            dirs::home_dir().context("cannot resolve home directory for `~` in config path")?;
        let rest = s.strip_prefix('~').unwrap().trim_start_matches('/');
        Ok(if rest.is_empty() {
            home
        } else {
            home.join(rest)
        })
    } else {
        Ok(path.to_path_buf())
    }
}

/// Accepts either a TOML integer (bytes) or a suffixed string.
#[derive(Deserialize)]
#[serde(untagged)]
enum SizeSpec {
    Bytes(u64),
    Text(String),
}

fn deserialize_size<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match SizeSpec::deserialize(deserializer)? {
        SizeSpec::Bytes(n) => Ok(n),
        SizeSpec::Text(s) => parse_size(&s).map_err(serde::de::Error::custom),
    }
}

/// Parse a byte size: a bare number, or a number with a decimal (`KB`, `MB`,
/// `GB`) or binary (`KiB`, `MiB`, `GiB`) suffix. Case-insensitive; a lone `B`
/// is allowed. Whitespace between number and suffix is permitted.
fn parse_size(s: &str) -> Result<u64, String> {
    let t = s.trim();
    let split = t.find(|c: char| !c.is_ascii_digit()).unwrap_or(t.len());
    let (digits, suffix) = t.split_at(split);
    if digits.is_empty() {
        return Err(format!("invalid size {s:?}: expected a leading number"));
    }
    let n: u64 = digits
        .parse()
        .map_err(|_| format!("invalid size {s:?}: {digits:?} is not a number"))?;

    let multiplier = match suffix.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "kb" => 1_000,
        "mb" => 1_000_000,
        "gb" => 1_000_000_000,
        "kib" => 1 << 10,
        "mib" => 1 << 20,
        "gib" => 1 << 30,
        other => return Err(format!("invalid size {s:?}: unknown unit {other:?}")),
    };
    n.checked_mul(multiplier)
        .ok_or_else(|| format!("invalid size {s:?}: overflows u64"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sizes_with_and_without_units() {
        assert_eq!(parse_size("1024"), Ok(1024));
        assert_eq!(parse_size("100MB"), Ok(100_000_000));
        assert_eq!(parse_size("100mb"), Ok(100_000_000));
        assert_eq!(parse_size("100 MB"), Ok(100_000_000));
        assert_eq!(parse_size("100MiB"), Ok(100 * (1 << 20)));
        assert_eq!(parse_size("8KB"), Ok(8_000));
        assert_eq!(parse_size("512B"), Ok(512));
    }

    #[test]
    fn rejects_garbage_sizes() {
        assert!(parse_size("").is_err());
        assert!(parse_size("MB").is_err());
        assert!(parse_size("100 potatoes").is_err());
        assert!(parse_size("18446744073709551615GB").is_err()); // overflow
    }

    #[test]
    fn empty_config_matches_builtin_defaults() {
        let parsed: Config = toml::from_str("").unwrap();
        let defaults = Config::default();
        assert_eq!(parsed.log.dir, defaults.log.dir);
        assert_eq!(parsed.log.filter, defaults.log.filter);
        assert_eq!(parsed.log.rotation.max_size, defaults.log.rotation.max_size);
        assert_eq!(parsed.log.rotation.keep, defaults.log.rotation.keep);
        assert_eq!(parsed.log.rotation.interval, defaults.log.rotation.interval);
        assert_eq!(parsed.cache.dir, defaults.cache.dir);
        assert_eq!(parsed.cache.ttl_secs, defaults.cache.ttl_secs);
    }

    #[test]
    fn parses_a_cache_table() {
        let parsed: Config = toml::from_str(
            r#"
            [cache]
            dir = "/var/cache/ai-harness"
            ttl_secs = "30m"
            "#,
        )
        .unwrap();
        assert_eq!(parsed.cache.dir, PathBuf::from("/var/cache/ai-harness"));
        assert_eq!(parsed.cache.ttl_secs, 1_800);
    }

    #[test]
    fn cache_ttl_accepts_a_bare_integer_and_zero_disables_caching() {
        let parsed: Config = toml::from_str("[cache]\nttl_secs = 0\n").unwrap();
        assert_eq!(parsed.cache.ttl_secs, 0);
        let parsed: Config = toml::from_str("[cache]\nttl_secs = 3600\n").unwrap();
        assert_eq!(parsed.cache.ttl_secs, 3_600);
    }

    #[test]
    fn rejects_unknown_cache_keys() {
        assert!(toml::from_str::<Config>("[cache]\nnope = 1\n").is_err());
    }

    #[test]
    fn parses_a_database_table() {
        let parsed: Config = toml::from_str(
            r#"
            [database]
            path = "/var/lib/ai-harness/ai-harness.db"
            "#,
        )
        .unwrap();
        assert_eq!(
            parsed.database.path,
            PathBuf::from("/var/lib/ai-harness/ai-harness.db")
        );
    }

    #[test]
    fn rejects_unknown_database_keys() {
        assert!(toml::from_str::<Config>("[database]\nnope = 1\n").is_err());
    }

    #[test]
    fn parses_a_full_config() {
        let parsed: Config = toml::from_str(
            r#"
            [log]
            dir = "/var/log/ai-harness"
            filter = "info"
            [log.rotation]
            interval = "hourly"
            max_size = "8KB"
            keep = 3
            "#,
        )
        .unwrap();
        assert_eq!(parsed.log.dir, PathBuf::from("/var/log/ai-harness"));
        assert_eq!(parsed.log.filter, "info");
        assert_eq!(parsed.log.rotation.interval, Interval::Hourly);
        assert_eq!(parsed.log.rotation.max_size, 8_000);
        assert_eq!(parsed.log.rotation.keep, 3);
    }

    #[test]
    fn max_size_accepts_a_bare_integer() {
        let parsed: Config = toml::from_str("[log.rotation]\nmax_size = 4096\n").unwrap();
        assert_eq!(parsed.log.rotation.max_size, 4096);
    }

    #[test]
    fn rejects_malformed_toml() {
        assert!(toml::from_str::<Config>("[log").is_err());
        assert!(toml::from_str::<Config>("[log]\nnope = 1\n").is_err());
    }

    #[test]
    fn explicit_missing_cli_path_is_an_error() {
        let err = Config::load(Some(PathBuf::from("/definitely/not/here.toml"))).unwrap_err();
        assert!(err.to_string().contains("config file not found"));
    }

    #[test]
    fn expands_tilde_only_at_the_front() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(
            expand_tilde(Path::new("~/logs")).unwrap(),
            home.join("logs")
        );
        assert_eq!(expand_tilde(Path::new("~")).unwrap(), home);
        assert_eq!(
            expand_tilde(Path::new("/tmp/a~b")).unwrap(),
            PathBuf::from("/tmp/a~b")
        );
    }

    /// Guards `config.example.toml` against drifting out of sync with these
    /// structs — its header claims "the values below ARE the built-in
    /// defaults", so it must always parse, and every value it spells out
    /// must match `Config::default()`.
    #[test]
    fn the_checked_in_example_config_parses_and_matches_the_builtin_defaults() {
        const EXAMPLE: &str = include_str!("../../config.example.toml");
        let parsed: Config = toml::from_str(EXAMPLE).unwrap();
        let defaults = Config::default();
        assert_eq!(parsed.log.dir, defaults.log.dir);
        assert_eq!(parsed.log.filter, defaults.log.filter);
        assert_eq!(parsed.log.rotation.interval, defaults.log.rotation.interval);
        assert_eq!(parsed.log.rotation.max_size, defaults.log.rotation.max_size);
        assert_eq!(parsed.log.rotation.keep, defaults.log.rotation.keep);
        assert_eq!(parsed.llm.request_timeout_secs, defaults.llm.request_timeout_secs);
        assert_eq!(parsed.llm.max_retries, defaults.llm.max_retries);
        assert_eq!(parsed.llm.anthropic.base_url, defaults.llm.anthropic.base_url);
        assert_eq!(parsed.llm.anthropic.api_key_env, defaults.llm.anthropic.api_key_env);
        assert_eq!(
            parsed.llm.anthropic.anthropic_version,
            defaults.llm.anthropic.anthropic_version
        );
        assert_eq!(parsed.llm.ollama.base_url, defaults.llm.ollama.base_url);
        assert_eq!(parsed.llm.ollama.keep_alive, defaults.llm.ollama.keep_alive);
        assert_eq!(parsed.llm.ollama.library_url, defaults.llm.ollama.library_url);
        assert_eq!(parsed.llm.openai.base_url, defaults.llm.openai.base_url);
        assert_eq!(parsed.llm.openai.api_key_env, defaults.llm.openai.api_key_env);
        assert_eq!(parsed.cache.dir, defaults.cache.dir);
        assert_eq!(parsed.cache.ttl_secs, defaults.cache.ttl_secs);
        assert_eq!(parsed.database.path, defaults.database.path);
        assert_eq!(parsed.sandbox.enabled, defaults.sandbox.enabled);
        assert_eq!(parsed.sandbox.bwrap_path, defaults.sandbox.bwrap_path);
        assert_eq!(parsed.sandbox.network, defaults.sandbox.network);
        assert_eq!(parsed.sandbox.system_paths, defaults.sandbox.system_paths);
        assert_eq!(parsed.tools.bash_timeout_secs, defaults.tools.bash_timeout_secs);
        assert_eq!(parsed.tools.max_output_bytes, defaults.tools.max_output_bytes);
        assert_eq!(parsed.tools.max_read_bytes, defaults.tools.max_read_bytes);
    }

    #[test]
    fn parses_a_sandbox_table() {
        let parsed: Config = toml::from_str(
            r#"
            [sandbox]
            enabled = false
            bwrap_path = "/usr/bin/bwrap"
            network = false
            system_paths = ["/usr", "/bin"]
            "#,
        )
        .unwrap();
        assert!(!parsed.sandbox.enabled);
        assert_eq!(parsed.sandbox.bwrap_path, PathBuf::from("/usr/bin/bwrap"));
        assert!(!parsed.sandbox.network);
        assert_eq!(
            parsed.sandbox.system_paths,
            vec![PathBuf::from("/usr"), PathBuf::from("/bin")]
        );
    }

    #[test]
    fn rejects_unknown_sandbox_keys() {
        assert!(toml::from_str::<Config>("[sandbox]\nnope = 1\n").is_err());
    }

    #[test]
    fn sandbox_defaults_are_enabled_with_network() {
        let defaults = SandboxConfig::default();
        assert!(defaults.enabled);
        assert!(defaults.network);
        assert!(!defaults.system_paths.is_empty());
    }

    #[test]
    fn parses_a_tools_table() {
        let parsed: Config = toml::from_str(
            r#"
            [tools]
            bash_timeout_secs = 30
            max_output_bytes = 1000
            max_read_bytes = 2000
            "#,
        )
        .unwrap();
        assert_eq!(parsed.tools.bash_timeout_secs, 30);
        assert_eq!(parsed.tools.max_output_bytes, 1000);
        assert_eq!(parsed.tools.max_read_bytes, 2000);
    }

    #[test]
    fn rejects_unknown_tools_keys() {
        assert!(toml::from_str::<Config>("[tools]\nnope = 1\n").is_err());
    }

    #[test]
    fn tools_defaults_are_sane() {
        let defaults = ToolsConfig::default();
        assert_eq!(defaults.bash_timeout_secs, 120);
        assert_eq!(defaults.max_output_bytes, 30_000);
        assert_eq!(defaults.max_read_bytes, 262_144);
    }
}

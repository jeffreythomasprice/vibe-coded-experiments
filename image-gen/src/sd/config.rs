use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 1234;
pub const DEFAULT_RELEASE_TAG: &str = "latest";
pub const DEFAULT_STARTUP_TIMEOUT_SECS: u64 = 300;
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 1800;
pub const DEFAULT_POLL_INTERVAL_MS: u64 = 250;
pub const DEFAULT_SERVER_DIR: &str = "/tmp/image-gen/sd-server";
pub const DEFAULT_STATE_DIR: &str = "/tmp/image-gen/run";

/// Which prebuilt `sd-server` release asset to download. Distinct from the
/// per-generation `--backend` routing flag (`cli::Backend`, part of `ModelSet`)
/// that tells an already-running server which compute device to use for a
/// model: this instead picks which binary variant is fetched in the first
/// place. There is no `cuda` release asset for Linux — CUDA builds are
/// Windows-only upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseBackend {
    Cpu,
    Vulkan,
    Rocm,
}

impl ReleaseBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            ReleaseBackend::Cpu => "cpu",
            ReleaseBackend::Vulkan => "vulkan",
            ReleaseBackend::Rocm => "rocm",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SdConfig {
    pub host: String,
    pub port: u16,
    pub backend: ReleaseBackend,
    /// A specific upstream tag (e.g. `master-890-74988b2`), or `"latest"` to
    /// resolve the newest release at the time a download is needed.
    pub release_tag: String,
    pub auto_download: bool,
    /// Extracted release trees live at `<server_dir>/<tag>/`, one per tag ever
    /// used, so switching tags never clobbers a working install.
    pub server_dir: PathBuf,
    /// Daemon lock file and state record (`daemon.json`); defaults under the
    /// same `/tmp/image-gen` root as `models_dir`/`log_dir`.
    pub state_dir: PathBuf,
    /// How long to wait for the server to report itself ready after spawning —
    /// generous by default since loading a large checkpoint can take minutes.
    pub startup_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub poll_interval_ms: u64,
}

impl Default for SdConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_owned(),
            port: DEFAULT_PORT,
            backend: ReleaseBackend::Vulkan,
            release_tag: DEFAULT_RELEASE_TAG.to_owned(),
            auto_download: true,
            server_dir: PathBuf::from(DEFAULT_SERVER_DIR),
            state_dir: PathBuf::from(DEFAULT_STATE_DIR),
            startup_timeout_secs: DEFAULT_STARTUP_TIMEOUT_SECS,
            request_timeout_secs: DEFAULT_REQUEST_TIMEOUT_SECS,
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
        }
    }
}

impl SdConfig {
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    pub fn startup_timeout(&self) -> Duration {
        Duration::from_secs(self.startup_timeout_secs)
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }

    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.poll_interval_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_point_at_localhost() {
        let config = SdConfig::default();
        assert_eq!(config.backend, ReleaseBackend::Vulkan);
        assert_eq!(config.release_tag, "latest");
        assert_eq!(config.base_url(), "http://127.0.0.1:1234");
        assert!(config.auto_download);
    }

    #[test]
    fn toml_overrides_are_applied() {
        let parsed: SdConfig = toml::from_str(
            "port = 9999\nbackend = \"cpu\"\nrelease_tag = \"master-890-74988b2\"\n",
        )
        .unwrap();
        assert_eq!(parsed.port, 9999);
        assert_eq!(parsed.backend, ReleaseBackend::Cpu);
        assert_eq!(parsed.release_tag, "master-890-74988b2");
        assert_eq!(parsed.base_url(), "http://127.0.0.1:9999");
    }

    #[test]
    fn unknown_key_is_rejected() {
        let err = toml::from_str::<SdConfig>("not_a_real_key = 1\n").unwrap_err();
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn unknown_backend_value_is_rejected() {
        let err = toml::from_str::<SdConfig>("backend = \"cuda\"\n").unwrap_err();
        assert!(err.to_string().contains("vulkan"));
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let parsed: SdConfig = toml::from_str("port = 5000\n").unwrap();
        assert_eq!(parsed.port, 5000);
        assert_eq!(parsed.host, DEFAULT_HOST);
        assert_eq!(parsed.startup_timeout_secs, DEFAULT_STARTUP_TIMEOUT_SECS);
    }

    #[test]
    fn timeouts_convert_to_durations() {
        let config = SdConfig::default();
        assert_eq!(config.startup_timeout(), Duration::from_secs(300));
        assert_eq!(config.request_timeout(), Duration::from_secs(1800));
        assert_eq!(config.poll_interval(), Duration::from_millis(250));
    }
}

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::cli::Backend;
use crate::sd::SdError;
use crate::sd::client::Client;
use crate::sd::config::SdConfig;
use crate::sd::modelset::ModelSet;
use crate::sd::release;

const STATE_FILE: &str = "daemon.json";
const LOCK_FILE: &str = "daemon.lock";
const LOG_FILE: &str = "sd-server.log";
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(100);
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(200);
const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const LOG_TAIL_LINES: usize = 40;

/// The record `ensure_ready` writes to `<state_dir>/daemon.json` right after a
/// successful spawn. It is the source of truth for "what is this process, and
/// is it what we want" — the live `/sdcpp/v1/capabilities` probe only ever
/// answers "is something alive on the port", not "is it ours" or "is it the
/// right model set", since the capabilities response doesn't carry a
/// fingerprint of its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    pub pid: u32,
    pub port: u16,
    pub fingerprint: String,
    pub tag: String,
    pub args: Vec<String>,
    pub started_unix: u64,
    pub log_path: PathBuf,
}

/// The outcome of `status()`: a state file exists, but is it a live daemon.
#[derive(Debug, Clone)]
pub struct DaemonStatus {
    pub state: DaemonState,
    pub alive: bool,
}

/// Ensures an `sd-server` process matching `model_set` is running and
/// reachable, spawning (or restarting) one if necessary, and returns a client
/// bound to it. The startup lock is held only while reconciling/spawning; the
/// returned `Client` is used for generation entirely outside that lock, so a
/// long-running generation never blocks a second invocation from starting or
/// finding this same daemon.
pub async fn ensure_ready(config: &SdConfig, log_dir: &Path, model_set: &ModelSet) -> Result<Client, SdError> {
    if model_set.backend == Some(Backend::Cuda) {
        return Err(SdError::BackendUnavailable {
            backend: Backend::Cuda.as_str(),
            tag: config.release_tag.clone(),
        });
    }

    std::fs::create_dir_all(&config.state_dir).map_err(|source| io_err("create directory", &config.state_dir, source))?;
    let wanted_fingerprint = model_set.fingerprint();

    if probe_ready(config, &wanted_fingerprint).await {
        return Ok(client_for(config));
    }

    let lock = acquire_lock(config).await?;

    if probe_ready(config, &wanted_fingerprint).await {
        drop(lock);
        return Ok(client_for(config));
    }

    reconcile_and_spawn(config, log_dir, model_set, &wanted_fingerprint).await?;
    drop(lock);
    Ok(client_for(config))
}

/// The current state file plus whether a live process still answers for it, or
/// `None` if no daemon has ever been recorded here.
pub async fn status(config: &SdConfig) -> Option<DaemonStatus> {
    let state = read_state(config)?;
    let alive = client_for(config).capabilities().await.is_ok();
    Some(DaemonStatus { state, alive })
}

/// Stops the recorded daemon, if any is both recorded and actually alive.
/// Returns `false` when there was nothing to stop.
pub async fn stop(config: &SdConfig) -> Result<bool, SdError> {
    let Some(state) = read_state(config) else {
        return Ok(false);
    };
    if client_for(config).capabilities().await.is_ok() {
        terminate(&state).await?;
    }
    let _ = std::fs::remove_file(state_path(config));
    Ok(true)
}

fn client_for(config: &SdConfig) -> Client {
    Client::new(config.base_url(), config.request_timeout())
}

/// Where `ensure_ready` directs the daemon's stdout/stderr — exposed so
/// `sd::progress` can tail the same file a generation's daemon writes to,
/// without hardcoding the filename a second time.
pub fn log_path(log_dir: &Path) -> PathBuf {
    log_dir.join(LOG_FILE)
}

fn state_path(config: &SdConfig) -> PathBuf {
    config.state_dir.join(STATE_FILE)
}

fn lock_path(config: &SdConfig) -> PathBuf {
    config.state_dir.join(LOCK_FILE)
}

fn read_state(config: &SdConfig) -> Option<DaemonState> {
    let text = std::fs::read_to_string(state_path(config)).ok()?;
    serde_json::from_str(&text).ok()
}

/// `true` only when the recorded state matches `wanted_fingerprint` *and* a
/// live probe confirms something is actually answering for it — a state file
/// alone proves nothing about whether the process it describes still exists.
async fn probe_ready(config: &SdConfig, wanted_fingerprint: &str) -> bool {
    match read_state(config) {
        Some(state) if state.fingerprint == wanted_fingerprint => {
            let probe = Client::new(config.base_url(), PROBE_TIMEOUT);
            probe.capabilities().await.is_ok()
        }
        _ => false,
    }
}

/// Blocks (via polling, not an indefinite OS-level wait, so a lost race never
/// leaks a blocking-pool thread) until the startup lock is acquired or
/// `config.startup_timeout()` elapses — bounded this generously because the
/// lock can legitimately be held for as long as a checkpoint takes to load.
async fn acquire_lock(config: &SdConfig) -> Result<std::fs::File, SdError> {
    let path = lock_path(config);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|source| io_err("open", &path, source))?;

    let deadline = Instant::now() + config.startup_timeout();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => {
                if Instant::now() >= deadline {
                    return Err(SdError::LockTimeout {
                        path,
                        waited: config.startup_timeout(),
                    });
                }
                tokio::time::sleep(LOCK_POLL_INTERVAL).await;
            }
            Err(std::fs::TryLockError::Error(source)) => return Err(io_err("lock", &path, source)),
        }
    }
}

enum Port {
    Free,
    Ours(DaemonState),
    RespondingButUnrecognized,
}

/// Distinguishes "nothing is listening" (a connection-level failure) from
/// "something answered, but not with a shape we recognize" (an HTTP-level
/// failure) — only the latter is actually `PortOccupied`, since the former is
/// exactly what we expect to see before our own first spawn.
async fn classify_port(config: &SdConfig) -> Port {
    let probe = Client::new(config.base_url(), PROBE_TIMEOUT);
    match probe.capabilities().await {
        Ok(_) => match read_state(config) {
            Some(state) => Port::Ours(state),
            None => Port::RespondingButUnrecognized,
        },
        Err(SdError::Request { .. }) => Port::Free,
        Err(_) => Port::RespondingButUnrecognized,
    }
}

async fn reconcile_and_spawn(
    config: &SdConfig,
    log_dir: &Path,
    model_set: &ModelSet,
    fingerprint: &str,
) -> Result<(), SdError> {
    match classify_port(config).await {
        Port::Free => {}
        // Reachable here only because the two `probe_ready` calls upstream of
        // this both failed, so a state file that still parses can only mean a
        // stale fingerprint from an earlier model set.
        Port::Ours(state) => {
            terminate(&state).await?;
            let _ = std::fs::remove_file(state_path(config));
        }
        Port::RespondingButUnrecognized => {
            return Err(SdError::PortOccupied {
                addr: config.base_url(),
            });
        }
    }

    spawn_and_wait(config, log_dir, model_set, fingerprint).await
}

/// SIGTERM, then poll for the port to stop answering, then SIGKILL as a
/// fallback for a process wedged deep in a blocking call (e.g. mid-load).
async fn terminate(state: &DaemonState) -> Result<(), SdError> {
    send_signal(state.pid, libc::SIGTERM);

    let deadline = Instant::now() + GRACEFUL_SHUTDOWN_TIMEOUT;
    while process_alive(state.pid) {
        if Instant::now() >= deadline {
            send_signal(state.pid, libc::SIGKILL);
            break;
        }
        tokio::time::sleep(SHUTDOWN_POLL_INTERVAL).await;
    }
    Ok(())
}

fn send_signal(pid: u32, signal: i32) {
    // Best-effort: the process may have already exited between our check and
    // this call, which `kill` simply reports as ESRCH — nothing to recover.
    unsafe {
        libc::kill(pid as libc::pid_t, signal);
    }
}

fn process_alive(pid: u32) -> bool {
    // Signal 0 sends nothing but still performs the existence/permission
    // check, the standard way to poll liveness without actually signaling.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

async fn spawn_and_wait(
    config: &SdConfig,
    log_dir: &Path,
    model_set: &ModelSet,
    fingerprint: &str,
) -> Result<(), SdError> {
    let download_client = reqwest::Client::new();
    let binary = release::ensure_release(&download_client, config).await?;

    std::fs::create_dir_all(log_dir).map_err(|source| io_err("create directory", log_dir, source))?;
    let log_path = log_dir.join(LOG_FILE);
    let stdout_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|source| io_err("open", &log_path, source))?;
    let stderr_log = stdout_log
        .try_clone()
        .map_err(|source| io_err("open", &log_path, source))?;

    let model_args = model_set.to_args();
    let mut command = tokio::process::Command::new(&binary);
    command
        .args(&model_args)
        .arg("--listen-ip")
        .arg(&config.host)
        .arg("--listen-port")
        .arg(config.port.to_string())
        .arg("--log-level")
        .arg("info")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log));
    #[cfg(unix)]
    {
        command.process_group(0);
    }

    let mut child = command.spawn().map_err(|source| SdError::Spawn {
        binary: binary.clone(),
        source,
    })?;
    let pid = child.id().expect("a just-spawned child process has a pid");

    let probe = Client::new(config.base_url(), PROBE_TIMEOUT);
    let deadline = Instant::now() + config.startup_timeout();
    loop {
        if probe.capabilities().await.is_ok() {
            break;
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(SdError::ServerExited {
                code: status.code(),
                log_tail: tail(&log_path, LOG_TAIL_LINES),
            });
        }
        if Instant::now() >= deadline {
            return Err(SdError::StartupTimeout {
                waited: config.startup_timeout(),
                log: tail(&log_path, LOG_TAIL_LINES),
            });
        }
        tokio::time::sleep(config.poll_interval()).await;
    }

    write_state(
        config,
        &DaemonState {
            pid,
            port: config.port,
            fingerprint: fingerprint.to_owned(),
            tag: model_set.release_tag.clone(),
            args: model_args.iter().map(|a| a.to_string_lossy().into_owned()).collect(),
            started_unix: unix_now(),
            log_path,
        },
    )
}

fn write_state(config: &SdConfig, state: &DaemonState) -> Result<(), SdError> {
    let path = state_path(config);
    let mut file = tempfile::NamedTempFile::new_in(&config.state_dir)
        .map_err(|source| io_err("create temp file in", &config.state_dir, source))?;
    serde_json::to_writer_pretty(&mut file, state).map_err(|source| SdError::Decode {
        url: path.display().to_string(),
        source,
    })?;
    file.persist(&path)
        .map_err(|err| io_err("rename temp file to", &path, err.error))?;
    Ok(())
}

fn tail(path: &Path, lines: usize) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn io_err(action: &'static str, path: &Path, source: std::io::Error) -> SdError {
    SdError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use serde_json::json;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn test_config(dir: &Path, port: u16) -> SdConfig {
        SdConfig {
            port,
            server_dir: dir.join("server"),
            state_dir: dir.join("run"),
            startup_timeout_secs: 3,
            poll_interval_ms: 20,
            ..SdConfig::default()
        }
    }

    fn minimal_model_set(tag: &str) -> ModelSet {
        ModelSet {
            model: None,
            diffusion_model: Some(PathBuf::from("/models/does-not-matter.gguf")),
            vae: None,
            clip_l: None,
            clip_g: None,
            t5xxl: None,
            taesd: None,
            text_encoder: None,
            vision_encoder: None,
            weight_type: None,
            threads: None,
            backend: None,
            vae_tiling: false,
            flash_attn: false,
            release_tag: tag.to_owned(),
        }
    }

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    #[tokio::test]
    async fn status_is_none_when_nothing_was_ever_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path(), free_port());
        assert!(status(&config).await.is_none());
    }

    #[tokio::test]
    async fn stop_on_an_unrecorded_daemon_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path(), free_port());
        assert!(!stop(&config).await.unwrap());
    }

    #[tokio::test]
    async fn ensure_ready_rejects_cuda_backend_before_touching_anything() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path(), free_port());
        let mut model_set = minimal_model_set("master-890-74988b2");
        model_set.backend = Some(Backend::Cuda);

        let err = ensure_ready(&config, &dir.path().join("logs"), &model_set)
            .await
            .unwrap_err();
        assert!(matches!(err, SdError::BackendUnavailable { .. }));
        assert!(!config.state_dir.join(STATE_FILE).exists());
    }

    #[tokio::test]
    async fn foreign_process_on_the_port_is_reported_as_occupied() {
        // A real sd-server answers /sdcpp/v1/capabilities with a `model` object;
        // an HTTP server that responds but with an unrelated JSON shape stands
        // in for "some other, non-sd-server process already has this port".
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"unrelated": "shape"})))
            .mount(&server)
            .await;
        let port = reqwest::Url::parse(&server.uri()).unwrap().port().unwrap();

        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path(), port);
        let model_set = minimal_model_set("master-890-74988b2");

        let err = ensure_ready(&config, &dir.path().join("logs"), &model_set)
            .await
            .unwrap_err();
        assert!(matches!(err, SdError::PortOccupied { .. }));
    }

    #[test]
    fn write_and_read_state_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path(), 1234);
        std::fs::create_dir_all(&config.state_dir).unwrap();

        let state = DaemonState {
            pid: 4242,
            port: 1234,
            fingerprint: "abc123".to_owned(),
            tag: "master-890-74988b2".to_owned(),
            args: vec!["--diffusion-model".to_owned(), "/models/x.gguf".to_owned()],
            started_unix: 1_700_000_000,
            log_path: dir.path().join("sd-server.log"),
        };
        write_state(&config, &state).unwrap();

        let read = read_state(&config).unwrap();
        assert_eq!(read.pid, 4242);
        assert_eq!(read.fingerprint, "abc123");
        assert_eq!(read.args, vec!["--diffusion-model", "/models/x.gguf"]);
    }

    #[test]
    fn tail_returns_only_the_last_n_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.txt");
        let contents: String = (1..=100).map(|n| format!("line {n}\n")).collect();
        std::fs::write(&path, contents).unwrap();

        let tail = tail(&path, 5);
        let lines: Vec<&str> = tail.lines().collect();
        assert_eq!(lines, vec!["line 96", "line 97", "line 98", "line 99", "line 100"]);
    }

    #[test]
    fn tail_of_a_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(tail(&dir.path().join("nope.log"), 10), "");
    }

    #[tokio::test]
    async fn two_concurrent_lock_acquisitions_serialize() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path(), free_port());
        std::fs::create_dir_all(&config.state_dir).unwrap();

        let first = acquire_lock(&config).await.unwrap();

        let config_clone = config.clone();
        let second_attempt = tokio::spawn(async move { acquire_lock(&config_clone).await });
        // Give the second attempt a moment to observe contention rather than a
        // lucky race before the first lock is even held.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!second_attempt.is_finished());

        drop(first);
        let second = second_attempt.await.unwrap().unwrap();
        drop(second);
    }
}

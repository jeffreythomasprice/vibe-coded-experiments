//! Tracing initialization. Logs go to both stderr and the configured log file.
//!
//! Default filter is `ecs=trace,exalted=trace,warn` — meaning our own crates
//! log at TRACE while every other dependency stays at WARN. Override with
//! `RUST_LOG`.
//!
//! Every line is prefixed with `pid=<n>` so multiple concurrent instances of
//! the app sharing one log file can be told apart.

use std::fmt::Result as FmtResult;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use tracing::{Event, Subscriber};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

const DEFAULT_FILTER: &str = "ecs=trace,exalted=trace,warn";

/// Initialize the global tracing subscriber. The returned `WorkerGuard` must
/// be held for the lifetime of the process — when it drops, the background
/// file-writer thread flushes and exits.
///
/// If the existing log file already exceeds `max_size_mb`, it is rotated to
/// `<log_file>.old` (overwriting any prior `.old`) before the new file is
/// opened. The rotate-decide-and-open step is serialized across processes
/// via an advisory lock on `<log_file>.lock`, so two instances starting at
/// the same time can't both rotate and clobber each other's `.old`. There is
/// no in-session rotation.
pub fn init(log_file: &Path, max_size_mb: u64) -> Option<WorkerGuard> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));

    let pid = std::process::id();

    let stderr_layer = fmt::layer()
        .event_format(PidFormat::new(pid, fmt::format().with_ansi(true)))
        .with_writer(std::io::stderr);

    let (file_layer, guard) = match prepare_file_writer(log_file, max_size_mb) {
        Some((nb, guard)) => {
            let layer = fmt::layer()
                .event_format(PidFormat::new(pid, fmt::format().with_ansi(false)))
                .with_writer(nb);
            (Some(layer), Some(guard))
        }
        None => (None, None),
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();

    guard
}

fn prepare_file_writer(log_file: &Path, max_size_mb: u64) -> Option<(NonBlocking, WorkerGuard)> {
    let parent = log_file.parent().unwrap_or_else(|| Path::new("."));
    if let Err(e) = fs::create_dir_all(parent) {
        eprintln!(
            "warning: could not create log directory {}: {} (file logging disabled)",
            parent.display(),
            e
        );
        return None;
    }

    // Hold the rotation lock across both the size check and the rename so
    // a concurrent starter can't squeeze its own rotation in between and
    // clobber our `.old`. Released when `_rotation_lock` drops at end of fn.
    let _rotation_lock = acquire_rotation_lock(log_file);
    rotate_if_oversized(log_file, max_size_mb);

    let filename = log_file.file_name()?.to_owned();
    let appender = tracing_appender::rolling::never(parent, filename);
    Some(tracing_appender::non_blocking(appender))
}

fn rotate_if_oversized(log_file: &Path, max_size_mb: u64) {
    let Ok(meta) = fs::metadata(log_file) else {
        return;
    };
    let max_bytes = max_size_mb.saturating_mul(1_048_576);
    if max_bytes == 0 || meta.len() <= max_bytes {
        return;
    }
    let mut old = log_file.as_os_str().to_owned();
    old.push(".old");
    let old_path = PathBuf::from(old);
    if let Err(e) = fs::rename(log_file, &old_path) {
        eprintln!(
            "warning: could not rotate oversized log file {} → {}: {} (continuing without rotation)",
            log_file.display(),
            old_path.display(),
            e
        );
    }
}

/// Open `<log_file>.lock` and take an exclusive advisory lock on it. The lock
/// is released when the returned `File` is dropped. Returns `None` if either
/// the open or the lock acquisition fails — in that case the caller proceeds
/// without coordination (the rotate may race, same as before this lock
/// existed).
fn acquire_rotation_lock(log_file: &Path) -> Option<File> {
    let mut lock_path = log_file.as_os_str().to_owned();
    lock_path.push(".lock");
    let lock_path = PathBuf::from(lock_path);

    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "warning: could not open log rotation lock {}: {} (rotation may race with concurrent instances)",
                lock_path.display(),
                e
            );
            return None;
        }
    };

    if let Err(e) = file.lock() {
        eprintln!(
            "warning: could not acquire log rotation lock {}: {} (rotation may race with concurrent instances)",
            lock_path.display(),
            e
        );
        return None;
    }

    Some(file)
}

/// Wraps any `FormatEvent` to prefix each line with `pid=<n> `.
struct PidFormat<F> {
    pid: u32,
    inner: F,
}

impl<F> PidFormat<F> {
    fn new(pid: u32, inner: F) -> Self {
        Self { pid, inner }
    }
}

impl<S, N, F> FormatEvent<S, N> for PidFormat<F>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
    F: FormatEvent<S, N>,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> FmtResult {
        write!(writer, "pid={} ", self.pid)?;
        self.inner.format_event(ctx, writer, event)
    }
}

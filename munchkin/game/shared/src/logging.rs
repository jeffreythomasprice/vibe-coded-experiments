//! Process-wide `tracing` setup.
//!
//! Both the engine and the tui call [`init`] once at startup. The logging
//! scheme is:
//!
//! - **One shared log file** ([`Config::log_file`](crate::config::Config::log_file)),
//!   opened in append mode. Multiple processes may write to it at once; on POSIX
//!   each line write to an `O_APPEND` file is atomic for reasonable line
//!   lengths, so lines interleave cleanly without corruption.
//! - **Per-line pid + mode tags.** [`init`] returns a [`LogGuard`] whose
//!   lifetime must span the whole process. While it is alive, a root span
//!   `app{pid=… mode=…}` is entered, so the default formatter prefixes every log
//!   line with the originating process id and mode (`engine`/`tui`).
//! - **Optional stderr mirror.** The engine also logs to stderr; the tui does
//!   not, because it owns the terminal.
//! - **Default filter:** our own crates at `trace`, everything else at `warn`.
//!   Override with the `RUST_LOG` environment variable.

use anyhow::{Context, Result};
use tracing::span::EnteredSpan;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::Config;

/// Which binary is doing the logging. Determines the `mode` tag and whether
/// logs are mirrored to stderr.
#[derive(Clone, Copy, Debug)]
pub enum AppMode {
    Engine,
    Tui,
}

impl AppMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AppMode::Engine => "engine",
            AppMode::Tui => "tui",
        }
    }

    /// The engine mirrors logs to stderr; the tui keeps the terminal clean.
    fn logs_to_stderr(self) -> bool {
        matches!(self, AppMode::Engine)
    }
}

/// Default filter: our crates at `trace`, all other crates at `warn`.
const DEFAULT_FILTER: &str = "warn,shared=trace,engine=trace,tui=trace";

/// Target of the process-wide root span. Pinned on (see [`init`]) regardless of
/// `RUST_LOG` so the pid/mode prefix is never filtered off the log lines.
const ROOT_SPAN_TARGET: &str = "munchkin_app";

/// Keeps logging alive for the lifetime of the process. Drop it (e.g. at the
/// end of `main`) to flush the background writer and exit the root span.
///
/// Hold this in a binding that lives as long as the program — dropping it early
/// stops the pid/mode tagging and may drop buffered log lines.
pub struct LogGuard {
    // Order matters: the entered span is dropped before the worker guard, so
    // the final flush still happens with the subscriber fully in place.
    _span: EnteredSpan,
    _worker: WorkerGuard,
}

/// Initialise `tracing` for this process. See the module docs for the scheme.
pub fn init(config: &Config, mode: AppMode) -> Result<LogGuard> {
    let (dir, file_name) = split_log_path(&config.log_file)?;
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating log directory {}", dir.display()))?;

    // `never` opens the file in append mode and does not roll it, so every
    // process shares the one file.
    let file_appender = tracing_appender::rolling::never(dir, file_name);
    let (file_writer, worker) = tracing_appender::non_blocking(file_appender);

    // Honour RUST_LOG if set, else our default. Either way, force the root
    // span's target always-on so the pid/mode prefix is never filtered out.
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
        .add_directive(
            format!("{ROOT_SPAN_TARGET}=trace")
                .parse()
                .expect("root span filter directive is valid"),
        );

    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_writer(file_writer);

    let stderr_layer = mode
        .logs_to_stderr()
        .then(|| fmt::layer().with_writer(std::io::stderr));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(stderr_layer)
        .try_init()
        .map_err(|e| anyhow::anyhow!("initialising tracing subscriber: {e}"))?;

    // Enter a root span for the whole process. The default formatter renders the
    // active span's fields, so every line is prefixed `app{pid=… mode=…}`.
    let span = tracing::info_span!(
        target: ROOT_SPAN_TARGET,
        "app",
        pid = std::process::id(),
        mode = mode.as_str(),
    )
    .entered();

    Ok(LogGuard {
        _span: span,
        _worker: worker,
    })
}

/// Split a log file path into its parent directory and file name, defaulting the
/// directory to `.` when the path has no parent.
fn split_log_path(log_file: &std::path::Path) -> Result<(&std::path::Path, &std::ffi::OsStr)> {
    let file_name = log_file
        .file_name()
        .with_context(|| format!("log_file {} has no file name", log_file.display()))?;
    let dir = log_file.parent().unwrap_or_else(|| std::path::Path::new("."));
    Ok((dir, file_name))
}

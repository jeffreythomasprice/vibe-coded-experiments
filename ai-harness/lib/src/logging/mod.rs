//! `tracing` setup: one filter, two sinks (stderr and a rotating file).
//!
//! Every crate in the workspace logs through the `tracing` macros; this module
//! is the only place a subscriber is installed. Call [`init`] exactly once,
//! early in `main` — as soon as the config that configures it has been read.

mod rotate;

use std::io;
use std::sync::Mutex;

use anyhow::Context;
use time::macros::format_description;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

pub use rotate::ACTIVE_LOG;

use crate::config::LogConfig;

/// ISO8601, UTC, fixed at three fractional digits.
///
/// Fixed-width rather than RFC3339's variable precision, so the timestamp
/// column stays aligned and greppable.
macro_rules! timestamp_format {
    () => {
        format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z")
    };
}

/// Install the global subscriber.
///
/// Filter precedence is `RUST_LOG`, then `cfg.filter` (which itself defaults to
/// our crates at TRACE and everything else at WARN).
///
/// Writes are synchronous and unbuffered — deliberately. The obvious
/// alternative, `tracing_appender::non_blocking`, hands back a `WorkerGuard`
/// that flushes on drop, but `tauri::App::run` never returns (it ends the
/// process with `std::process::exit`), so that guard would never drop and every
/// session would silently lose its tail. `Mutex<W>` implements `MakeWriter`,
/// which also serializes concurrent writes so lines never interleave.
pub fn init(cfg: &LogConfig) -> anyhow::Result<()> {
    let writer = rotate::RotatingWriter::new(&cfg.dir, &cfg.rotation)
        .with_context(|| format!("opening log file in {}", cfg.dir.display()))?;

    let filter = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new(&cfg.filter))?;

    // `with_target` gives the module path (e.g. `server::config`); `with_file`
    // and `with_line_number` give the originating source location.
    let stderr_layer = fmt::layer()
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_timer(UtcTime::new(timestamp_format!()))
        .with_writer(io::stderr);

    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_timer(UtcTime::new(timestamp_format!()))
        .with_writer(Mutex::new(writer));

    tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer)
        .with(file_layer)
        .try_init()
        .context("installing the tracing subscriber (already initialized?)")?;

    Ok(())
}

/// Re-emit a log event forwarded from the wasm frontend.
///
/// `tracing`'s macros bake their metadata in statically, so a forwarded event
/// cannot carry the client's module path or source location in the usual
/// positions. Instead the target is the literal `client` and the real origin
/// rides along as `client.*` fields — so when reading a `client` line, the
/// `file:line` shown is *this* function, and `client.file` / `client.line` are
/// the ones you want.
pub fn log_client_record(rec: &shared::ClientLogRecord) {
    use shared::LogLevel;

    macro_rules! emit {
        ($level:expr) => {
            tracing::event!(
                target: "client",
                $level,
                client.time = %rec.timestamp,
                client.target = %rec.target,
                client.file = %rec.file.as_deref().unwrap_or("?"),
                client.line = rec.line.unwrap_or(0),
                "{}",
                rec.message,
            )
        };
    }

    match rec.level {
        LogLevel::Trace => emit!(tracing::Level::TRACE),
        LogLevel::Debug => emit!(tracing::Level::DEBUG),
        LogLevel::Info => emit!(tracing::Level::INFO),
        LogLevel::Warn => emit!(tracing::Level::WARN),
        LogLevel::Error => emit!(tracing::Level::ERROR),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Interval, RotationConfig};

    /// Drives real `tracing` events through the real formatter into the real
    /// rotating writer, with a cap small enough to rotate many times. Covers
    /// what the `rotate` unit tests can't: that a formatted event survives
    /// rotation intact.
    #[test]
    fn events_rotate_without_splitting_a_line() {
        let dir = std::env::temp_dir().join(format!("ai-harness-fmt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let cfg = RotationConfig {
            interval: Interval::Never,
            max_size: 2_000,
            keep: 3,
        };
        let writer = rotate::RotatingWriter::new(&dir, &cfg).unwrap();
        let layer = fmt::layer()
            .with_ansi(false)
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .with_timer(UtcTime::new(timestamp_format!()))
            .with_writer(Mutex::new(writer));
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::subscriber::with_default(subscriber, || {
            for i in 0..200 {
                tracing::info!(
                    iteration = i,
                    "a log line long enough to fill the cap quickly"
                );
            }
        });

        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        files.sort();
        assert!(files.len() > 1, "expected rotations, got {files:?}");
        // `keep` rotated files plus the active one.
        assert!(files.len() <= cfg.keep + 1, "kept too many: {files:?}");

        let mut total_lines = 0;
        for path in &files {
            let body = std::fs::read_to_string(path).unwrap();
            for line in body.lines() {
                total_lines += 1;
                // Every line is a whole event: an ISO8601 timestamp, then the
                // target and source location. A line split across a rotation
                // boundary would fail the timestamp check.
                assert!(
                    line.as_bytes()[..4].iter().all(u8::is_ascii_digit) && &line[4..5] == "-",
                    "line does not start with a timestamp in {}: {line:?}",
                    path.display()
                );
                assert!(
                    line.contains("lib::logging::tests:"),
                    "missing target: {line:?}"
                );
                assert!(
                    line.contains("lib/src/logging/mod.rs:"),
                    "missing file: {line:?}"
                );
                assert!(line.contains("iteration="), "missing field: {line:?}");
            }
        }
        // Older events were pruned, so we expect fewer than we emitted — but
        // every line that survived must be intact.
        assert!(total_lines > 0);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

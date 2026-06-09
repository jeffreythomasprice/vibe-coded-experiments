use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Name of the active log file inside the configured log directory.
const ACTIVE_LOG: &str = "server.log";
/// Rotate when the active file would exceed this many bytes (10 MiB).
const MAX_BYTES: u64 = 10 * 1024 * 1024;
/// Number of rotated files to retain.
const KEEP: usize = 30;
const SECONDS_PER_DAY: u64 = 86_400;

/// Keeps the non-blocking writer's worker thread alive. Must be held for the
/// whole process; on drop it flushes any buffered log lines.
pub struct LogGuard(#[allow(dead_code)] WorkerGuard);

/// Initialize `tracing` with two layers — human-readable to stderr and plain
/// text to a rotating file in `log_dir` — both gated by a single `EnvFilter`
/// (`RUST_LOG`, or our default of WARN with `server`/`shared` at TRACE).
pub fn init(log_dir: &Path) -> anyhow::Result<LogGuard> {
    let writer = RollingWriter::new(log_dir, MAX_BYTES, KEEP)
        .with_context(|| format!("opening log file in {}", log_dir.display()))?;
    let (file_writer, guard) = tracing_appender::non_blocking(writer);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,server=trace,shared=trace"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(io::stderr))
        .with(fmt::layer().with_ansi(false).with_writer(file_writer))
        .init();

    Ok(LogGuard(guard))
}

/// A `std::io::Write` sink that rotates the active log file when it would exceed
/// `max_bytes` *or* when the local day changes — whichever comes first — keeping
/// at most `keep` rotated files.
struct RollingWriter {
    inner: Mutex<Inner>,
}

struct Inner {
    dir: PathBuf,
    active: PathBuf,
    file: File,
    written: u64,
    day: u64,
    max_bytes: u64,
    keep: usize,
}

impl RollingWriter {
    fn new(dir: &Path, max_bytes: u64, keep: usize) -> io::Result<Self> {
        let active = dir.join(ACTIVE_LOG);
        let file = OpenOptions::new().create(true).append(true).open(&active)?;
        let written = file.metadata()?.len();
        Ok(Self {
            inner: Mutex::new(Inner {
                dir: dir.to_path_buf(),
                active,
                file,
                written,
                day: today(),
                max_bytes,
                keep,
            }),
        })
    }
}

impl Inner {
    /// Move the active file aside (suffixed with the current unix-millis), open a
    /// fresh active file, and prune old rotations.
    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let rotated = self.dir.join(format!("{ACTIVE_LOG}.{}", now_millis()));
        std::fs::rename(&self.active, &rotated)?;
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.active)?;
        self.written = 0;
        self.day = today();
        self.prune();
        Ok(())
    }

    /// Delete the oldest `server.log.*` files until at most `keep` remain. The
    /// unix-millis suffix sorts chronologically as a string.
    fn prune(&self) {
        let prefix = format!("{ACTIVE_LOG}.");
        let mut rotated: Vec<PathBuf> = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(&prefix))
                })
                .collect(),
            Err(_) => return,
        };
        rotated.sort();
        if rotated.len() > self.keep {
            for old in &rotated[..rotated.len() - self.keep] {
                let _ = std::fs::remove_file(old);
            }
        }
    }
}

impl Write for RollingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if today() != inner.day || inner.written + buf.len() as u64 > inner.max_bytes {
            inner.rotate()?;
        }
        let n = inner.file.write(buf)?;
        inner.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.file.flush()
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn today() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / SECONDS_PER_DAY)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_rotated(dir: &Path) -> usize {
        let prefix = format!("{ACTIVE_LOG}.");
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .count()
    }

    #[test]
    fn rotates_on_size_and_keeps_only_n() {
        let dir = std::env::temp_dir().join(format!("roll20-rwtest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 32-byte cap, keep 3 rotations. Each 20-byte write that would exceed the
        // cap forces a rotation first.
        let mut w = RollingWriter::new(&dir, 32, 3).unwrap();
        for _ in 0..10 {
            w.write_all(b"01234567890123456789").unwrap(); // 20 bytes
            // Distinct millis suffixes per rotation.
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        w.flush().unwrap();

        // The active file always exists; rotations are pruned to at most `keep`.
        assert!(dir.join(ACTIVE_LOG).exists());
        assert!(count_rotated(&dir) <= 3, "kept more than 3 rotated files");
        assert!(count_rotated(&dir) >= 1, "expected at least one rotation");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

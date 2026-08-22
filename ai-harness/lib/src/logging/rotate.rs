//! A rotating log-file sink.
//!
//! `tracing-appender`'s own appender rotates on time only, so this is a small
//! hand-rolled `std::io::Write` that rotates on **either** a size cap or an
//! interval boundary — whichever comes first — and prunes old files.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use time::macros::format_description;
use time::OffsetDateTime;

use crate::config::{Interval, RotationConfig};

/// Basename of the file currently being written. Stable, so `tail -f` on it
/// always follows the live log.
pub const ACTIVE_LOG: &str = "ai-harness.log";
/// Rotated files are `ai-harness-<timestamp>.log`.
const ROTATED_PREFIX: &str = "ai-harness-";
const ROTATED_SUFFIX: &str = ".log";

const SECONDS_PER_HOUR: u64 = 3_600;
const SECONDS_PER_DAY: u64 = 86_400;
const SECONDS_PER_WEEK: u64 = SECONDS_PER_DAY * 7;

/// Which interval bucket `now_secs` falls into. Rotation is due when this
/// changes. `Never` pins to a constant, reducing the check to size-only.
fn period_index(interval: Interval, now_secs: u64) -> u64 {
    match interval {
        Interval::Hourly => now_secs / SECONDS_PER_HOUR,
        Interval::Daily => now_secs / SECONDS_PER_DAY,
        Interval::Weekly => now_secs / SECONDS_PER_WEEK,
        Interval::Never => 0,
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// ISO8601 *basic* format (no separators) so the name is filename-safe on every
/// platform — notably Windows, which forbids `:` — while still sorting
/// chronologically as a plain string, which is what makes pruning a sort.
fn rotation_stamp() -> String {
    let fmt = format_description!("[year][month][day]T[hour][minute][second][subsecond digits:3]Z");
    OffsetDateTime::now_utc()
        .format(&fmt)
        .unwrap_or_else(|_| now_secs().to_string())
}

/// Appends to `<dir>/ai-harness.log`, rotating it aside when it would exceed
/// `max_bytes` or when the interval boundary passes, and keeping at most `keep`
/// rotated files.
pub struct RotatingWriter {
    dir: PathBuf,
    active: PathBuf,
    file: File,
    written: u64,
    period: u64,
    interval: Interval,
    max_bytes: u64,
    keep: usize,
}

impl RotatingWriter {
    /// Create `dir` if needed and open (or re-open, appending to) the active file.
    pub fn new(dir: &Path, cfg: &RotationConfig) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let active = dir.join(ACTIVE_LOG);
        let file = OpenOptions::new().create(true).append(true).open(&active)?;
        let written = file.metadata()?.len();
        let writer = Self {
            dir: dir.to_path_buf(),
            active,
            file,
            written,
            period: period_index(cfg.interval, now_secs()),
            interval: cfg.interval,
            max_bytes: cfg.max_size,
            keep: cfg.keep,
        };
        // Apply `keep` immediately, so lowering it in the config takes effect on
        // the next start rather than only after the next rotation.
        writer.prune();
        Ok(writer)
    }

    /// Rotation is due if the interval rolled over, or if this write would push
    /// the active file past the cap. An empty active file is never rotated —
    /// otherwise a single write larger than the cap would rotate on every call.
    fn should_rotate(&self, incoming: usize, now: u64) -> bool {
        if self.written == 0 {
            return false;
        }
        period_index(self.interval, now) != self.period
            || self.written + incoming as u64 > self.max_bytes
    }

    /// Move the active file aside under a rotation-stamped name, open a fresh
    /// active file, and prune.
    fn rotate(&mut self, now: u64) -> io::Result<()> {
        self.file.flush()?;
        std::fs::rename(&self.active, self.rotated_path())?;
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.active)?;
        self.written = 0;
        self.period = period_index(self.interval, now);
        self.prune();
        Ok(())
    }

    /// A rotated path that doesn't already exist. Two rotations inside the same
    /// millisecond would otherwise collide and silently clobber a file.
    ///
    /// The disambiguator is `_`, not `-`, so that a collision name still sorts
    /// *after* the unsuffixed one: `_` (0x5F) is greater than the `.` (0x2E)
    /// that starts the extension, whereas `-` (0x2D) is less. `prune` relies on
    /// the name order being chronological.
    fn rotated_path(&self) -> PathBuf {
        let stamp = rotation_stamp();
        let path = self
            .dir
            .join(format!("{ROTATED_PREFIX}{stamp}{ROTATED_SUFFIX}"));
        if !path.exists() {
            return path;
        }
        for n in 1..1000 {
            let path = self
                .dir
                .join(format!("{ROTATED_PREFIX}{stamp}_{n}{ROTATED_SUFFIX}"));
            if !path.exists() {
                return path;
            }
        }
        path
    }

    /// Delete the oldest rotated files until at most `keep` remain. Names sort
    /// chronologically, so this is a sort and a truncate.
    fn prune(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut rotated: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(ROTATED_PREFIX) && n.ends_with(ROTATED_SUFFIX))
            })
            .collect();
        rotated.sort();
        if rotated.len() > self.keep {
            for old in &rotated[..rotated.len() - self.keep] {
                let _ = std::fs::remove_file(old);
            }
        }
    }

    /// `write`, with the clock injected so interval rotation is testable.
    ///
    /// Writes `buf` in full and reports it as one write. The fmt layer hands us
    /// exactly one formatted event per call and wraps it in `write_all`; if we
    /// returned a short count, `write_all` would re-enter with the remainder and
    /// a rotation could land mid-event, splitting one log line across two files.
    fn write_at(&mut self, buf: &[u8], now: u64) -> io::Result<usize> {
        if self.should_rotate(buf.len(), now) {
            self.rotate(now)?;
        }
        self.file.write_all(buf)?;
        self.written += buf.len() as u64;
        Ok(buf.len())
    }
}

impl Write for RotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.write_at(buf, now_secs())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp dir that cleans itself up, so a failing assert doesn't leak files.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "ai-harness-rotate-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn count_rotated(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n.starts_with(ROTATED_PREFIX) && n.ends_with(ROTATED_SUFFIX)
            })
            .count()
    }

    fn cfg(interval: Interval, max_size: u64, keep: usize) -> RotationConfig {
        RotationConfig {
            interval,
            max_size,
            keep,
        }
    }

    #[test]
    fn rotates_on_size_and_prunes_to_keep() {
        let dir = TempDir::new("size");
        // 32-byte cap, keep 3. Each 20-byte write that would exceed the cap
        // forces a rotation first.
        let mut w = RotatingWriter::new(dir.path(), &cfg(Interval::Never, 32, 3)).unwrap();
        for _ in 0..10 {
            w.write_all(b"01234567890123456789").unwrap();
        }
        w.flush().unwrap();

        assert!(dir.path().join(ACTIVE_LOG).exists(), "active file missing");
        assert!(count_rotated(dir.path()) >= 1, "expected a rotation");
        assert!(
            count_rotated(dir.path()) <= 3,
            "kept more than `keep` files"
        );
    }

    #[test]
    fn rotates_when_the_interval_rolls_over() {
        let dir = TempDir::new("interval");
        // Huge size cap, so only the clock can trigger rotation.
        let mut w = RotatingWriter::new(dir.path(), &cfg(Interval::Daily, u64::MAX, 10)).unwrap();

        let day1 = 0;
        w.write_at(b"first day\n", day1).unwrap();
        assert_eq!(count_rotated(dir.path()), 0, "rotated without a roll-over");

        w.write_at(b"second day\n", day1 + SECONDS_PER_DAY).unwrap();
        assert_eq!(count_rotated(dir.path()), 1, "no rotation on day change");
    }

    #[test]
    fn never_interval_ignores_the_clock() {
        let dir = TempDir::new("never");
        let mut w = RotatingWriter::new(dir.path(), &cfg(Interval::Never, u64::MAX, 10)).unwrap();
        w.write_at(b"a\n", 0).unwrap();
        w.write_at(b"b\n", SECONDS_PER_WEEK * 52).unwrap();
        assert_eq!(count_rotated(dir.path()), 0);
    }

    #[test]
    fn a_single_write_over_the_cap_does_not_spin() {
        let dir = TempDir::new("oversize");
        let mut w = RotatingWriter::new(dir.path(), &cfg(Interval::Never, 4, 5)).unwrap();
        // Every one of these exceeds the 4-byte cap on its own. We should get
        // one rotation per write, not an infinite loop or a rotation of an
        // empty file.
        for _ in 0..3 {
            w.write_all(b"way over the cap").unwrap();
        }
        assert!(count_rotated(dir.path()) <= 3);
        assert!(dir.path().join(ACTIVE_LOG).exists());
    }

    #[test]
    fn reopening_appends_rather_than_truncating() {
        let dir = TempDir::new("reopen");
        {
            let mut w = RotatingWriter::new(dir.path(), &cfg(Interval::Never, 1_000, 5)).unwrap();
            w.write_all(b"first\n").unwrap();
            w.flush().unwrap();
        }
        {
            let mut w = RotatingWriter::new(dir.path(), &cfg(Interval::Never, 1_000, 5)).unwrap();
            w.write_all(b"second\n").unwrap();
            w.flush().unwrap();
        }
        let body = std::fs::read_to_string(dir.path().join(ACTIVE_LOG)).unwrap();
        assert_eq!(body, "first\nsecond\n");
    }

    #[test]
    fn rotation_stamp_is_filename_safe_and_sortable() {
        let stamp = rotation_stamp();
        assert!(!stamp.contains(':'), "colons break Windows filenames");
        // YYYYMMDD T HHMMSS mmm Z  =  8 + 1 + 6 + 3 + 1
        assert_eq!(stamp.len(), 19, "unexpected stamp shape: {stamp}");
        assert!(stamp.ends_with('Z'));
        assert_eq!(&stamp[8..9], "T");
    }

    #[test]
    fn same_millisecond_collisions_still_sort_chronologically() {
        let dir = TempDir::new("collide");
        let w = RotatingWriter::new(dir.path(), &cfg(Interval::Never, 1_000, 5)).unwrap();

        // Force a collision: claim the first name, then ask for another.
        let first = w.rotated_path();
        std::fs::write(&first, b"").unwrap();
        let second = w.rotated_path();
        assert_ne!(first, second, "collision returned the same path");

        // `prune` deletes the lexicographically smallest first, so the later
        // file must sort after the earlier one.
        assert!(
            first.file_name().unwrap() < second.file_name().unwrap(),
            "collision name {second:?} sorts before {first:?} — prune would \
             delete the newer file first"
        );
    }

    #[test]
    fn period_index_buckets_by_interval() {
        assert_eq!(period_index(Interval::Hourly, 0), 0);
        assert_eq!(period_index(Interval::Hourly, SECONDS_PER_HOUR), 1);
        assert_eq!(period_index(Interval::Daily, SECONDS_PER_DAY - 1), 0);
        assert_eq!(period_index(Interval::Daily, SECONDS_PER_DAY), 1);
        assert_eq!(period_index(Interval::Weekly, SECONDS_PER_WEEK), 1);
        assert_eq!(period_index(Interval::Never, u64::MAX), 0);
    }
}

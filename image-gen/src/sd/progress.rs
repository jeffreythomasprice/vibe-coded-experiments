use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

const TRACING_TARGET: &str = "image_gen::sd";

/// Reconstructs the per-step progress bar for one generation by tailing
/// `sd-server`'s log file for new output written after `offset` — the file's
/// length recorded right before the request this display is for was
/// submitted. `sd-server` prints its own step counter as a raw, non-leveled
/// `printf` (verified against `print_progress_line` in
/// stable-diffusion.cpp), redrawn in place with `\r`, so it never overlaps
/// with a `[LEVEL]`-prefixed log line; everything that isn't a progress
/// update is forwarded into `tracing` under the `image_gen::sd` target,
/// mirroring the old FFI-callback-based `sdlog.rs`.
///
/// Correlation with a specific request is best-effort: since the daemon's
/// stdout is shared across every generation that process ever serves, a
/// second concurrent invocation's output would be interleaved into the same
/// stream. A single-user CLI runs one generation at a time in practice, so
/// this is not handled beyond tailing only from this call's own offset.
pub struct Tailer {
    stop: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl Tailer {
    /// Stops the tailer and waits for its last poll to finish, so the bar (if
    /// any) is cleared before the caller prints anything else.
    pub async fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.task.await;
    }
}

/// The offset to start tailing from for a new request: the log file's current
/// length, or 0 if it doesn't exist yet (a daemon that was just spawned may
/// not have flushed its first write). Read this *before* submitting the
/// request it corresponds to.
pub async fn current_offset(log_path: &Path) -> u64 {
    tokio::fs::metadata(log_path).await.map(|m| m.len()).unwrap_or(0)
}

/// Starts tailing `log_path` from `offset` in the background. Best-effort and
/// never fatal: any I/O error while tailing just ends the polling loop
/// quietly, since a lost progress display must never fail a generation that
/// otherwise succeeded.
pub fn spawn(log_path: PathBuf, offset: u64, poll_interval: Duration) -> Tailer {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_for_task = Arc::clone(&stop);

    let task = tokio::spawn(async move {
        let bar = ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::stderr());
        bar.set_style(
            ProgressStyle::with_template("  [{bar:50}] {pos}/{len} - {msg}")
                .expect("static template is always valid")
                .progress_chars("=> "),
        );
        let mut bar_started = false;
        let mut offset = offset;

        while !stop_for_task.load(Ordering::Relaxed) {
            if let Ok((bytes, new_offset)) = read_new(&log_path, offset).await
                && new_offset > offset
            {
                offset = new_offset;
                for segment in split_segments(&bytes) {
                    if let Some(progress) = parse_progress_line(&segment) {
                        if !bar_started {
                            bar.set_length(progress.total);
                            bar_started = true;
                        }
                        bar.set_position(progress.step);
                        bar.set_message(format!("{:.2}{}", progress.rate, progress.unit));
                    } else if let Some((level, message)) = classify_log_line(&segment) {
                        emit(level, message);
                    } else if !segment.trim().is_empty() {
                        tracing::debug!(target: TRACING_TARGET, "{}", segment.trim());
                    }
                }
            }
            tokio::time::sleep(poll_interval).await;
        }

        if bar_started {
            bar.finish_and_clear();
        }
    });

    Tailer { stop, task }
}

async fn read_new(path: &Path, offset: u64) -> std::io::Result<(Vec<u8>, u64)> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = tokio::fs::File::open(path).await?;
    let len = file.metadata().await?.len();
    if len <= offset {
        return Ok((Vec::new(), offset));
    }
    file.seek(std::io::SeekFrom::Start(offset)).await?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).await?;
    let new_offset = offset + buf.len() as u64;
    Ok((buf, new_offset))
}

/// Splits raw output on `\r` and `\n` alike, since the server's progress bar
/// redraws in place with `\r` and only emits a trailing `\n` on the final
/// step. A boundary landing mid multi-byte UTF-8 sequence (possible if a poll
/// lands mid-write) degrades to a replacement character in that one segment,
/// not a parse failure — acceptable for a display that's best-effort already.
fn split_segments(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .split(['\r', '\n'])
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

struct ProgressLine {
    step: u64,
    total: u64,
    rate: f64,
    unit: &'static str,
}

/// Parses a line shaped like `|===...>   | 5/20 - 2.34it/s`, stripping the
/// trailing `\x1b[K` (erase-to-end-of-line) `print_progress_line` appends to
/// every redraw. Returns `None` for anything else, including log lines.
fn parse_progress_line(segment: &str) -> Option<ProgressLine> {
    let trimmed = segment.trim();
    let trimmed = trimmed.strip_suffix("\x1b[K").unwrap_or(trimmed).trim();

    let (left, rate_text) = trimmed.rsplit_once(" - ")?;
    let counts = left.rsplit(char::is_whitespace).next()?;
    let (step, total) = counts.split_once('/')?;
    let step: u64 = step.parse().ok()?;
    let total: u64 = total.parse().ok()?;

    let (rate, unit) = if let Some(number) = rate_text.strip_suffix("it/s") {
        (number, "it/s")
    } else {
        let number = rate_text.strip_suffix("s/it")?;
        (number, "s/it")
    };
    let rate: f64 = rate.parse().ok()?;

    Some(ProgressLine { step, total, rate, unit })
}

/// Parses a line shaped like `[INFO   ] model_loader.cpp:227  - message`, the
/// exact format `log_print` in stable-diffusion.cpp's `examples/common/log.cpp`
/// writes with `color` disabled (the default `sd-server` runs with).
fn classify_log_line(line: &str) -> Option<(tracing::Level, &str)> {
    let rest = line.trim_start().strip_prefix('[')?;
    let (level_str, rest) = rest.split_once(']')?;
    let level = match level_str.trim() {
        "DEBUG" => tracing::Level::DEBUG,
        "VERBOSE" => tracing::Level::TRACE,
        "INFO" => tracing::Level::INFO,
        "WARN" => tracing::Level::WARN,
        "ERROR" => tracing::Level::ERROR,
        _ => return None,
    };
    Some((level, rest.trim_start()))
}

fn emit(level: tracing::Level, message: &str) {
    match level {
        tracing::Level::ERROR => tracing::error!(target: TRACING_TARGET, "{message}"),
        tracing::Level::WARN => tracing::warn!(target: TRACING_TARGET, "{message}"),
        tracing::Level::INFO => tracing::info!(target: TRACING_TARGET, "{message}"),
        tracing::Level::DEBUG => tracing::debug!(target: TRACING_TARGET, "{message}"),
        tracing::Level::TRACE => tracing::trace!(target: TRACING_TARGET, "{message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_it_per_second_progress_line() {
        let line = parse_progress_line("|===>      | 5/20 - 2.34it/s\x1b[K").unwrap();
        assert_eq!(line.step, 5);
        assert_eq!(line.total, 20);
        assert!((line.rate - 2.34).abs() < 1e-9);
        assert_eq!(line.unit, "it/s");
    }

    #[test]
    fn parses_a_seconds_per_it_progress_line_without_the_escape_suffix() {
        let line = parse_progress_line("|=====|  20/20 - 0.87s/it").unwrap();
        assert_eq!(line.step, 20);
        assert_eq!(line.total, 20);
        assert!((line.rate - 0.87).abs() < 1e-9);
        assert_eq!(line.unit, "s/it");
    }

    #[test]
    fn non_progress_text_does_not_parse() {
        assert!(parse_progress_line("[INFO   ] model_loader.cpp:227  - using 24 threads").is_none());
        assert!(parse_progress_line("").is_none());
        assert!(parse_progress_line("just some text").is_none());
    }

    #[test]
    fn classifies_every_known_level() {
        for (raw, expected) in [
            ("[DEBUG  ] a.cpp:1    - x", tracing::Level::DEBUG),
            ("[VERBOSE] a.cpp:1    - x", tracing::Level::TRACE),
            ("[INFO   ] a.cpp:1    - x", tracing::Level::INFO),
            ("[WARN   ] a.cpp:1    - x", tracing::Level::WARN),
            ("[ERROR  ] a.cpp:1    - x", tracing::Level::ERROR),
        ] {
            let (level, message) = classify_log_line(raw).unwrap();
            assert_eq!(level, expected);
            assert_eq!(message, "a.cpp:1    - x");
        }
    }

    #[test]
    fn unrecognized_prefix_is_not_classified() {
        assert!(classify_log_line("|===>  | 5/20 - 2.34it/s").is_none());
        assert!(classify_log_line("no brackets here").is_none());
    }

    #[test]
    fn split_segments_splits_on_cr_and_lf_and_drops_empties() {
        let bytes = b"[INFO   ] a - one\r|===| 1/2 - 1.00it/s\x1b[K\r|====| 2/2 - 1.00it/s\x1b[K\n";
        let segments = split_segments(bytes);
        assert_eq!(
            segments,
            vec![
                "[INFO   ] a - one",
                "|===| 1/2 - 1.00it/s\u{1b}[K",
                "|====| 2/2 - 1.00it/s\u{1b}[K",
            ]
        );
    }

    #[tokio::test]
    async fn current_offset_of_a_missing_file_is_zero() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(current_offset(&dir.path().join("nope.log")).await, 0);
    }

    #[tokio::test]
    async fn spawn_updates_the_bar_and_forwards_logs_then_stops_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("sd-server.log");
        std::fs::write(&log_path, b"[INFO   ] a.cpp:1 - before offset\n").unwrap();

        let offset = current_offset(&log_path).await;
        assert!(offset > 0);

        // Simulate the daemon appending output after the request was submitted.
        {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new().append(true).open(&log_path).unwrap();
            write!(file, "\r|==>  | 1/2 - 1.00it/s\x1b[K").unwrap();
            write!(file, "\r|====| 2/2 - 0.50it/s\x1b[K\n").unwrap();
            writeln!(file, "[INFO   ] a.cpp:2 - done").unwrap();
        }

        let tailer = spawn(log_path, offset, Duration::from_millis(10));
        tokio::time::sleep(Duration::from_millis(100)).await;
        tailer.stop().await;
        // No assertion beyond "this completes": the bar and tracing output are
        // side effects on stderr, not observable return values. The parsing
        // logic itself is covered by the unit tests above; this proves the
        // polling loop drives them without panicking or hanging.
    }
}

use std::fs::{DirEntry, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{Datelike, NaiveDate, Utc};
use tracing_subscriber::fmt::MakeWriter;

use crate::config::Config;
use crate::logging::LogError;

const PREFIX: &str = "diffusion-";
const SUFFIX: &str = ".log";

struct Sink {
    dir: PathBuf,
    max_bytes: u64,
    keep: usize,
    file: Option<File>,
    path: PathBuf,
    date: NaiveDate,
    index: u32,
    written: u64,
}

impl Sink {
    fn open(dir: &Path, max_bytes: u64, keep: usize) -> Result<Self, LogError> {
        if max_bytes == 0 {
            return Err(LogError::InvalidLimit {
                key: "log_max_bytes",
            });
        }
        if keep == 0 {
            return Err(LogError::InvalidLimit {
                key: "log_max_files",
            });
        }

        std::fs::create_dir_all(dir).map_err(|source| LogError::CreateDir {
            path: dir.to_path_buf(),
            source,
        })?;

        let date = Utc::now().date_naive();
        let mut sink = Sink {
            dir: dir.to_path_buf(),
            max_bytes,
            keep,
            file: None,
            path: PathBuf::new(),
            date,
            index: 0,
            written: 0,
        };
        sink.open_from(date, latest_index(dir, date))?;
        prune(&sink.dir, sink.keep, &sink.path);
        Ok(sink)
    }

    fn open_from(&mut self, date: NaiveDate, mut index: u32) -> Result<(), LogError> {
        loop {
            let path = self.dir.join(file_name(date, index));
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .map_err(|source| LogError::OpenFile {
                    path: path.clone(),
                    source,
                })?;
            let written = file.metadata().map(|meta| meta.len()).unwrap_or(0);

            if written < self.max_bytes || index == u32::MAX {
                self.file = Some(file);
                self.path = path;
                self.date = date;
                self.index = index;
                self.written = written;
                return Ok(());
            }
            index += 1;
        }
    }

    fn reopen(&mut self, date: NaiveDate, index: u32) -> bool {
        match self.open_from(date, index) {
            Ok(()) => {
                prune(&self.dir, self.keep, &self.path);
                true
            }
            Err(err) => {
                self.disable(&err);
                false
            }
        }
    }

    fn write_event(&mut self, today: NaiveDate, buf: &[u8]) {
        if self.file.is_none() {
            return;
        }

        if today != self.date {
            let index = latest_index(&self.dir, today);
            if !self.reopen(today, index) {
                return;
            }
        } else if self.written > 0
            && self.written.saturating_add(buf.len() as u64) > self.max_bytes
            && !self.reopen(self.date, self.index.saturating_add(1))
        {
            return;
        }

        let Some(file) = self.file.as_mut() else {
            return;
        };
        match file.write_all(buf) {
            Ok(()) => self.written = self.written.saturating_add(buf.len() as u64),
            Err(source) => self.disable(&source),
        }
    }

    fn disable(&mut self, err: &dyn std::fmt::Display) {
        self.file = None;
        eprintln!("warning: file logging disabled: {err}");
    }
}

fn file_name(date: NaiveDate, index: u32) -> String {
    format!(
        "{PREFIX}{:04}-{:02}-{:02}.{index:03}{SUFFIX}",
        date.year(),
        date.month(),
        date.day()
    )
}

fn parse_name(name: &str) -> Option<(NaiveDate, u32)> {
    let rest = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    let (date, index) = rest.split_once('.')?;

    let mut parts = date.splitn(3, '-');
    let year = fixed_digits(parts.next()?, 4)?;
    let month = fixed_digits(parts.next()?, 2)?;
    let day = fixed_digits(parts.next()?, 2)?;
    if parts.next().is_some() {
        return None;
    }

    if index.len() < 3 || !index.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    Some((
        NaiveDate::from_ymd_opt(year as i32, month, day)?,
        index.parse().ok()?,
    ))
}

fn fixed_digits(text: &str, width: usize) -> Option<u32> {
    if text.len() != width || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn latest_index(dir: &Path, date: NaiveDate) -> u32 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|entry| parse_name(entry.file_name().to_str()?))
        .filter(|(found, _)| *found == date)
        .map(|(_, index)| index)
        .max()
        .unwrap_or(0)
}

fn prune(dir: &Path, keep: usize, current: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    let is_log_file = |entry: &DirEntry| entry.file_type().is_ok_and(|kind| kind.is_file());
    let mut logs: Vec<(NaiveDate, u32, PathBuf)> = entries
        .flatten()
        .filter(is_log_file)
        .filter_map(|entry| {
            let (date, index) = parse_name(entry.file_name().to_str()?)?;
            Some((date, index, entry.path()))
        })
        .collect();

    if logs.len() <= keep {
        return;
    }

    logs.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    logs.truncate(logs.len() - keep);
    for (_, _, path) in logs {
        if path != current {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Clone)]
pub struct FileWriter(Arc<Mutex<Sink>>);

impl FileWriter {
    pub fn new(config: &Config) -> Result<Self, LogError> {
        Self::with_limits(&config.log_dir, config.log_max_bytes, config.log_max_files)
    }

    fn with_limits(dir: &Path, max_bytes: u64, keep: usize) -> Result<Self, LogError> {
        Ok(Self(Arc::new(Mutex::new(Sink::open(
            dir, max_bytes, keep,
        )?))))
    }
}

impl Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut sink = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        sink.write_event(Utc::now().date_naive(), buf);
        Ok(buf.len())
    }

    // std::fs::File is unbuffered, so every event is already in the kernel by
    // the time write() returns; there is nothing to flush.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for FileWriter {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn day(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap()
    }

    #[test]
    fn size_limit_starts_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        // Two 20-byte events fit under a 50-byte cap; a third does not, so the
        // check has to be against the incoming buffer, not just "is the file full".
        let mut sink = Sink::open(dir.path(), 50, 15).unwrap();

        sink.write_event(day(19), &[b'a'; 20]);
        sink.write_event(day(19), &[b'b'; 20]);
        sink.write_event(day(19), &[b'c'; 20]);

        assert_eq!(
            names(dir.path()),
            vec![
                "diffusion-2026-09-19.000.log",
                "diffusion-2026-09-19.001.log"
            ]
        );
        let first = std::fs::read(dir.path().join("diffusion-2026-09-19.000.log")).unwrap();
        assert_eq!(first, [vec![b'a'; 20], vec![b'b'; 20]].concat());
        let second = std::fs::read(dir.path().join("diffusion-2026-09-19.001.log")).unwrap();
        assert_eq!(second, vec![b'c'; 20]);
    }

    #[test]
    fn oversized_event_is_not_split() {
        let dir = tempfile::tempdir().unwrap();
        let mut sink = Sink::open(dir.path(), 32, 15).unwrap();

        sink.write_event(day(19), &[b'a'; 100]);

        assert_eq!(names(dir.path()), vec!["diffusion-2026-09-19.000.log"]);
        let contents = std::fs::read(dir.path().join("diffusion-2026-09-19.000.log")).unwrap();
        assert_eq!(contents.len(), 100);
    }

    #[test]
    fn day_change_starts_a_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut sink = Sink::open(dir.path(), 1024, 15).unwrap();

        sink.write_event(day(19), b"one");
        sink.write_event(day(20), b"two");

        assert_eq!(
            names(dir.path()),
            vec![
                "diffusion-2026-09-19.000.log",
                "diffusion-2026-09-20.000.log"
            ]
        );
    }

    #[test]
    fn existing_file_length_is_carried_over() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("diffusion-2026-09-19.000.log"), [0u8; 30]).unwrap();

        let mut sink = Sink::open(dir.path(), 32, 15).unwrap();
        sink.write_event(day(19), &[b'x'; 10]);

        assert_eq!(
            names(dir.path()),
            vec![
                "diffusion-2026-09-19.000.log",
                "diffusion-2026-09-19.001.log"
            ]
        );
        let first = std::fs::read(dir.path().join("diffusion-2026-09-19.000.log")).unwrap();
        assert_eq!(first.len(), 30);
        let second = std::fs::read(dir.path().join("diffusion-2026-09-19.001.log")).unwrap();
        assert_eq!(second, vec![b'x'; 10]);
    }

    #[test]
    fn full_file_at_startup_opens_the_next_index() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("diffusion-2026-09-19.000.log"), [0u8; 40]).unwrap();

        let mut sink = Sink::open(dir.path(), 32, 15).unwrap();
        sink.write_event(day(19), b"x");

        assert_eq!(
            names(dir.path()),
            vec![
                "diffusion-2026-09-19.000.log",
                "diffusion-2026-09-19.001.log"
            ]
        );
        let second = std::fs::read(dir.path().join("diffusion-2026-09-19.001.log")).unwrap();
        assert_eq!(second, b"x");
    }

    #[test]
    fn pruning_keeps_the_newest_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut sink = Sink::open(dir.path(), 1024, 3).unwrap();

        for d in 14..=19 {
            sink.write_event(day(d), b"x");
        }

        assert_eq!(
            names(dir.path()),
            vec![
                "diffusion-2026-09-17.000.log",
                "diffusion-2026-09-18.000.log",
                "diffusion-2026-09-19.000.log",
            ]
        );
    }

    #[test]
    fn pruning_ignores_unrelated_files() {
        let dir = tempfile::tempdir().unwrap();
        let unrelated = [
            "model.safetensors",
            "diffusion.log",
            "diffusion-2026-09-19.log",
            "diffusion-2026-09-19.1.log",
            "diffusion-2026-13-45.000.log",
            "diffusion-2026-09-19.000.log.gz",
            "diffusionista-2026-09-19.000.log",
        ];
        for name in unrelated {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        std::fs::create_dir(dir.path().join("hub")).unwrap();

        let mut sink = Sink::open(dir.path(), 1024, 1).unwrap();
        for d in 14..=19 {
            sink.write_event(day(d), b"x");
        }

        let present = names(dir.path());
        for name in unrelated {
            assert!(present.contains(&name.to_owned()), "{name} was removed");
        }
        assert!(dir.path().join("hub").is_dir());
    }

    #[test]
    fn pruning_keeps_the_open_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("diffusion-2026-09-20.000.log"), b"").unwrap();

        let mut sink = Sink::open(dir.path(), 1024, 1).unwrap();
        sink.write_event(day(19), b"x");

        assert!(dir.path().join("diffusion-2026-09-19.000.log").is_file());
    }

    #[test]
    fn write_errors_disable_the_sink() {
        let dir = tempfile::tempdir().unwrap();
        let mut sink = Sink::open(dir.path(), 1024, 15).unwrap();
        let before = names(dir.path());

        sink.file = None;
        sink.write_event(day(19), b"x");

        assert!(sink.file.is_none());
        assert_eq!(names(dir.path()), before);
    }

    #[test]
    fn zero_limits_are_rejected() {
        let dir = tempfile::tempdir().unwrap();

        assert!(matches!(
            Sink::open(dir.path(), 0, 15),
            Err(LogError::InvalidLimit {
                key: "log_max_bytes"
            })
        ));
        assert!(matches!(
            Sink::open(dir.path(), 1024, 0),
            Err(LogError::InvalidLimit {
                key: "log_max_files"
            })
        ));
    }

    #[test]
    fn parse_name_rejects_near_misses() {
        let cases = [
            ("diffusion-2026-09-19.000.log", Some((day(19), 0))),
            ("diffusion-2026-09-19.042.log", Some((day(19), 42))),
            ("model.safetensors", None),
            ("diffusion.log", None),
            ("diffusion-2026-09-19.log", None),
            ("diffusion-2026-09-19.1.log", None),
            ("diffusion-2026-09-19.000.log.gz", None),
            ("diffusion-2026-13-45.000.log", None),
            ("diffusionista-2026-09-19.000.log", None),
        ];
        for (name, expected) in cases {
            assert_eq!(parse_name(name), expected, "{name}");
        }
    }
}

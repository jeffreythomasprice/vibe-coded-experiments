use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use editor_cmd::results::RecoveryEntry;
use editor_proto::Config;
use serde::{Deserialize, Serialize};

const SUFFIX: &str = ".scene.toml";

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0}")]
    Write(#[from] editor_fmt::FileError),
}

#[derive(Debug, Clone)]
pub struct RecoverySettings {
    pub dir: PathBuf,
    pub autosave: Option<Duration>,
    pub backup: Option<Duration>,
    pub keep: usize,
}

impl RecoverySettings {
    pub fn from_config(config: &Config) -> Self {
        let secs = |s: u64| (s > 0).then(|| Duration::from_secs(s));
        Self {
            dir: config.recovery_dir(),
            autosave: secs(config.recovery.autosave_secs),
            backup: secs(config.recovery.backup_secs).filter(|_| config.recovery.backup_keep > 0),
            keep: config.recovery.backup_keep,
        }
    }

    pub fn autosave_dir(&self) -> PathBuf {
        self.dir.join("autosave")
    }

    pub fn backup_dir(&self, key: &str) -> PathBuf {
        self.dir.join("backups").join(key)
    }

    pub fn autosave_path(&self, id: &str) -> PathBuf {
        self.autosave_dir().join(format!("{id}{SUFFIX}"))
    }
}

#[derive(Serialize, Deserialize)]
struct Sidecar {
    source: Option<String>,
    name: String,
    revision: u64,
    saved_at: u64,
}

pub(crate) fn sidecar_path(autosave: &Path) -> PathBuf {
    let name = autosave
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    autosave.with_file_name(format!("{}.json", name.trim_end_matches(SUFFIX)))
}

pub fn id_of(autosave: &Path) -> String {
    autosave
        .file_name()
        .map(|n| n.to_string_lossy().trim_end_matches(SUFFIX).to_string())
        .unwrap_or_default()
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

pub fn key_of(path: Option<&Path>) -> String {
    match path {
        None => "untitled".into(),
        Some(p) => {
            let stem = p
                .file_name()
                .map(|n| n.to_string_lossy().trim_end_matches(SUFFIX).to_string())
                .unwrap_or_default();
            let stem: String = stem
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            format!("{stem}-{:016x}", fnv(&p.display().to_string()))
        }
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !id.starts_with('.')
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn stamp(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let rem = unix % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// File work decided under the hub lock and done outside it.
#[derive(Debug, Default)]
pub struct Job {
    pub writes: Vec<(PathBuf, String)>,
    pub removes: Vec<PathBuf>,
    pub prune: Vec<(PathBuf, usize)>,
}

impl Job {
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.removes.is_empty() && self.prune.is_empty()
    }

    pub fn run(self) -> Result<(), RecoveryError> {
        for (path, text) in self.writes {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|source| RecoveryError::Io {
                    path: dir.to_path_buf(),
                    source,
                })?;
            }
            editor_fmt::write_atomic(&path, text.as_bytes())?;
        }
        for path in self.removes {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => return Err(RecoveryError::Io { path, source }),
            }
        }
        for (dir, keep) in self.prune {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .map_err(|source| RecoveryError::Io {
                    path: dir.clone(),
                    source,
                })?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.to_string_lossy().ends_with(SUFFIX))
                .collect();
            files.sort();
            let excess = files.len().saturating_sub(keep);
            for old in files.into_iter().take(excess) {
                let _ = std::fs::remove_file(old);
            }
        }
        Ok(())
    }
}

pub fn sidecar_text(source: Option<&Path>, name: &str, revision: u64) -> String {
    serde_json::to_string(&Sidecar {
        source: source.map(|p| p.display().to_string()),
        name: name.to_string(),
        revision,
        saved_at: unix_now(),
    })
    .expect("sidecar serializes")
}

pub fn backup_path(settings: &RecoverySettings, key: &str, unix: u64) -> PathBuf {
    let dir = settings.backup_dir(key);
    let base = stamp(unix);
    let mut path = dir.join(format!("{base}{SUFFIX}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{base}-{n}{SUFFIX}"));
        n += 1;
    }
    path
}

pub fn autosave_files(own: &Path) -> Vec<PathBuf> {
    vec![own.to_path_buf(), sidecar_path(own)]
}

pub fn list(settings: &RecoverySettings, live: &BTreeSet<String>) -> Vec<RecoveryEntry> {
    let Ok(dir) = std::fs::read_dir(settings.autosave_dir()) else {
        return Vec::new();
    };
    let mut entries: Vec<RecoveryEntry> = dir
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(SUFFIX))
        .filter_map(|path| {
            let side: Sidecar =
                serde_json::from_str(&std::fs::read_to_string(sidecar_path(&path)).ok()?).ok()?;
            let source_mtime = side
                .source
                .as_deref()
                .and_then(|s| std::fs::metadata(s).ok())
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let id = id_of(&path);
            Some(RecoveryEntry {
                live: live.contains(&id),
                newer_than_source: source_mtime.is_none_or(|m| side.saved_at >= m),
                id,
                source_path: side.source,
                name: side.name,
                saved_at: side.saved_at,
                revision: side.revision,
            })
        })
        .collect();
    entries.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then_with(|| a.id.cmp(&b.id)));
    entries
}

pub(crate) struct SlotRecovery {
    pub key: String,
    pub own: Option<PathBuf>,
    pub autosaved_rev: Option<u64>,
    pub backup_rev: Option<u64>,
    pub last_autosave: Instant,
    pub last_backup: Instant,
    pub superseded: Vec<PathBuf>,
}

impl SlotRecovery {
    pub fn new(revision: u64) -> Self {
        let now = Instant::now();
        Self {
            key: String::new(),
            own: None,
            autosaved_rev: None,
            backup_rev: Some(revision),
            last_autosave: now,
            last_backup: now,
            superseded: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps_are_utc() {
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(1_700_000_000), "20231114-221320");
        assert_eq!(stamp(951_782_400), "20000229-000000");
    }

    #[test]
    fn keys_are_stable_and_path_specific() {
        let a = key_of(Some(Path::new("/x/a.scene.toml")));
        assert_eq!(a, key_of(Some(Path::new("/x/a.scene.toml"))));
        assert_ne!(a, key_of(Some(Path::new("/y/a.scene.toml"))));
        assert!(a.starts_with("a-"));
        assert_eq!(key_of(None), "untitled");
    }

    #[test]
    fn ids_cannot_escape_the_directory() {
        assert!(valid_id("a-1234.run1-s1"));
        assert!(!valid_id("../x"));
        assert!(!valid_id("a/b"));
        assert!(!valid_id(".hidden"));
        assert!(!valid_id(""));
    }
}

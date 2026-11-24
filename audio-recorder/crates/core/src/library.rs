use std::{
    io,
    path::{Path, PathBuf},
    time::Duration,
};

use audio_recorder_schema::manifest::Manifest;

use crate::session::manifest::{self, ManifestError};

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("failed to read {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("invalid session name {0:?}")]
    BadName(String),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub dir: PathBuf,
    pub name: String,
    pub started_at: String,
    pub duration: Duration,
    pub segment_count: usize,
    pub device_count: usize,
    pub bytes: u64,
    pub recording: bool,
    pub has_transcript: bool,
    pub has_live_transcript: bool,
    pub has_summary: bool,
    pub error: Option<String>,
}

pub fn session_duration(manifest: &Manifest) -> Duration {
    let ns = manifest.segments.iter().filter_map(|s| s.end_offset_ns).max().unwrap_or(0);
    Duration::from_nanos(ns)
}

pub(crate) fn dir_bytes(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .filter_map(Result::ok)
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_bytes(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

pub fn list_sessions(sessions_dir: &Path) -> Result<Vec<SessionSummary>, LibraryError> {
    let entries = match std::fs::read_dir(sessions_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(LibraryError::Io { path: sessions_dir.to_path_buf(), source }),
    };
    let mut out = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let dir = entry.path();
        if !dir.join(manifest::MANIFEST_FILE).is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let recording = manifest::is_recording(&dir);
        let bytes = dir_bytes(&dir);
        let has_transcript = dir.join(crate::transcript::TRANSCRIPT_FILE).is_file();
        let has_live_transcript = crate::transcript::live_exists(&dir);
        let has_summary = crate::summary::exists(&dir);
        let summary = match manifest::load_recovering(&dir) {
            Ok(m) => SessionSummary {
                started_at: m.started_at.clone(),
                duration: session_duration(&m),
                segment_count: m.segments.len(),
                device_count: m.devices.len(),
                dir,
                name,
                bytes,
                recording,
                has_transcript,
                has_live_transcript,
                has_summary,
                error: None,
            },
            Err(e) => SessionSummary {
                dir,
                name,
                started_at: String::new(),
                duration: Duration::ZERO,
                segment_count: 0,
                device_count: 0,
                bytes,
                recording,
                has_transcript,
                has_live_transcript,
                has_summary,
                error: Some(e.to_string()),
            },
        };
        out.push(summary);
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

pub fn session_dir(sessions_dir: &Path, name: &str) -> Result<PathBuf, LibraryError> {
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        return Err(LibraryError::BadName(name.to_string()));
    }
    Ok(sessions_dir.join(name))
}

pub fn load_session(dir: &Path) -> Result<Manifest, LibraryError> {
    Ok(manifest::load_recovering(dir)?)
}

pub fn delete_session(sessions_dir: &Path, name: &str) -> Result<(), LibraryError> {
    let dir = session_dir(sessions_dir, name)?;
    std::fs::remove_dir_all(&dir).map_err(|source| LibraryError::Io { path: dir, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal() {
        let base = Path::new("/x");
        assert!(session_dir(base, "../etc").is_err());
        assert!(session_dir(base, "a/b").is_err());
        assert!(session_dir(base, "..").is_err());
        assert!(session_dir(base, "2026-01-01T00-00-00").is_ok());
    }

    #[test]
    fn lists_newest_first_and_skips_non_sessions() {
        let base = std::env::temp_dir().join(format!("audio-recorder-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for name in ["2026-01-01T00-00-00", "2026-02-01T00-00-00"] {
            let dir = base.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let mut m = manifest::new_manifest("2026-01-01T00:00:00Z".into());
            m.ended_at = Some("2026-01-01T00:00:01Z".into());
            manifest::save(&dir, &m).unwrap();
        }
        std::fs::create_dir_all(base.join("junk")).unwrap();
        let list = list_sessions(&base).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "2026-02-01T00-00-00");
        assert!(list_sessions(&base.join("missing")).unwrap().is_empty());
    }
}

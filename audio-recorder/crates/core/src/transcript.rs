use std::{io, path::{Path, PathBuf}};

use audio_recorder_schema::{SchemaError, from_json_str, to_json_string_pretty, transcript::Transcript};

use crate::atomic;

pub const TRANSCRIPT_FILE: &str = "transcript.json";
pub const LIVE_TRANSCRIPT_FILE: &str = "transcript.live.json";
pub const TRANSCRIPT_VERSION: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum TranscriptError {
    #[error("failed to read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("invalid transcript {}: {source}", path.display())]
    Invalid { path: PathBuf, source: SchemaError },
    #[error("unsupported transcript version {version} in {}", path.display())]
    UnsupportedVersion { path: PathBuf, version: i64 },
    #[error("could not serialize transcript: {0}")]
    Serialize(#[from] SchemaError),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderOptions {
    pub source: bool,
    pub speaker: bool,
    pub timestamps: bool,
}

pub fn exists(dir: &Path) -> bool {
    dir.join(TRANSCRIPT_FILE).is_file()
}

pub fn live_exists(dir: &Path) -> bool {
    dir.join(LIVE_TRANSCRIPT_FILE).is_file()
}

fn load_file(dir: &Path, file: &str) -> Result<Option<Transcript>, TranscriptError> {
    let path = dir.join(file);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(TranscriptError::Read { path, source }),
    };
    let transcript: Transcript = from_json_str(&text).map_err(|source| TranscriptError::Invalid { path: path.clone(), source })?;
    if transcript.version != TRANSCRIPT_VERSION {
        return Err(TranscriptError::UnsupportedVersion { path, version: transcript.version });
    }
    Ok(Some(transcript))
}

fn save_file(dir: &Path, file: &str, transcript: &Transcript) -> Result<(), TranscriptError> {
    let path = dir.join(file);
    let text = to_json_string_pretty(transcript)?;
    atomic::write(&path, text.as_bytes()).map_err(|source| TranscriptError::Write { path, source })
}

pub fn load(dir: &Path) -> Result<Option<Transcript>, TranscriptError> {
    load_file(dir, TRANSCRIPT_FILE)
}

pub fn save(dir: &Path, transcript: &Transcript) -> Result<(), TranscriptError> {
    save_file(dir, TRANSCRIPT_FILE, transcript)
}

pub fn load_live(dir: &Path) -> Result<Option<Transcript>, TranscriptError> {
    load_file(dir, LIVE_TRANSCRIPT_FILE)
}

pub fn save_live(dir: &Path, transcript: &Transcript) -> Result<(), TranscriptError> {
    save_file(dir, LIVE_TRANSCRIPT_FILE, transcript)
}

/// The final transcript if there is one, otherwise the live draft; the flag is true for a draft.
pub fn load_best(dir: &Path) -> Result<Option<(Transcript, bool)>, TranscriptError> {
    if let Some(t) = load(dir)? {
        return Ok(Some((t, false)));
    }
    Ok(load_live(dir)?.map(|t| (t, true)))
}

pub fn render_json(transcript: &Transcript) -> Result<String, TranscriptError> {
    Ok(to_json_string_pretty(transcript)?)
}

pub fn format_timestamp(ns: u64) -> String {
    let total = ns / 1_000_000_000;
    format!("{:02}:{:02}:{:02}", total / 3600, total / 60 % 60, total % 60)
}

pub fn render(transcript: &Transcript, options: RenderOptions) -> String {
    let mut out = String::new();
    for snippet in &transcript.snippets {
        if options.timestamps {
            out.push('[');
            out.push_str(&format_timestamp(snippet.start_ns));
            out.push_str("] ");
        }
        if options.source {
            let name = transcript.sources.get(&snippet.source).map_or(snippet.source.as_str(), |s| s.description.as_str());
            out.push('[');
            out.push_str(name);
            out.push_str("] ");
        }
        if options.speaker {
            if let Some(id) = &snippet.speaker {
                let label = transcript.speakers.get(id).map_or(id.as_str(), |s| s.label.as_str());
                out.push('[');
                out.push_str(label);
                out.push_str("] ");
            }
        }
        out.push_str(snippet.text.trim());
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_recorder_schema::transcript::{Snippet, TranscriptSource, TranscriptSourceDirection, TranscriptSpeaker};

    fn sample() -> Transcript {
        Transcript {
            version: TRANSCRIPT_VERSION,
            created_at: "2026-11-23T10:00:00Z".into(),
            model: "m".into(),
            language: Some("en".into()),
            diarized: true,
            sources: [("input:mic".to_string(), TranscriptSource { description: "Mic".into(), direction: TranscriptSourceDirection::Input })].into(),
            speakers: [("S1".to_string(), TranscriptSpeaker { source: "input:mic".into(), label: "Speaker 1".into() })].into(),
            snippets: vec![
                Snippet { source: "input:mic".into(), speaker: Some("S1".into()), segment: "f".into(), start_ns: 3_723_000_000_000, end_ns: 3_724_000_000_000, text: " hello ".into() },
                Snippet { source: "input:mic".into(), speaker: None, segment: "f".into(), start_ns: 3_725_000_000_000, end_ns: 3_726_000_000_000, text: "world".into() },
            ],
        }
    }

    #[test]
    fn renders_every_tag_combination() {
        let t = sample();
        let r = |source, speaker, timestamps| render(&t, RenderOptions { source, speaker, timestamps });
        assert_eq!(r(false, false, false), "hello\nworld\n");
        assert_eq!(r(true, false, false), "[Mic] hello\n[Mic] world\n");
        assert_eq!(r(false, true, false), "[Speaker 1] hello\nworld\n");
        assert_eq!(r(true, true, true), "[01:02:03] [Mic] [Speaker 1] hello\n[01:02:05] [Mic] world\n");
    }

    #[test]
    fn missing_file_is_none_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("audio-recorder-transcript-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load(&dir).unwrap().is_none());
        assert!(!exists(&dir));
        save(&dir, &sample()).unwrap();
        assert!(exists(&dir));
        assert_eq!(load(&dir).unwrap().unwrap().snippets.len(), 2);
    }
}

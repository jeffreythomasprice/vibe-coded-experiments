use std::{
    io,
    path::{Path, PathBuf},
};

use audio_recorder_schema::{
    SchemaError, from_json_str,
    manifest::{Manifest, SegmentStatus},
    to_json_string_pretty,
};

use crate::{
    atomic,
    audio_file::{Format, repair_flac},
};

pub const MANIFEST_FILE: &str = "manifest.json";
pub const PID_FILE: &str = "recording.pid";

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("failed to read {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error("failed to write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("invalid manifest {}: {source}", path.display())]
    Invalid { path: PathBuf, source: SchemaError },
    #[error("could not serialize manifest: {0}")]
    Serialize(#[from] SchemaError),
    #[error("unsupported manifest version {0}")]
    UnsupportedVersion(i64),
}

pub fn new_manifest(started_at: String) -> Manifest {
    Manifest {
        version: 1,
        started_at,
        ended_at: None,
        devices: Default::default(),
        segments: Vec::new(),
    }
}

pub fn load(dir: &Path) -> Result<Manifest, ManifestError> {
    let path = dir.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|source| ManifestError::Read { path: path.clone(), source })?;
    let manifest: Manifest =
        from_json_str(&text).map_err(|source| ManifestError::Invalid { path: path.clone(), source })?;
    if manifest.version != 1 {
        return Err(ManifestError::UnsupportedVersion(manifest.version));
    }
    Ok(manifest)
}

pub fn save(dir: &Path, manifest: &Manifest) -> Result<(), ManifestError> {
    let path = dir.join(MANIFEST_FILE);
    let text = to_json_string_pretty(manifest)?;
    atomic::write(&path, text.as_bytes()).map_err(|source| ManifestError::Write { path, source })
}

pub fn is_recording(dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(dir.join(PID_FILE)) else { return false };
    match text.trim().parse::<u32>() {
        Ok(pid) => Path::new(&format!("/proc/{pid}")).exists(),
        Err(_) => false,
    }
}

pub fn load_recovering(dir: &Path) -> Result<Manifest, ManifestError> {
    let mut manifest = load(dir)?;
    let stale = manifest.ended_at.is_none() || manifest.segments.iter().any(|s| s.status == SegmentStatus::Recording);
    if !stale || is_recording(dir) {
        return Ok(manifest);
    }
    for seg in &mut manifest.segments {
        if seg.status != SegmentStatus::Recording {
            continue;
        }
        let path = dir.join(&seg.file);
        let frames = match Format::of(Path::new(&seg.file)) {
            Some(Format::Flac) => repair_flac(&path).unwrap_or_else(|e| {
                tracing::warn!("could not repair {}: {e}", path.display());
                0
            }),
            _ => hound::WavReader::open(&path).map(|r| r.duration() as u64).unwrap_or(0),
        };
        let rate = seg.sample_rate.get();
        let end = seg.start_offset_ns + frames * 1_000_000_000 / rate;
        seg.frames = frames;
        seg.end_offset_ns = Some(end);
        seg.status = SegmentStatus::Truncated;
    }
    if manifest.ended_at.is_none() {
        manifest.ended_at = Some(manifest.started_at.clone());
    }
    save(dir, &manifest)?;
    let _ = std::fs::remove_file(dir.join(PID_FILE));
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use audio_recorder_schema::manifest::Segment;

    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("audio-recorder-mf-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tracks")).unwrap();
        dir
    }

    #[test]
    fn round_trip() {
        let dir = tmp("rt");
        let mut m = new_manifest("2026-01-01T00:00:00Z".into());
        m.ended_at = Some("2026-01-01T00:01:00Z".into());
        save(&dir, &m).unwrap();
        let back = load(&dir).unwrap();
        assert_eq!(back.started_at, m.started_at);
    }

    #[test]
    fn recovers_truncated_segment_from_wav() {
        let dir = tmp("recover");
        let spec = hound::WavSpec { channels: 1, sample_rate: 1000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(dir.join("tracks/a.wav"), spec).unwrap();
        for _ in 0..500 {
            w.write_sample(0i16).unwrap();
        }
        w.finalize().unwrap();
        let mut m = new_manifest("2026-01-01T00:00:00Z".into());
        m.segments.push(Segment {
            channels: NonZeroU64::new(1).unwrap(),
            device: "input:a".into(),
            end_offset_ns: None,
            file: "tracks/a.wav".into(),
            frames: 0,
            sample_rate: NonZeroU64::new(1000).unwrap(),
            start_offset_ns: 1_000_000_000,
            status: SegmentStatus::Recording,
        });
        save(&dir, &m).unwrap();
        let rec = load_recovering(&dir).unwrap();
        let seg = &rec.segments[0];
        assert_eq!(seg.status, SegmentStatus::Truncated);
        assert_eq!(seg.frames, 500);
        assert_eq!(seg.end_offset_ns, Some(1_500_000_000));
        assert!(rec.ended_at.is_some());
    }

    #[test]
    fn recovers_truncated_flac_segment() {
        let dir = tmp("recover-flac");
        let path = dir.join("tracks/a.flac");
        let mut w = crate::flac_writer::FlacWriter::create(&path, 8000, 1, 16).unwrap();
        w.write(&vec![100i32; 10_000]).unwrap();
        w.flush().unwrap();
        std::mem::forget(w);
        let mut m = new_manifest("2026-01-01T00:00:00Z".into());
        m.segments.push(Segment {
            channels: NonZeroU64::new(1).unwrap(),
            device: "input:a".into(),
            end_offset_ns: None,
            file: "tracks/a.flac".into(),
            frames: 0,
            sample_rate: NonZeroU64::new(8000).unwrap(),
            start_offset_ns: 0,
            status: SegmentStatus::Recording,
        });
        save(&dir, &m).unwrap();
        let seg = load_recovering(&dir).unwrap().segments.remove(0);
        assert_eq!(seg.status, SegmentStatus::Truncated);
        assert_eq!(seg.frames, 8192);
        let mut r = crate::audio_file::SegmentReader::open(&path).unwrap();
        assert_eq!(r.frames(), 8192);
        let mut out = Vec::new();
        assert_eq!(r.read(&mut out, usize::MAX).unwrap(), 8192);
    }

    #[test]
    fn dead_pid_is_not_recording() {
        let dir = tmp("pid");
        std::fs::write(dir.join(PID_FILE), "4294967").unwrap();
        assert!(!is_recording(&dir));
        std::fs::write(dir.join(PID_FILE), std::process::id().to_string()).unwrap();
        assert!(is_recording(&dir));
    }
}

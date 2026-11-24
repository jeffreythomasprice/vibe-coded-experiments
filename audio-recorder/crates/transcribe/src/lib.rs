pub mod assemble;
pub mod audio;
pub mod diarize;
pub mod live;
pub mod models;
pub mod whisper;
pub mod worker;

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use audio_recorder_core::{
    config::TranscriptionSettings,
    mixdown::MixdownError,
    session::manifest::{self, ManifestError},
    transcript::{self, TRANSCRIPT_VERSION, TranscriptError},
};
use audio_recorder_schema::{
    manifest::{Direction, Manifest, Segment},
    transcript::{Transcript, TranscriptSource, TranscriptSourceDirection},
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    assemble::{DeviceInput, assemble},
    audio::DeviceAudio,
    diarize::{DiarizeFailure, Diarizer},
    models::ModelError,
    whisper::{Whisper, WhisperFailure},
};

#[derive(Debug, thiserror::Error)]
pub enum TranscribeError {
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Decode(#[from] MixdownError),
    #[error(transparent)]
    Whisper(WhisperFailure),
    #[error(transparent)]
    Diarize(DiarizeFailure),
    #[error(transparent)]
    Transcript(#[from] TranscriptError),
    #[error("session is still recording")]
    SessionActive,
    #[error("session has no audible audio")]
    NoAudio,
    #[error("cancelled")]
    Cancelled,
}

impl From<WhisperFailure> for TranscribeError {
    fn from(e: WhisperFailure) -> Self {
        match e {
            WhisperFailure::Cancelled => Self::Cancelled,
            other => Self::Whisper(other),
        }
    }
}

impl From<DiarizeFailure> for TranscribeError {
    fn from(e: DiarizeFailure) -> Self {
        match e {
            DiarizeFailure::Cancelled => Self::Cancelled,
            other => Self::Diarize(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub stage: String,
    pub fraction: Option<f32>,
}

pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

fn check(cancel: &AtomicBool) -> Result<(), TranscribeError> {
    if cancel.load(Ordering::Relaxed) { Err(TranscribeError::Cancelled) } else { Ok(()) }
}

fn group_by_device(manifest: &Manifest) -> BTreeMap<&str, Vec<&Segment>> {
    let mut map: BTreeMap<&str, Vec<&Segment>> = BTreeMap::new();
    for seg in &manifest.segments {
        map.entry(seg.device.as_str()).or_default().push(seg);
    }
    map
}

pub fn transcribe_session(
    dir: &Path,
    settings: &TranscriptionSettings,
    progress: ProgressFn,
    cancel: Arc<AtomicBool>,
) -> Result<Transcript, TranscribeError> {
    if manifest::is_recording(dir) {
        return Err(TranscribeError::SessionActive);
    }
    let manifest = manifest::load_recovering(dir)?;

    let files = models::required(settings);
    {
        let progress = progress.clone();
        let mut last = 0u64;
        models::ensure(
            &files,
            &mut |label, done, total| {
                if done < last || done - last > 1 << 20 || Some(done) == total {
                    last = done;
                    progress(Progress {
                        stage: format!("Downloading {label} model"),
                        fraction: total.map(|t| done as f32 / t as f32),
                    });
                }
            },
            &|| cancel.load(Ordering::Relaxed),
        )?;
    }
    check(&cancel)?;

    progress(Progress { stage: "Reading audio".into(), fraction: None });
    let mut devices: Vec<(String, DeviceAudio)> = Vec::new();
    for (device, segments) in group_by_device(&manifest) {
        check(&cancel)?;
        let audio = DeviceAudio::load(dir, &segments)?;
        if audio.is_silent() {
            tracing::info!("skipping silent device {device}");
            continue;
        }
        devices.push((device.to_string(), audio));
    }
    if devices.is_empty() {
        return Err(TranscribeError::NoAudio);
    }

    progress(Progress { stage: "Loading models".into(), fraction: None });
    let mut whisper = Whisper::load(&settings.whisper_model.path, &settings.vad_model.path, &settings.language, settings.gpu)?;
    let mut diarizer = if settings.diarize {
        Some(Diarizer::load(
            &settings.segmentation_model.path,
            &settings.embedding_model.path,
            settings.num_speakers,
            settings.cluster_threshold,
        )?)
    } else {
        None
    };

    let total = devices.len() as f32;
    let mut results = Vec::new();
    for (index, (device, audio)) in devices.iter().enumerate() {
        check(&cancel)?;
        let description = manifest.devices.get(device).map_or(device.as_str(), |d| d.description.as_str()).to_string();
        let base = index as f32 / total;
        let share = 1.0 / total;
        let whisper_share = if diarizer.is_some() { 0.85 } else { 1.0 };

        let stage = format!("Transcribing {description} ({}/{})", index + 1, devices.len());
        let report = {
            let progress = progress.clone();
            let stage = stage.clone();
            Arc::new(move |p: f32| progress(Progress { stage: stage.clone(), fraction: Some(base + share * whisper_share * p) }))
        };
        let words = whisper.transcribe(&audio.samples, report, cancel.clone())?;

        let turns = match diarizer.as_mut() {
            Some(d) => {
                let stage = format!("Identifying speakers in {description} ({}/{})", index + 1, devices.len());
                let progress = progress.clone();
                let report = Arc::new(move |p: f32| {
                    progress(Progress { stage: stage.clone(), fraction: Some(base + share * (whisper_share + (1.0 - whisper_share) * p)) })
                });
                d.turns(&audio.samples, report, cancel.clone())?
            }
            None => Vec::new(),
        };
        results.push((device.clone(), words, turns));
    }

    let inputs: Vec<DeviceInput> = results
        .into_iter()
        .map(|(device, words, turns)| {
            let (name, audio) = devices.iter().find(|(d, _)| *d == device).expect("device present");
            DeviceInput { device: name.as_str(), audio, words, turns }
        })
        .collect();
    let (snippets, speakers) = assemble(&inputs);

    let sources: HashMap<String, TranscriptSource> = devices
        .iter()
        .map(|(device, _)| {
            let (description, direction) = match manifest.devices.get(device) {
                Some(d) => (
                    d.description.clone(),
                    match d.direction {
                        Direction::Input => TranscriptSourceDirection::Input,
                        Direction::Output => TranscriptSourceDirection::Output,
                    },
                ),
                None => (device.clone(), TranscriptSourceDirection::Input),
            };
            (device.clone(), TranscriptSource { description, direction })
        })
        .collect();

    let model_name = settings
        .whisper_model
        .path
        .file_name()
        .map_or_else(|| "unknown".to_string(), |n| n.to_string_lossy().into_owned());
    let transcript = Transcript {
        version: TRANSCRIPT_VERSION,
        created_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default(),
        model: model_name,
        language: (settings.language != "auto").then(|| settings.language.clone()),
        diarized: diarizer.is_some(),
        sources,
        speakers,
        snippets,
    };
    check(&cancel)?;
    transcript::save(dir, &transcript)?;
    Ok(transcript)
}

use std::{path::Path, time::Duration};

use audio_recorder_core::{
    device::{DeviceKey, DeviceStatus, Direction},
    library::SessionSummary,
    mix::default_settings,
    playback::PlaybackState,
    session::RecorderEvent,
};
use audio_recorder_transcribe::live::{LiveEvent, LiveStatus};
use audio_recorder_schema::{
    ipc::{
        ChannelGainDto, DeviceDto, Direction as DtoDirection, MixDto, PlaybackStateDto, RecorderEventDto, SegmentDto, SegmentDtoStatus,
        JobDto, JobDtoKind, JobDtoState, SessionDetailDto, SessionDeviceDto, SessionState, SessionStatusDto, SessionSummaryDto,
        LiveTranscriptEventDto, SnippetDto, TranscriptDto, TranscriptSourceDto, TranscriptSpeakerDto,
    },
    jobs::{Job, JobKind, JobState},
    transcript::Transcript,
    manifest::{Manifest, SegmentStatus},
    mix::{ChannelGain, MixSettings},
};

fn direction(d: Direction) -> DtoDirection {
    match d {
        Direction::Input => DtoDirection::Input,
        Direction::Output => DtoDirection::Output,
    }
}

pub fn millis(d: Duration) -> u64 {
    d.as_millis() as u64
}

pub fn session_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn device(d: &DeviceStatus) -> DeviceDto {
    DeviceDto {
        key: d.key.to_string(),
        direction: direction(d.key.direction),
        description: d.description.clone(),
        present: d.present,
        enabled: d.enabled,
        recording: d.recording,
    }
}

pub fn session_status(status: Option<&audio_recorder_core::session::SessionStatus>) -> SessionStatusDto {
    SessionStatusDto {
        state: match status {
            None => SessionState::Idle,
            Some(s) if s.paused => SessionState::Paused,
            Some(_) => SessionState::Recording,
        },
        name: status.map(|s| session_name(&s.dir)),
        elapsed_ms: status.map(|s| millis(s.elapsed)).unwrap_or(0),
        bytes: status.map(|s| s.bytes).unwrap_or(0),
    }
}

pub fn summary(s: &SessionSummary) -> SessionSummaryDto {
    SessionSummaryDto {
        name: s.name.clone(),
        started_at: s.started_at.clone(),
        duration_ms: millis(s.duration),
        segment_count: s.segment_count as u64,
        device_count: s.device_count as u64,
        bytes: s.bytes,
        recording: s.recording,
        has_transcript: s.has_transcript,
        has_live_transcript: s.has_live_transcript,
        has_summary: s.has_summary,
        error: s.error.clone(),
    }
}

pub fn mix_to_dto(m: &MixSettings) -> MixDto {
    let conv = |g: &ChannelGain| ChannelGainDto { gain: g.gain, muted: g.muted };
    MixDto {
        leveling: m.leveling,
        auto: m.auto,
        devices: m.devices.iter().map(|(k, g)| (k.clone(), conv(g))).collect(),
        segments: m.segments.iter().map(|(k, g)| (k.clone(), conv(g))).collect(),
    }
}

pub fn mix_from_dto(m: &MixDto) -> MixSettings {
    let conv = |g: &ChannelGainDto| ChannelGain { gain: g.gain, muted: g.muted };
    let mut out = default_settings();
    out.leveling = m.leveling;
    out.auto = m.auto;
    out.devices = m.devices.iter().map(|(k, g)| (k.clone(), conv(g))).collect();
    out.segments = m.segments.iter().map(|(k, g)| (k.clone(), conv(g))).collect();
    out
}

pub fn detail(summary: SessionSummaryDto, manifest: &Manifest, mix: &MixSettings) -> SessionDetailDto {
    let mut devices: Vec<SessionDeviceDto> = manifest
        .devices
        .iter()
        .map(|(key, d)| SessionDeviceDto {
            key: key.clone(),
            direction: match d.direction {
                audio_recorder_schema::manifest::Direction::Input => DtoDirection::Input,
                audio_recorder_schema::manifest::Direction::Output => DtoDirection::Output,
            },
            description: d.description.clone(),
        })
        .collect();
    devices.sort_by(|a, b| a.key.cmp(&b.key));
    let segments = manifest
        .segments
        .iter()
        .map(|s| SegmentDto {
            device: s.device.clone(),
            file: s.file.clone(),
            start_offset_ms: s.start_offset_ns / 1_000_000,
            end_offset_ms: s.end_offset_ns.map(|e| e / 1_000_000),
            status: match s.status {
                SegmentStatus::Recording => SegmentDtoStatus::Recording,
                SegmentStatus::Complete => SegmentDtoStatus::Complete,
                SegmentStatus::Truncated => SegmentDtoStatus::Truncated,
            },
        })
        .collect();
    SessionDetailDto { summary, devices, segments, mix: mix_to_dto(mix) }
}

pub fn playback(s: &PlaybackState) -> PlaybackStateDto {
    PlaybackStateDto {
        session: s.session.as_deref().map(session_name),
        track: s.track.as_ref().map(DeviceKey::to_string),
        position_ms: millis(s.position),
        duration_ms: millis(s.duration),
        paused: s.paused,
        playing: s.playing,
    }
}

pub fn event(ev: &RecorderEvent) -> Option<RecorderEventDto> {
    Some(match ev {
        RecorderEvent::DeviceChanged(d) => RecorderEventDto::DeviceChanged { device: device(d) },
        RecorderEvent::SessionStarted { dir } => RecorderEventDto::SessionStarted { name: session_name(dir) },
        RecorderEvent::SessionStopped { dir } => RecorderEventDto::SessionStopped { name: session_name(dir) },
        RecorderEvent::SessionPaused { dir } => RecorderEventDto::SessionPaused { name: session_name(dir) },
        RecorderEvent::SessionResumed { dir } => RecorderEventDto::SessionResumed { name: session_name(dir) },
        RecorderEvent::SegmentOpened { key, file } => {
            RecorderEventDto::SegmentOpened { device: key.to_string(), file: file.clone() }
        }
        RecorderEvent::SegmentClosed { key, file } => {
            RecorderEventDto::SegmentClosed { device: key.to_string(), file: file.clone() }
        }
        RecorderEvent::Level { key, peak } => RecorderEventDto::Level { device: key.to_string(), peak: *peak as f64 },
        RecorderEvent::CaptureError { key, message } => {
            RecorderEventDto::CaptureError { device: key.to_string(), message: message.clone() }
        }
    })
}

pub fn job(j: &Job) -> JobDto {
    JobDto {
        id: j.id.clone(),
        kind: match j.kind {
            JobKind::Transcribe => JobDtoKind::Transcribe,
            JobKind::Compress => JobDtoKind::Compress,
            JobKind::Summarize => JobDtoKind::Summarize,
        },
        session: j.session.clone(),
        replace: j.replace,
        state: match j.state {
            JobState::Queued => JobDtoState::Queued,
            JobState::Running => JobDtoState::Running,
            JobState::Done => JobDtoState::Done,
            JobState::Failed => JobDtoState::Failed,
            JobState::Cancelled => JobDtoState::Cancelled,
        },
        created_at: j.created_at.clone(),
        started_at: j.started_at.clone(),
        finished_at: j.finished_at.clone(),
        stage: j.stage.clone(),
        fraction: j.fraction,
        error: j.error.clone(),
    }
}

pub fn summary_dto(s: &audio_recorder_schema::summary::Summary, stale: bool) -> audio_recorder_schema::ipc::SummaryDto {
    audio_recorder_schema::ipc::SummaryDto {
        text: s.text.clone(),
        created_at: s.created_at.clone(),
        provider: s.provider.clone(),
        model: s.model.clone(),
        stale,
    }
}

pub fn transcript(name: &str, t: &Transcript, live: bool) -> TranscriptDto {
    let mut sources: Vec<_> = t
        .sources
        .iter()
        .map(|(key, s)| TranscriptSourceDto {
            key: key.clone(),
            description: s.description.clone(),
            direction: match s.direction {
                audio_recorder_schema::transcript::TranscriptSourceDirection::Input => DtoDirection::Input,
                audio_recorder_schema::transcript::TranscriptSourceDirection::Output => DtoDirection::Output,
            },
        })
        .collect();
    sources.sort_by(|a, b| a.key.cmp(&b.key));
    let mut speakers: Vec<_> = t
        .speakers
        .iter()
        .map(|(id, s)| TranscriptSpeakerDto { id: id.clone(), source: s.source.clone(), label: s.label.clone() })
        .collect();
    speakers.sort_by(|a, b| a.id.len().cmp(&b.id.len()).then_with(|| a.id.cmp(&b.id)));
    TranscriptDto {
        name: name.to_string(),
        created_at: t.created_at.clone(),
        model: t.model.clone(),
        language: t.language.clone(),
        diarized: t.diarized,
        live,
        sources,
        speakers,
        snippets: t
            .snippets
            .iter()
            .map(snippet)
            .collect(),
    }
}

pub fn snippet(s: &audio_recorder_schema::transcript::Snippet) -> SnippetDto {
    SnippetDto {
        source: s.source.clone(),
        speaker: s.speaker.clone(),
        segment: s.segment.clone(),
        start_ms: s.start_ns / 1_000_000,
        end_ms: s.end_ns / 1_000_000,
        text: s.text.clone(),
    }
}

pub fn live_event(name: &str, ev: &LiveEvent) -> LiveTranscriptEventDto {
    let name = name.to_string();
    match ev {
        LiveEvent::Snippet { snippet: s, description } => {
            LiveTranscriptEventDto::Snippet { name, description: description.clone(), snippet: snippet(s) }
        }
        LiveEvent::Partial { source, description, start_ns, text } => LiveTranscriptEventDto::Partial {
            name,
            source: source.clone(),
            description: description.clone(),
            start_ms: start_ns / 1_000_000,
            text: text.clone(),
        },
        LiveEvent::Status(status) => {
            let (state, message, fraction) = match status {
                LiveStatus::Downloading { label, fraction } => {
                    ("downloading", Some(format!("Downloading {label} model")), fraction.map(f64::from))
                }
                LiveStatus::Loading => ("loading", None, None),
                LiveStatus::Ready => ("ready", None, None),
                LiveStatus::Failed(message) => ("failed", Some(message.clone()), None),
            };
            LiveTranscriptEventDto::Status { name, state: state.into(), message, fraction }
        }
    }
}

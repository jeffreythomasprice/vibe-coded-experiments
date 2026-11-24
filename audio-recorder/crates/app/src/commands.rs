use std::time::Duration;

use audio_recorder_core::{
    balance::{self, BalanceError},
    compress,
    device::DeviceKey,
    jobs::JobError,
    library::{self, LibraryError},
    mix::{self, MixError},
    playback::PlaybackError,
    session::RecorderError,
    settings::SettingsError,
    summary::{self, SummaryError},
    transcript::{self, RenderOptions, TranscriptError},
};
use audio_recorder_schema::{
    Schema, SchemaError, from_value,
    ipc::{
        DeviceList, Empty, EnqueueCompressionArgs, EnqueueSummaryArgs, EnqueueTranscriptionArgs, IpcError, IpcErrorKind, JobDto, JobIdArgs, JobList, MixDto, NoArgs, PlayArgs,
        PlaybackStateDto, QuitArgs, RenderTranscriptArgs, RenderedTranscript, SeekArgs, SessionDetailDto, SessionList,
        SessionNameArgs, SessionStatusDto, SetDeviceEnabledArgs, SetMixArgs, SettingsDto, SummaryDto, TranscriptDto,
    },
    jobs::JobKind,
    to_value,
};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::{AppState, dto};

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    Args(#[from] SchemaError),
    #[error(transparent)]
    Recorder(#[from] RecorderError),
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error(transparent)]
    Mix(#[from] MixError),
    #[error(transparent)]
    Balance(#[from] BalanceError),
    #[error(transparent)]
    Jobs(#[from] JobError),
    #[error(transparent)]
    Transcript(#[from] TranscriptError),
    #[error("{0} has no transcript")]
    NoTranscript(String),
    #[error("{0} already has a transcript")]
    AlreadyTranscribed(String),
    #[error("{0} has no summary")]
    NoSummary(String),
    #[error("{0} already has a summary")]
    AlreadySummarized(String),
    #[error(transparent)]
    Summary(#[from] SummaryError),
    #[error("{0} has no uncompressed tracks")]
    AlreadyCompressed(String),
    #[error(transparent)]
    Manifest(#[from] audio_recorder_core::session::manifest::ManifestError),
    #[error("{0} is still recording")]
    SessionActive(String),
    #[error("background task failed: {0}")]
    Task(String),
    #[error(transparent)]
    Key(#[from] audio_recorder_core::device::ParseKeyError),
    #[error("a recording is in progress; stop it first")]
    RecordingActive,
}

impl CommandError {
    fn to_ipc(&self) -> IpcError {
        let kind = match self {
            CommandError::Args(_) | CommandError::Key(_) => IpcErrorKind::InvalidArgs,
            CommandError::Recorder(_) | CommandError::Settings(_) => IpcErrorKind::Recorder,
            CommandError::Library(_)
            | CommandError::Mix(_)
            | CommandError::Balance(_)
            | CommandError::Manifest(_)
            | CommandError::AlreadyCompressed(_) => IpcErrorKind::Library,
            CommandError::Task(_) => IpcErrorKind::Internal,
            CommandError::Jobs(_)
            | CommandError::Transcript(_)
            | CommandError::NoTranscript(_)
            | CommandError::AlreadyTranscribed(_)
            | CommandError::SessionActive(_) => IpcErrorKind::Transcription,
            CommandError::Summary(_) | CommandError::NoSummary(_) | CommandError::AlreadySummarized(_) => IpcErrorKind::Summarization,
            CommandError::Playback(_) | CommandError::RecordingActive => IpcErrorKind::Playback,
        };
        IpcError { kind, message: self.to_string() }
    }
}

fn fail(e: CommandError) -> Value {
    let ipc = e.to_ipc();
    to_value(&ipc).unwrap_or(Value::String(ipc.message))
}

fn handle<A: Schema, R: Schema>(raw: Value, f: impl FnOnce(A) -> Result<R, CommandError>) -> Result<Value, Value> {
    let run = || -> Result<Value, CommandError> {
        let args: A = from_value(raw)?;
        Ok(to_value(&f(args)?)?)
    };
    run().map_err(fail)
}

pub fn session_status_dto(state: &AppState) -> Result<SessionStatusDto, CommandError> {
    Ok(dto::session_status(state.recorder.session_status()?.as_ref()))
}

pub fn start(state: &AppState) -> Result<SessionStatusDto, CommandError> {
    state.playback.stop();
    state.recorder.start_session()?;
    session_status_dto(state)
}

pub fn toggle(state: &AppState) -> Result<SessionStatusDto, CommandError> {
    if state.recorder.session_status()?.is_some() {
        state.recorder.stop_session()?;
        session_status_dto(state)
    } else {
        start(state)
    }
}

pub fn toggle_pause(state: &AppState) -> Result<SessionStatusDto, CommandError> {
    match state.recorder.session_status()? {
        Some(s) if s.paused => {
            state.playback.stop();
            state.recorder.resume_session()?;
        }
        Some(_) => state.recorder.pause_session()?,
        None => return Err(RecorderError::NoSession.into()),
    }
    session_status_dto(state)
}

#[tauri::command]
pub fn list_devices(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| {
        let devices = state.recorder.devices()?.iter().map(dto::device).collect();
        Ok(DeviceList { devices })
    })
}

#[tauri::command]
pub fn set_device_enabled(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: SetDeviceEnabledArgs| {
        let key: DeviceKey = a.key.parse()?;
        state.recorder.set_enabled(&key, a.enabled)?;
        Ok(Empty {})
    })
}

#[tauri::command]
pub fn session_status(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| session_status_dto(&state))
}

#[tauri::command]
pub fn start_session(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| start(&state))
}

#[tauri::command]
pub fn pause_session(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| {
        state.recorder.pause_session()?;
        session_status_dto(&state)
    })
}

#[tauri::command]
pub fn resume_session(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| {
        state.playback.stop();
        state.recorder.resume_session()?;
        session_status_dto(&state)
    })
}

#[tauri::command]
pub fn stop_session(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| {
        state.recorder.stop_session()?;
        session_status_dto(&state)
    })
}

#[tauri::command]
pub fn list_sessions(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| {
        let sessions = library::list_sessions(&state.config.sessions_dir)?.iter().map(dto::summary).collect();
        Ok(SessionList { sessions })
    })
}

#[tauri::command]
pub fn get_session(args: Value, app: AppHandle, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: SessionNameArgs| -> Result<SessionDetailDto, CommandError> {
        let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
        let manifest = library::load_session(&dir)?;
        let mix = mix::load(&dir)?;
        if !dir.join(mix::MIX_FILE).exists() {
            crate::spawn_auto_balance(&app, dir.clone());
        }
        let summary = library::list_sessions(&state.config.sessions_dir)?
            .into_iter()
            .find(|s| s.name == a.name)
            .map(|s| dto::summary(&s))
            .ok_or_else(|| LibraryError::BadName(a.name.clone()))?;
        Ok(dto::detail(summary, &manifest, &mix))
    })
}

#[tauri::command]
pub fn set_mix(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: SetMixArgs| {
        let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
        let settings = dto::mix_from_dto(&a.mix);
        mix::save(&dir, &settings)?;
        state.playback.set_mix(dir, settings);
        Ok(Empty {})
    })
}

#[tauri::command]
pub async fn auto_balance(args: Value, app: AppHandle) -> Result<Value, Value> {
    let joined = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        handle(args, |a: SessionNameArgs| -> Result<MixDto, CommandError> {
            let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
            if state.recorder.session_status()?.is_some_and(|s| dto::session_name(&s.dir) == a.name) {
                return Err(CommandError::RecordingActive);
            }
            let settings = balance::auto_mix(&dir)?;
            mix::save(&dir, &settings)?;
            let out = dto::mix_to_dto(&settings);
            state.playback.set_mix(dir, settings);
            Ok(out)
        })
    })
    .await;
    joined.unwrap_or_else(|e| Err(fail(CommandError::Task(e.to_string()))))
}

#[tauri::command]
pub fn reset_mix(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: SessionNameArgs| -> Result<MixDto, CommandError> {
        let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
        let settings = mix::default_settings();
        mix::save(&dir, &settings)?;
        state.playback.set_mix(dir, settings.clone());
        Ok(dto::mix_to_dto(&settings))
    })
}

#[tauri::command]
pub fn delete_session(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: SessionNameArgs| {
        let active = state.recorder.session_status()?;
        if active.is_some_and(|s| dto::session_name(&s.dir) == a.name) {
            return Err(CommandError::RecordingActive);
        }
        if state.playback.state().session.as_deref().map(dto::session_name).as_deref() == Some(a.name.as_str()) {
            state.playback.stop();
        }
        for job in state.jobs.list()?.iter().filter(|j| j.session == a.name && audio_recorder_core::jobs::is_active(&j.state)) {
            state.jobs.cancel(&job.id)?;
        }
        library::delete_session(&state.config.sessions_dir, &a.name)?;
        Ok(Empty {})
    })
}

#[tauri::command]
pub fn play(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: PlayArgs| -> Result<PlaybackStateDto, CommandError> {
        if state.recorder.session_status()?.is_some() {
            return Err(CommandError::RecordingActive);
        }
        let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
        let track = a.track.as_deref().map(str::parse::<DeviceKey>).transpose()?;
        state.playback.play(dir, track)?;
        Ok(dto::playback(&state.playback.state()))
    })
}

#[tauri::command]
pub fn pause(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| {
        state.playback.pause();
        Ok(dto::playback(&state.playback.state()))
    })
}

#[tauri::command]
pub fn resume(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| {
        state.playback.resume();
        Ok(dto::playback(&state.playback.state()))
    })
}

#[tauri::command]
pub fn seek(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: SeekArgs| {
        state.playback.seek(Duration::from_millis(a.position_ms));
        Ok(dto::playback(&state.playback.state()))
    })
}

#[tauri::command]
pub fn stop_playback(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| {
        state.playback.stop();
        Ok(dto::playback(&state.playback.state()))
    })
}

#[tauri::command]
pub fn playback_state(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| Ok(dto::playback(&state.playback.state())))
}

#[tauri::command]
pub fn get_settings(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| {
        let settings = state.settings.lock().expect("settings lock");
        Ok(SettingsDto { theme: settings.get().theme.clone() })
    })
}

#[tauri::command]
pub fn set_settings(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: SettingsDto| {
        let mut settings = state.settings.lock().expect("settings lock");
        settings.set(audio_recorder_schema::settings::Settings { theme: a.theme })?;
        Ok(SettingsDto { theme: settings.get().theme.clone() })
    })
}

#[tauri::command]
pub fn list_jobs(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| Ok(JobList { jobs: state.jobs.list()?.iter().map(dto::job).collect() }))
}

#[tauri::command]
pub fn enqueue_compression(args: Value, app: AppHandle, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: EnqueueCompressionArgs| -> Result<JobDto, CommandError> {
        let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
        if state.recorder.session_status()?.is_some_and(|s| dto::session_name(&s.dir) == a.name) {
            return Err(CommandError::SessionActive(a.name));
        }
        if compress::pending_segments(&dir, &state.config.compression)? == 0 {
            return Err(CommandError::AlreadyCompressed(a.name));
        }
        let (job, _) = state.jobs.enqueue(JobKind::Compress, &a.name, false)?;
        crate::kick_worker(&app);
        Ok(dto::job(&job))
    })
}

#[tauri::command]
pub fn enqueue_transcription(args: Value, app: AppHandle, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: EnqueueTranscriptionArgs| -> Result<JobDto, CommandError> {
        let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
        if state.recorder.session_status()?.is_some_and(|s| dto::session_name(&s.dir) == a.name) {
            return Err(CommandError::SessionActive(a.name));
        }
        if !a.replace && transcript::exists(&dir) {
            return Err(CommandError::AlreadyTranscribed(a.name));
        }
        let (job, _) = state.jobs.enqueue(JobKind::Transcribe, &a.name, a.replace)?;
        crate::kick_worker(&app);
        Ok(dto::job(&job))
    })
}

#[tauri::command]
pub fn enqueue_summary(args: Value, app: AppHandle, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: EnqueueSummaryArgs| -> Result<JobDto, CommandError> {
        let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
        if state.recorder.session_status()?.is_some_and(|s| dto::session_name(&s.dir) == a.name) {
            return Err(CommandError::SessionActive(a.name));
        }
        if !transcript::exists(&dir) {
            return Err(CommandError::NoTranscript(a.name));
        }
        if !a.replace && summary::exists(&dir) {
            return Err(CommandError::AlreadySummarized(a.name));
        }
        let (job, _) = state.jobs.enqueue(JobKind::Summarize, &a.name, a.replace)?;
        crate::kick_worker(&app);
        Ok(dto::job(&job))
    })
}

#[tauri::command]
pub fn get_summary(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: SessionNameArgs| -> Result<SummaryDto, CommandError> {
        let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
        let s = summary::load(&dir)?.ok_or_else(|| CommandError::NoSummary(a.name.clone()))?;
        let stale = transcript::load(&dir)?.is_some_and(|t| summary::is_stale(&s, &t));
        Ok(dto::summary_dto(&s, stale))
    })
}

#[tauri::command]
pub fn cancel_job(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: JobIdArgs| {
        state.jobs.cancel(&a.id)?;
        Ok(Empty {})
    })
}

#[tauri::command]
pub fn clear_finished_jobs(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |_: NoArgs| {
        state.jobs.clear_finished()?;
        Ok(Empty {})
    })
}

#[tauri::command]
pub fn get_transcript(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: SessionNameArgs| -> Result<TranscriptDto, CommandError> {
        let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
        let (t, live) = transcript::load_best(&dir)?.ok_or_else(|| CommandError::NoTranscript(a.name.clone()))?;
        Ok(dto::transcript(&a.name, &t, live))
    })
}

#[tauri::command]
pub fn render_transcript(args: Value, state: State<'_, AppState>) -> Result<Value, Value> {
    handle(args, |a: RenderTranscriptArgs| -> Result<RenderedTranscript, CommandError> {
        let dir = library::session_dir(&state.config.sessions_dir, &a.name)?;
        let (t, _) = transcript::load_best(&dir)?.ok_or_else(|| CommandError::NoTranscript(a.name.clone()))?;
        let text = transcript::render(&t, RenderOptions { source: a.source, speaker: a.speaker, timestamps: a.timestamps });
        Ok(RenderedTranscript { text })
    })
}

#[tauri::command]
pub fn quit_app(args: Value, app: AppHandle) -> Result<Value, Value> {
    handle(args, |a: QuitArgs| {
        crate::request_quit(&app, a.force);
        Ok(Empty {})
    })
}

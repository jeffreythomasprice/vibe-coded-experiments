use std::collections::HashMap;

use audio_recorder_schema::{
    Schema,
    ipc::{
        DeviceDto, DeviceList, JobDto, JobDtoKind, JobDtoState, JobList, LiveTranscriptEventDto, MixDto, NoArgs, PlaybackStateDto, RecorderEventDto,
        SessionDetailDto, SessionList, SessionNameArgs, SessionState, SessionStatusDto, SessionSummaryDto, SettingsDto, SnippetDto,
    },
    theme::Theme,
};
use leptos::{prelude::*, task::spawn_local};

use crate::{ipc, theme};

pub const PLAYBACK_LOCKED_TIP: &str = "Playback is disabled while a recording is in progress, because anything played would be recorded too. Stop the recording to play sessions back.";

#[derive(Clone, PartialEq)]
pub enum ConfirmAction {
    DeleteSession(String),
    Retranscribe(String),
    Resummarize(String),
    QuitApp,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tab {
    Sessions,
    Tasks,
}

pub fn job_active(job: &JobDto) -> bool {
    matches!(job.state, JobDtoState::Queued | JobDtoState::Running)
}

#[derive(Clone, PartialEq)]
pub struct Confirm {
    pub title: String,
    pub body: String,
    pub confirm_label: String,
    pub action: ConfirmAction,
}

#[derive(Clone, Default)]
pub struct LiveView {
    pub name: String,
    pub snippets: Vec<(SnippetDto, String)>,
    pub partials: Vec<(String, String, String)>,
    pub status: Option<String>,
}

#[derive(Clone, Copy)]
pub struct Store {
    pub live: RwSignal<LiveView>,
    pub devices: RwSignal<Vec<DeviceDto>>,
    pub levels: RwSignal<HashMap<String, f64>>,
    pub state: RwSignal<SessionState>,
    pub elapsed_base_ms: RwSignal<u64>,
    pub session_bytes: RwSignal<u64>,
    pub running_since_ms: RwSignal<f64>,
    pub now_ms: RwSignal<f64>,
    pub sessions: RwSignal<Vec<SessionSummaryDto>>,
    pub detail: RwSignal<Option<SessionDetailDto>>,
    pub mix_event: RwSignal<Option<(String, MixDto)>>,
    pub playback: RwSignal<PlaybackStateDto>,
    pub confirm: RwSignal<Option<Confirm>>,
    pub tab: RwSignal<Tab>,
    pub jobs: RwSignal<Vec<JobDto>>,
    pub transcript_rev: RwSignal<u32>,
    pub tag_source: RwSignal<bool>,
    pub tag_speaker: RwSignal<bool>,
    pub tag_timestamps: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub themes: StoredValue<Vec<Theme>>,
    pub theme: RwSignal<String>,
}

fn idle_playback() -> PlaybackStateDto {
    PlaybackStateDto { session: None, track: None, position_ms: 0, duration_ms: 0, paused: false, playing: false }
}

impl Store {
    pub fn new() -> Self {
        Self {
            live: RwSignal::new(LiveView::default()),
            devices: RwSignal::new(Vec::new()),
            levels: RwSignal::new(HashMap::new()),
            state: RwSignal::new(SessionState::Idle),
            elapsed_base_ms: RwSignal::new(0),
            session_bytes: RwSignal::new(0),
            running_since_ms: RwSignal::new(0.0),
            now_ms: RwSignal::new(js_sys::Date::now()),
            sessions: RwSignal::new(Vec::new()),
            detail: RwSignal::new(None),
            mix_event: RwSignal::new(None),
            playback: RwSignal::new(idle_playback()),
            confirm: RwSignal::new(None),
            tab: RwSignal::new(Tab::Sessions),
            jobs: RwSignal::new(Vec::new()),
            transcript_rev: RwSignal::new(0),
            tag_source: RwSignal::new(true),
            tag_speaker: RwSignal::new(true),
            tag_timestamps: RwSignal::new(true),
            error: RwSignal::new(None),
            themes: StoredValue::new(theme::all()),
            theme: RwSignal::new(theme::SYSTEM_ID.to_string()),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        let base = self.elapsed_base_ms.get();
        match self.state.get() {
            SessionState::Recording => base + (self.now_ms.get() - self.running_since_ms.get()).max(0.0) as u64,
            _ => base,
        }
    }

    pub fn playback_locked(&self) -> bool {
        self.state.get() != SessionState::Idle
    }
}

pub fn run<A, R>(store: Store, command: &'static str, args: A, on_ok: impl FnOnce(R) + 'static)
where
    A: Schema + 'static,
    R: Schema + 'static,
{
    run_then(store, command, args, on_ok, || {});
}

pub fn run_then<A, R>(
    store: Store,
    command: &'static str,
    args: A,
    on_ok: impl FnOnce(R) + 'static,
    on_done: impl FnOnce() + 'static,
) where
    A: Schema + 'static,
    R: Schema + 'static,
{
    spawn_local(async move {
        match ipc::call::<A, R>(command, &args).await {
            Ok(result) => on_ok(result),
            Err(e) => store.error.set(Some(format!("{command}: {e}"))),
        }
        on_done();
    });
}

pub fn apply_status(store: Store, status: &SessionStatusDto) {
    store.elapsed_base_ms.set(status.elapsed_ms);
    store.session_bytes.set(status.bytes);
    store.running_since_ms.set(js_sys::Date::now());
    store.state.set(status.state);
    if status.state != SessionState::Recording {
        store.levels.set(HashMap::new());
    }
}

pub fn run_status(store: Store, command: &'static str) {
    run(store, command, NoArgs::default(), move |s: SessionStatusDto| {
        apply_status(store, &s);
        if s.state == SessionState::Idle {
            refresh_sessions(store);
        }
    });
}

pub fn refresh_devices(store: Store) {
    run(store, "list_devices", NoArgs::default(), move |list: DeviceList| store.devices.set(list.devices));
}

pub fn refresh_status(store: Store) {
    run(store, "session_status", NoArgs::default(), move |s: SessionStatusDto| apply_status(store, &s));
}

pub fn refresh_sessions(store: Store) {
    run(store, "list_sessions", NoArgs::default(), move |l: SessionList| store.sessions.set(l.sessions));
}

pub fn refresh_jobs(store: Store) {
    run(store, "list_jobs", NoArgs::default(), move |l: JobList| apply_jobs(store, l.jobs));
}

pub fn apply_jobs(store: Store, jobs: Vec<JobDto>) {
    let previous = store.jobs.get_untracked();
    let finished_now = jobs.iter().any(|j| {
        j.state == JobDtoState::Done && previous.iter().find(|p| p.id == j.id).is_some_and(|p| p.state != JobDtoState::Done)
    });
    store.jobs.set(jobs);
    if finished_now {
        refresh_sessions(store);
        store.transcript_rev.update(|r| *r += 1);
    }
    let compressed: Vec<String> = store
        .jobs
        .get_untracked()
        .iter()
        .filter(|j| j.kind == JobDtoKind::Compress && j.state == JobDtoState::Done)
        .filter(|j| previous.iter().find(|p| p.id == j.id).is_some_and(|p| p.state != JobDtoState::Done))
        .map(|j| j.session.clone())
        .collect();
    for name in compressed {
        if store.detail.get_untracked().is_some_and(|d| d.summary.name == name) {
            run(store, "get_session", SessionNameArgs { name }, move |d: SessionDetailDto| store.detail.set(Some(d)));
        }
    }
}

pub fn on_recorder_event(store: Store, ev: RecorderEventDto) {
    match ev {
        RecorderEventDto::DeviceChanged { device } => store.devices.update(|list| {
            match list.iter_mut().find(|d| d.key == device.key) {
                Some(existing) => *existing = device,
                None => list.push(device),
            }
        }),
        RecorderEventDto::SessionStarted { .. }
        | RecorderEventDto::SessionPaused { .. }
        | RecorderEventDto::SessionResumed { .. } => refresh_status(store),
        RecorderEventDto::SessionStopped { .. } => {
            refresh_status(store);
            refresh_sessions(store);
        }
        RecorderEventDto::Level { device, peak } => store.levels.update(|m| {
            m.insert(device, peak);
        }),
        RecorderEventDto::CaptureError { device, message } => {
            store.error.set(Some(format!("{device}: {message}")));
        }
        RecorderEventDto::SegmentOpened { .. } | RecorderEventDto::SegmentClosed { .. } => {}
    }
}

pub fn refresh_settings(store: Store) {
    run(store, "get_settings", NoArgs::default(), move |s: SettingsDto| set_theme_local(store, &s.theme));
}

fn set_theme_local(store: Store, id: &str) {
    store.themes.with_value(|themes| theme::select(themes, id));
    store.theme.set(id.to_string());
}

pub fn choose_theme(store: Store, id: String) {
    set_theme_local(store, &id);
    run(store, "set_settings", SettingsDto { theme: id }, |_: SettingsDto| {});
}

pub fn on_live_event(store: Store, ev: LiveTranscriptEventDto) {
    store.live.update(|l| {
        let enter = |l: &mut LiveView, name: String| {
            if l.name != name {
                *l = LiveView { name, ..LiveView::default() };
            }
        };
        match ev {
            LiveTranscriptEventDto::Snippet { name, description, snippet } => {
                enter(l, name);
                l.snippets.push((snippet, description));
            }
            LiveTranscriptEventDto::Partial { name, source, description, text, .. } => {
                enter(l, name);
                l.partials.retain(|(s, _, _)| *s != source);
                if let Some(text) = text {
                    l.partials.push((source, description, text));
                }
            }
            LiveTranscriptEventDto::Status { name, state, message, fraction } => {
                enter(l, name);
                l.status = match state.as_str() {
                    "downloading" => {
                        let what = message.unwrap_or_else(|| "Downloading live model".into());
                        Some(match fraction {
                            Some(f) => format!("{what} · {:.0}%", f * 100.0),
                            None => what,
                        })
                    }
                    "loading" => Some("Loading live model".into()),
                    "failed" => Some(format!("Live transcription unavailable: {}", message.unwrap_or_default())),
                    _ => None,
                };
            }
        }
    });
}

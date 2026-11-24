mod commands;
mod dto;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use audio_recorder_core::{
    ConfigError,
    balance, ResolvedConfig,
    jobs::JobStore,
    logging, paths,
    playback::Playback,
    session::{Recorder, RecorderError, RecorderEvent},
    settings::SettingsStore,
};
use audio_recorder_schema::{
    ipc::{ConfirmQuitDto, JobList, MixChangedDto, SessionState},
    jobs::JobKind,
};
use audio_recorder_transcribe::{
    live::LiveTranscriber,
    worker::{self, JobRunner},
};
use audio_recorder_schema::{Schema, to_value};
use tauri::{
    AppHandle, Emitter, Manager, WindowEvent,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

const TRAY_ID: &str = "main";
const EVENT_RECORDER: &str = "recorder://event";
const EVENT_PLAYER: &str = "player://state";
const EVENT_MIX: &str = "mix://changed";
const EVENT_LIVE: &str = "live://transcript";
const EVENT_JOBS: &str = "jobs://changed";
const EVENT_CONFIRM_QUIT: &str = "app://confirm-quit";
const QUIT_WORKER_WAIT: Duration = Duration::from_secs(8);
const ICON_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const ICON_RECORDING: &[u8] = include_bytes!("../icons/tray-recording.png");

pub struct AppState {
    pub recorder: Recorder,
    pub playback: Playback,
    pub config: ResolvedConfig,
    pub settings: Mutex<SettingsStore>,
    pub jobs: JobStore,
    live: Option<LiveTranscriber>,
    live_session: Mutex<String>,
    worker_stop: Arc<AtomicBool>,
    worker_active: AtomicBool,
    balancing: Mutex<HashSet<PathBuf>>,
    tray_items: Mutex<Option<TrayItems>>,
}

struct TrayItems {
    toggle: MenuItem<tauri::Wry>,
    pause: MenuItem<tauri::Wry>,
}

#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Paths(#[from] paths::PathsError),
    #[error(transparent)]
    Recorder(#[from] RecorderError),
    #[error(transparent)]
    Jobs(audio_recorder_core::jobs::JobError),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn emit_validated<T: Schema>(app: &AppHandle, event: &str, payload: &T) {
    match to_value(payload) {
        Ok(value) => {
            let _ = app.emit(event, value);
        }
        Err(e) => tracing::warn!("dropping invalid {event} payload: {e}"),
    }
}

fn set_tray_state(app: &AppHandle, session: SessionState) {
    let recording = session == SessionState::Recording;
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(icon) = Image::from_bytes(if recording { ICON_RECORDING } else { ICON_IDLE }) {
            let _ = tray.set_icon(Some(icon));
        }
        let tooltip = match session {
            SessionState::Idle => "Audio Recorder",
            SessionState::Recording => "Audio Recorder (recording)",
            SessionState::Paused => "Audio Recorder (paused)",
        };
        let _ = tray.set_tooltip(Some(tooltip));
    }
    let state = app.state::<AppState>();
    if let Some(items) = state.tray_items.lock().expect("tray items lock").as_ref() {
        let _ = items.toggle.set_text(if session == SessionState::Idle { "Start recording" } else { "Stop recording" });
        let _ = items.pause.set_enabled(session != SessionState::Idle);
        let _ = items.pause.set_text(if session == SessionState::Paused { "Resume recording" } else { "Pause recording" });
    }
}

pub fn kick_worker(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.worker_stop.load(Ordering::Relaxed) || state.worker_active.swap(true, Ordering::AcqRel) {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let runner = JobRunner::new(&state.config);
        if let Err(e) = worker::run_until_idle(&state.jobs, &runner, &state.worker_stop) {
            tracing::error!("transcription worker failed: {e}");
        }
        state.worker_active.store(false, Ordering::Release);
    });
}

fn enqueue_post_session_jobs(app: &AppHandle, dir: &Path) {
    let state = app.state::<AppState>();
    let name = dto::session_name(dir);
    let wanted = [
        (JobKind::Transcribe, state.config.transcription.auto),
        (JobKind::Compress, state.config.compression.auto),
    ];
    let mut queued = false;
    for (kind, enabled) in wanted {
        if !enabled {
            continue;
        }
        match state.jobs.enqueue(kind, &name, false) {
            Ok(_) => queued = true,
            Err(e) => tracing::error!("could not queue {kind} for {}: {e}", dir.display()),
        }
    }
    if queued {
        kick_worker(app);
    }
}

pub fn request_quit(app: &AppHandle, force: bool) {
    let state = app.state::<AppState>();
    let active = state.jobs.active_count().unwrap_or(0);
    if active > 0 && !force {
        show_main(app);
        emit_validated(app, EVENT_CONFIRM_QUIT, &ConfirmQuitDto { active: active as u64 });
        return;
    }
    quit(app);
}

fn quit(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(dir) = state.recorder.stop_session() {
        enqueue_post_session_jobs(app, &dir);
    }
    state.playback.stop();
    state.worker_stop.store(true, Ordering::Relaxed);
    let deadline = Instant::now() + QUIT_WORKER_WAIT;
    while state.worker_active.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    app.exit(0);
}

fn build_tray(app: &AppHandle) -> tauri::Result<TrayItems> {
    let show = MenuItem::with_id(app, "show", "Show window", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Start recording", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause recording", false, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &pause, &separator, &quit_item])?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(ICON_IDLE)?)
        .tooltip("Audio Recorder")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main(app),
            "toggle" => {
                let state = app.state::<AppState>();
                if let Err(e) = commands::toggle(&state) {
                    tracing::error!("toggle recording failed: {e}");
                }
            }
            "pause" => {
                let state = app.state::<AppState>();
                if let Err(e) = commands::toggle_pause(&state) {
                    tracing::error!("pause recording failed: {e}");
                }
            }
            "quit" => request_quit(app, false),
            _ => {}
        })
        .build(app)?;
    Ok(TrayItems { toggle, pause })
}

pub fn spawn_auto_balance(app: &AppHandle, dir: PathBuf) {
    let state = app.state::<AppState>();
    if !state.balancing.lock().expect("balancing lock").insert(dir.clone()) {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        match balance::ensure_default(&dir) {
            Ok(Some(settings)) => {
                let event = MixChangedDto { name: dto::session_name(&dir), mix: dto::mix_to_dto(&settings) };
                emit_validated(&handle, EVENT_MIX, &event);
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("auto-balance of {} failed: {e}", dir.display()),
        }
        handle.state::<AppState>().balancing.lock().expect("balancing lock").remove(&dir);
    });
}

fn start_live(app: &AppHandle, dir: &Path) {
    let state = app.state::<AppState>();
    let Some(live) = &state.live else { return };
    *state.live_session.lock().expect("live session lock") = dto::session_name(dir);
    if let Err(e) = live.start_session(dir.to_path_buf()) {
        tracing::warn!("could not start live transcription: {e}");
    }
}

fn finish_live(app: &AppHandle) {
    if app.state::<AppState>().live.is_none() {
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        if let Some(live) = &state.live
            && let Err(e) = live.finish_session()
        {
            tracing::warn!("live transcription did not finish cleanly: {e}");
        }
    });
}

fn spawn_live_bridge(app: &AppHandle) {
    let Some(events) = app.state::<AppState>().live.as_ref().map(LiveTranscriber::events) else { return };
    let handle = app.clone();
    std::thread::spawn(move || {
        while let Ok(ev) = events.recv() {
            let name = handle.state::<AppState>().live_session.lock().expect("live session lock").clone();
            emit_validated(&handle, EVENT_LIVE, &dto::live_event(&name, &ev));
        }
    });
}

fn spawn_bridges(app: &AppHandle) {
    spawn_live_bridge(app);
    let events = app.state::<AppState>().recorder.subscribe();
    let handle = app.clone();
    std::thread::spawn(move || {
        while let Ok(ev) = events.recv() {
            match &ev {
                RecorderEvent::SessionStarted { dir } => {
                    set_tray_state(&handle, SessionState::Recording);
                    start_live(&handle, dir);
                }
                RecorderEvent::SessionResumed { .. } => set_tray_state(&handle, SessionState::Recording),
                RecorderEvent::SessionPaused { .. } => set_tray_state(&handle, SessionState::Paused),
                RecorderEvent::SessionStopped { dir } => {
                    set_tray_state(&handle, SessionState::Idle);
                    spawn_auto_balance(&handle, dir.clone());
                    enqueue_post_session_jobs(&handle, dir);
                    finish_live(&handle);
                }
                _ => {}
            }
            if let Some(dto) = dto::event(&ev) {
                emit_validated(&handle, EVENT_RECORDER, &dto);
            }
        }
    });

    let handle = app.clone();
    std::thread::spawn(move || {
        let mut last: Option<String> = None;
        loop {
            let state = handle.state::<AppState>();
            if let Ok(jobs) = state.jobs.list() {
                let payload = JobList { jobs: jobs.iter().map(dto::job).collect() };
                let current = serde_json::to_string(&payload).ok();
                if current != last {
                    emit_validated(&handle, EVENT_JOBS, &payload);
                    last = current;
                }
                let queued = jobs.iter().any(|j| j.state == audio_recorder_schema::jobs::JobState::Queued);
                if queued && !state.worker_active.load(Ordering::Acquire) && !state.jobs.worker_running().unwrap_or(true) {
                    kick_worker(&handle);
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });

    let handle = app.clone();
    std::thread::spawn(move || {
        let mut last = None;
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let state = dto::playback(&handle.state::<AppState>().playback.state());
            let current = serde_json::to_string(&state).ok();
            if current != last {
                emit_validated(&handle, EVENT_PLAYER, &state);
                last = current;
            }
        }
    });
}

fn setup(app: &mut tauri::App) -> Result<(), StartupError> {
    let config = ResolvedConfig::load()?;
    let recorder = Recorder::start(config.clone(), &paths::device_state_file()?)?;
    let settings_path = paths::settings_file()?;
    let settings = SettingsStore::load(&settings_path).unwrap_or_else(|e| {
        tracing::warn!("{e}; using default settings");
        SettingsStore::defaults(&settings_path)
    });
    let live = config
        .transcription
        .live
        .enabled
        .then(|| LiveTranscriber::spawn(config.transcription.clone(), recorder.subscribe_audio()));
    app.manage(AppState {
        recorder,
        playback: Playback::start(),
        config,
        settings: Mutex::new(settings),
        jobs: JobStore::open_default().map_err(StartupError::Jobs)?,
        live,
        live_session: Mutex::new(String::new()),
        worker_stop: Arc::new(AtomicBool::new(false)),
        worker_active: AtomicBool::new(false),
        balancing: Mutex::new(HashSet::new()),
        tray_items: Mutex::new(None),
    });
    let items = build_tray(app.handle())?;
    *app.state::<AppState>().tray_items.lock().expect("tray items lock") = Some(items);
    spawn_bridges(app.handle());
    Ok(())
}

fn main() {
    for (var, value) in [("WEBKIT_DISABLE_DMABUF_RENDERER", "1")] {
        if std::env::var_os(var).is_none() {
            // SAFETY: called first thing in main, before any other thread exists.
            unsafe { std::env::set_var(var, value) };
        }
    }
    let config = ResolvedConfig::load();
    let _log_guard = logging::init_from(config.as_ref().ok());
    if let Err(e) = &config {
        tracing::error!("{e}");
    }
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .setup(|app| setup(app).map_err(Into::into))
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_devices,
            commands::set_device_enabled,
            commands::session_status,
            commands::start_session,
            commands::stop_session,
            commands::pause_session,
            commands::resume_session,
            commands::list_sessions,
            commands::get_session,
            commands::set_mix,
            commands::auto_balance,
            commands::reset_mix,
            commands::delete_session,
            commands::play,
            commands::pause,
            commands::resume,
            commands::seek,
            commands::stop_playback,
            commands::playback_state,
            commands::get_settings,
            commands::set_settings,
            commands::list_jobs,
            commands::enqueue_transcription,
            commands::enqueue_compression,
            commands::enqueue_summary,
            commands::get_summary,
            commands::cancel_job,
            commands::clear_finished_jobs,
            commands::get_transcript,
            commands::render_transcript,
            commands::quit_app,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        tracing::error!("{e}");
        drop(_log_guard);
        std::process::exit(1);
    }
}

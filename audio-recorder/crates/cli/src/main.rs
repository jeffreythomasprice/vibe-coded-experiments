use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use audio_recorder_core::{
    balance::{self, BalanceError},
    ConfigError, ResolvedConfig,
    compress,
    session::manifest::ManifestError,
    jobs::{JobError, JobStore},
    logging,
    device::{DeviceKey, DeviceStatus},
    library::{self, LibraryError},
    mixdown::{self, MixdownError},
    paths,
    playback::{Playback, PlaybackError},
    session::{Recorder, RecorderError, RecorderEvent},
    summary,
    transcript::{self, RenderOptions, TranscriptError},
};
use audio_recorder_schema::jobs::{Job, JobKind, JobState};
use audio_recorder_transcribe::{
    live::{LiveEvent, LiveStatus, LiveTranscriber},
    models::{self, ModelError},
    worker::{self, JobRunner},
};
use clap::{Parser, Subcommand};
use crossbeam_channel::select;

#[derive(Parser)]
#[command(name = "audio-recorder", about = "Record input and output audio devices")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the resolved configuration and where it came from
    Config,
    /// List audio devices and whether they are enabled
    Devices {
        /// Keep running and print devices as they appear and disappear
        #[arg(long)]
        watch: bool,
    },
    /// Enable a device (by index from `devices` or by key)
    Enable { device: String },
    /// Disable a device (by index from `devices` or by key)
    Disable { device: String },
    /// Record a session until Ctrl-C
    Record {
        /// Transcribe live while recording, overriding the config
        #[arg(long, conflicts_with = "no_live")]
        live: bool,
        /// Do not transcribe live while recording, overriding the config
        #[arg(long)]
        no_live: bool,
    },
    /// List previous sessions
    Sessions {
        /// Print the full path of each session
        #[arg(short, long)]
        verbose: bool,
    },
    /// Play a session's combined audio, or one device's track
    Play {
        /// Session name, path, or "latest"
        session: String,
        /// Only play this device (key from `devices`)
        #[arg(long)]
        track: Option<String>,
    },
    /// Write a session's combined audio (or one track) to a WAV file
    Mixdown {
        /// Session name, path, or "latest"
        session: String,
        #[arg(long)]
        track: Option<String>,
        /// Output file (defaults to mix.wav inside the session)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Queue sessions for transcription and process the queue
    Transcribe {
        /// Session names, paths, or "latest"
        #[arg(required = true)]
        sessions: Vec<String>,
        /// Replace an existing transcript
        #[arg(long)]
        replace: bool,
        /// Do not ask before replacing
        #[arg(short, long)]
        yes: bool,
        /// Only queue the sessions; process later with `tasks run` or the app
        #[arg(long)]
        no_wait: bool,
    },
    /// Queue sessions for track compression (WAV to FLAC or Opus) and process the queue
    Compress {
        /// Session names, paths, or "latest"
        #[arg(required = true)]
        sessions: Vec<String>,
        /// Only queue the sessions; process later with `tasks run` or the app
        #[arg(long)]
        no_wait: bool,
    },
    /// Queue sessions for an LLM summary of their transcript and process the queue
    Summarize {
        /// Session names, paths, or "latest"
        #[arg(required = true)]
        sessions: Vec<String>,
        /// Replace an existing summary
        #[arg(long)]
        replace: bool,
        /// Do not ask before replacing
        #[arg(short, long)]
        yes: bool,
        /// Only queue the sessions; process later with `tasks run` or the app
        #[arg(long)]
        no_wait: bool,
    },
    /// Print a session's summary
    Summary {
        /// Session name, path, or "latest"
        session: String,
        /// Write to a file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print a session's transcript
    Transcript {
        /// Session name, path, or "latest"
        session: String,
        /// Include the source (device) of each snippet
        #[arg(long)]
        source: bool,
        /// Include the speaker of each snippet
        #[arg(long)]
        speaker: bool,
        /// Include the start time of each snippet
        #[arg(long)]
        timestamps: bool,
        /// Output the full transcript data as JSON (ignores the tag options)
        #[arg(long)]
        json: bool,
        /// Write to a file instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Show and manage background tasks
    Tasks {
        #[command(subcommand)]
        command: Option<TasksCommand>,
        /// Keep refreshing the list until Ctrl-C
        #[arg(long)]
        watch: bool,
    },
    /// Manage transcription models
    Models {
        #[command(subcommand)]
        command: ModelsCommand,
    },
}

#[derive(Subcommand)]
enum TasksCommand {
    /// Cancel a queued or running task (by session name, id, or unique id prefix)
    Cancel { id: String },
    /// Remove finished tasks from the list
    Clear,
    /// Process queued tasks in the foreground
    Run,
}

#[derive(Subcommand)]
enum ModelsCommand {
    /// Show model paths and whether they are present
    List,
    /// Download any missing models
    Download,
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Recorder(#[from] RecorderError),
    #[error(transparent)]
    Live(#[from] audio_recorder_transcribe::live::LiveError),
    #[error(transparent)]
    Paths(#[from] paths::PathsError),
    #[error("could not install Ctrl-C handler: {0}")]
    Signal(#[from] ctrlc::Error),
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error(transparent)]
    Mixdown(#[from] MixdownError),
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    #[error(transparent)]
    Balance(#[from] BalanceError),
    #[error(transparent)]
    Jobs(#[from] JobError),
    #[error(transparent)]
    Transcript(#[from] TranscriptError),
    #[error(transparent)]
    Models(#[from] ModelError),
    #[error("failed to write {}: {source}", path.display())]
    Write { path: PathBuf, source: io::Error },
    #[error("could not read confirmation: {0}")]
    Prompt(io::Error),
    #[error("{0} already has a transcript; pass --replace to replace it")]
    AlreadyTranscribed(String),
    #[error("{0} has no transcript; run `transcribe` first")]
    NoTranscript(String),
    #[error("{0} already has a summary; pass --replace to replace it")]
    AlreadySummarized(String),
    #[error("{0} has no summary; run `summarize` first")]
    NoSummary(String),
    #[error(transparent)]
    Summary(#[from] audio_recorder_core::summary::SummaryError),
    #[error("{} is not inside the sessions directory {}", path.display(), sessions.display())]
    OutsideSessions { path: PathBuf, sessions: PathBuf },
    #[error("no task matches {0:?}")]
    NoSuchJob(String),
    #[error("{0:?} matches more than one task")]
    AmbiguousJob(String),
    #[error("no sessions found in {}", .0.display())]
    NoSessions(PathBuf),
    #[error("no device matches {0:?}")]
    NoSuchDevice(String),
    #[error("{0}")]
    BadKey(#[from] audio_recorder_core::device::ParseKeyError),
}

fn start_recorder(config: &ResolvedConfig) -> Result<Recorder, CliError> {
    let state = paths::device_state_file()?;
    Ok(Recorder::start(config.clone(), &state)?)
}

fn describe(d: &DeviceStatus) -> String {
    format!(
        "{} {}  {}{}{}",
        if d.enabled { "[x]" } else { "[ ]" },
        d.key,
        d.description,
        if d.present { "" } else { "  (not connected)" },
        if d.recording { "  (recording)" } else { "" },
    )
}

fn resolve_device(recorder: &Recorder, arg: &str) -> Result<DeviceKey, CliError> {
    if let Ok(index) = arg.parse::<usize>() {
        let devices = recorder.devices()?;
        return devices
            .get(index)
            .map(|d| d.key.clone())
            .ok_or_else(|| CliError::NoSuchDevice(arg.to_string()));
    }
    Ok(arg.parse()?)
}

fn resolve_session(config: &ResolvedConfig, arg: &str) -> Result<PathBuf, CliError> {
    if arg == "latest" {
        return library::list_sessions(&config.sessions_dir)?
            .into_iter()
            .next()
            .map(|s| s.dir)
            .ok_or_else(|| CliError::NoSessions(config.sessions_dir.clone()));
    }
    if arg.contains('/') {
        return Ok(PathBuf::from(arg));
    }
    Ok(library::session_dir(&config.sessions_dir, arg)?)
}

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn interrupt_channel() -> Result<crossbeam_channel::Receiver<()>, CliError> {
    let (tx, rx) = crossbeam_channel::bounded(8);
    ctrlc::set_handler(move || {
        let _ = tx.try_send(());
    })?;
    Ok(rx)
}

fn session_name(config: &ResolvedConfig, dir: &Path) -> Result<String, CliError> {
    let outside = || CliError::OutsideSessions { path: dir.to_path_buf(), sessions: config.sessions_dir.clone() };
    match (dir.parent(), dir.file_name()) {
        (Some(parent), Some(name)) if parent == config.sessions_dir => Ok(name.to_string_lossy().into_owned()),
        _ => Err(outside()),
    }
}

fn runner(config: &ResolvedConfig) -> JobRunner {
    JobRunner::new(config)
}

fn queue(store: &JobStore, kind: JobKind, name: &str, replace: bool) -> Result<(), CliError> {
    let (job, created) = store.enqueue(kind, name, replace)?;
    if created {
        println!("queued {} for {name}", job.id);
    } else {
        println!("{name} already has an active task: {}", job.id);
    }
    Ok(())
}

fn describe_job(job: &Job) -> String {
    let state = match job.state {
        JobState::Queued => "queued",
        JobState::Running => "running",
        JobState::Done => "done",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
    };
    let detail = match (&job.stage, job.fraction, &job.error) {
        (Some(stage), Some(f), _) => format!("{stage} {:.0}%", f * 100.0),
        (Some(stage), None, _) => stage.clone(),
        (None, _, Some(error)) => error.clone(),
        _ => String::new(),
    };
    let kind = match job.kind {
        JobKind::Transcribe => "transcribe",
        JobKind::Compress => "compress",
        JobKind::Summarize => "summarize",
    };
    format!("{}  {kind:<10}  {state:<9}  {}  {detail}", job.id, job.session).trim_end().to_string()
}

struct SetOnDrop<'a>(&'a AtomicBool);

impl Drop for SetOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn run_worker_foreground(store: &JobStore, config: &ResolvedConfig, stop: &AtomicBool) -> Result<bool, CliError> {
    let runner = runner(config);
    let finished = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut last = String::new();
            while !finished.load(Ordering::Relaxed) {
                if let Ok(jobs) = store.list() {
                    if let Some(job) = jobs.iter().find(|j| j.state == JobState::Running) {
                        let line = describe_job(job);
                        if line != last {
                            tracing::info!("{line}");
                            last = line;
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
        let _done = SetOnDrop(&finished);
        worker::run_until_idle(store, &runner, stop)
    });
    let processed = result?;
    if processed.is_none() {
        tracing::info!("another worker (for example the app) is already processing the queue");
    }
    Ok(processed.is_some())
}

fn stop_flag(interrupted: crossbeam_channel::Receiver<()>) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        if interrupted.recv().is_ok() {
            flag.store(true, Ordering::Relaxed);
        }
    });
    stop
}

fn confirm(question: &str) -> Result<bool, CliError> {
    print!("{question} [y/N] ");
    io::stdout().flush().map_err(CliError::Prompt)?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line).map_err(CliError::Prompt)?;
    Ok(matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn resolve_job(store: &JobStore, arg: &str) -> Result<String, CliError> {
    let jobs = store.list()?;
    if let Some(job) = jobs.iter().find(|j| j.session == arg && audio_recorder_core::jobs::is_active(&j.state)) {
        return Ok(job.id.clone());
    }
    let matches: Vec<_> = jobs.into_iter().filter(|j| j.id.starts_with(arg)).collect();
    match matches.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => Err(CliError::NoSuchJob(arg.to_string())),
        _ => match matches.iter().find(|j| j.id == arg) {
            Some(exact) => Ok(exact.id.clone()),
            None => Err(CliError::AmbiguousJob(arg.to_string())),
        },
    }
}

fn print_event(ev: &RecorderEvent) {
    match ev {
        RecorderEvent::DeviceChanged(d) => tracing::info!("device: {}", describe(d)),
        RecorderEvent::SessionStarted { dir } => tracing::info!("session started: {}", dir.display()),
        RecorderEvent::SessionStopped { dir } => tracing::info!("session stopped: {}", dir.display()),
        RecorderEvent::SessionPaused { dir } => tracing::info!("session paused: {}", dir.display()),
        RecorderEvent::SessionResumed { dir } => tracing::info!("session resumed: {}", dir.display()),
        RecorderEvent::SegmentOpened { key, file } => tracing::info!("segment opened: {key} -> {file}"),
        RecorderEvent::SegmentClosed { key, file } => tracing::info!("segment closed: {key} -> {file}"),
        RecorderEvent::CaptureError { key, message } => tracing::error!("capture error on {key}: {message}"),
        RecorderEvent::Level { .. } => {}
    }
}

fn print_live(ev: &LiveEvent) {
    match ev {
        LiveEvent::Snippet { snippet, description } => {
            println!("[{}] {description}: {}", transcript::format_timestamp(snippet.start_ns), snippet.text.trim());
        }
        LiveEvent::Status(LiveStatus::Downloading { label, fraction }) => match fraction {
            Some(f) => tracing::info!("live: downloading {label} model ({:.0}%)", f * 100.0),
            None => tracing::info!("live: downloading {label} model"),
        },
        LiveEvent::Status(LiveStatus::Loading) => tracing::info!("live: loading model"),
        LiveEvent::Status(LiveStatus::Ready) => tracing::info!("live: transcribing"),
        LiveEvent::Status(LiveStatus::Failed(message)) => tracing::error!("live transcription unavailable: {message}"),
        LiveEvent::Partial { .. } => {}
    }
}

fn run(cli: Cli, config: &ResolvedConfig) -> Result<(), CliError> {
    match cli.command {
        Command::Config => {
            match &config.source {
                Some(path) => println!("source: {}", path.display()),
                None => println!("source: (defaults)"),
            }
            println!("sessions_dir: {}", config.sessions_dir.display());
            println!("sample_format: {:?}", config.sample_format);
            println!("log_file: {}", config.log_file.display());
            println!("transcription.auto: {}", config.transcription.auto);
        }
        Command::Devices { watch } => {
            let recorder = start_recorder(config)?;
            let events = recorder.subscribe();
            for (i, d) in recorder.devices()?.iter().enumerate() {
                println!("{i:>2}  {}", describe(d));
            }
            if watch {
                let interrupted = interrupt_channel()?;
                loop {
                    select! {
                        recv(events) -> ev => match ev {
                            Ok(ev) => print_event(&ev),
                            Err(_) => break,
                        },
                        recv(interrupted) -> _ => break,
                    }
                }
            }
        }
        Command::Enable { .. } | Command::Disable { .. } => unreachable!("dispatched in main"),
        Command::Record { live, no_live } => {
            let recorder = start_recorder(config)?;
            let events = recorder.subscribe();
            let interrupted = interrupt_channel()?;
            let live_enabled = live || (!no_live && config.transcription.live.enabled);
            let transcriber = live_enabled.then(|| {
                let mut settings = config.transcription.clone();
                settings.live.enabled = true;
                LiveTranscriber::spawn(settings, recorder.subscribe_audio())
            });
            let live_events = transcriber.as_ref().map(LiveTranscriber::events).unwrap_or_else(crossbeam_channel::never);
            let dir = recorder.start_session()?;
            if let Some(t) = &transcriber {
                t.start_session(dir)?;
            }
            loop {
                select! {
                    recv(events) -> ev => match ev {
                        Ok(ev) => print_event(&ev),
                        Err(_) => break,
                    },
                    recv(live_events) -> ev => if let Ok(ev) = ev { print_live(&ev) },
                    recv(interrupted) -> _ => break,
                }
            }
            let dir = recorder.stop_session()?;
            if let Some(t) = &transcriber {
                if let Err(e) = t.finish_session() {
                    tracing::warn!("live transcription did not finish cleanly: {e}");
                }
                live_events.try_iter().for_each(|ev| print_live(&ev));
            }
            let name = session_name(config, &dir)?;
            let store = JobStore::open_default()?;
            let mut queued = false;
            if config.transcription.auto {
                queue(&store, JobKind::Transcribe, &name, false)?;
                queued = true;
            }
            if config.compression.auto {
                queue(&store, JobKind::Compress, &name, false)?;
                queued = true;
            }
            if queued {
                let stop = stop_flag(interrupted);
                run_worker_foreground(&store, config, &stop)?;
            }
        }
        Command::Sessions { verbose } => {
            for s in library::list_sessions(&config.sessions_dir)? {
                if verbose {
                    println!("{}", s.dir.display());
                }
                match &s.error {
                    Some(e) => println!("{}  (unreadable: {e})", s.name),
                    None => println!(
                        "{}  {}  {} segments, {} devices, {:.1} MiB{}{}",
                        s.name,
                        fmt_duration(s.duration),
                        s.segment_count,
                        s.device_count,
                        s.bytes as f64 / 1_048_576.0,
                        if s.recording { "  (recording)" } else { "" },
                        if s.has_transcript { "  (transcript)" } else { "" },
                    ),
                }
            }
        }
        Command::Play { session, track } => {
            let dir = resolve_session(config, &session)?;
            let track = track.map(|t| t.parse::<DeviceKey>()).transpose()?;
            let interrupted = interrupt_channel()?;
            let playback = Playback::start();
            playback.play(dir, track)?;
            loop {
                if interrupted.recv_timeout(Duration::from_millis(250)).is_ok() {
                    break;
                }
                let st = playback.state();
                print!("\r{} / {}", fmt_duration(st.position), fmt_duration(st.duration));
                let _ = std::io::Write::flush(&mut std::io::stdout());
                if !st.playing {
                    break;
                }
            }
            println!();
        }
        Command::Mixdown { session, track, output } => {
            let dir = resolve_session(config, &session)?;
            let track = track.map(|t| t.parse::<DeviceKey>()).transpose()?;
            let output = output.unwrap_or_else(|| dir.join("mix.wav"));
            balance::ensure_default(&dir)?;
            mixdown::mixdown_to_file(&dir, track.as_ref(), &output)?;
            println!("wrote {}", output.display());
        }
        Command::Transcribe { sessions, replace, yes, no_wait } => {
            let store = JobStore::open_default()?;
            for arg in &sessions {
                let dir = resolve_session(config, arg)?;
                let name = session_name(config, &dir)?;
                if transcript::exists(&dir) {
                    if !replace {
                        return Err(CliError::AlreadyTranscribed(name));
                    }
                    if !yes && !confirm(&format!("Replace existing transcript for {name}?"))? {
                        println!("skipped {name}");
                        continue;
                    }
                }
                let (job, created) = store.enqueue(JobKind::Transcribe, &name, replace)?;
                if created {
                    println!("queued {} for {name}", job.id);
                } else {
                    println!("{name} already has an active task: {}", job.id);
                }
            }
            if !no_wait {
                let stop = stop_flag(interrupt_channel()?);
                run_worker_foreground(&store, config, &stop)?;
            }
        }
        Command::Summarize { sessions, replace, yes, no_wait } => {
            let store = JobStore::open_default()?;
            for arg in &sessions {
                let dir = resolve_session(config, arg)?;
                let name = session_name(config, &dir)?;
                if !transcript::exists(&dir) {
                    return Err(CliError::NoTranscript(name));
                }
                if summary::exists(&dir) {
                    if !replace {
                        return Err(CliError::AlreadySummarized(name));
                    }
                    if !yes && !confirm(&format!("Replace existing summary for {name}?"))? {
                        println!("skipped {name}");
                        continue;
                    }
                }
                queue(&store, JobKind::Summarize, &name, replace)?;
            }
            if !no_wait {
                let stop = stop_flag(interrupt_channel()?);
                run_worker_foreground(&store, config, &stop)?;
            }
        }
        Command::Summary { session, output } => {
            let dir = resolve_session(config, &session)?;
            let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(session);
            let found = summary::load(&dir)?.ok_or(CliError::NoSummary(name))?;
            if transcript::load(&dir)?.is_some_and(|t| summary::is_stale(&found, &t)) {
                tracing::warn!("the transcript has changed since this summary was made; run `summarize --replace` to refresh it");
            }
            let text = format!("{}\n", found.text.trim_end());
            match output {
                Some(path) => {
                    std::fs::write(&path, &text).map_err(|source| CliError::Write { path: path.clone(), source })?;
                    println!("wrote {}", path.display());
                }
                None => print!("{text}"),
            }
        }
        Command::Compress { sessions, no_wait } => {
            let store = JobStore::open_default()?;
            for arg in &sessions {
                let dir = resolve_session(config, arg)?;
                let name = session_name(config, &dir)?;
                if compress::pending_segments(&dir, &config.compression)? == 0 {
                    println!("{name} has no uncompressed tracks");
                    continue;
                }
                queue(&store, JobKind::Compress, &name, false)?;
            }
            if !no_wait {
                let stop = stop_flag(interrupt_channel()?);
                run_worker_foreground(&store, config, &stop)?;
            }
        }
        Command::Transcript { session, source, speaker, timestamps, json, output } => {
            let dir = resolve_session(config, &session)?;
            let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(session);
            let (t, draft) = transcript::load_best(&dir)?.ok_or(CliError::NoTranscript(name))?;
            if draft {
                tracing::warn!("showing the live draft; the final transcript has not been made yet");
            }
            let text = if json { transcript::render_json(&t)? } else { transcript::render(&t, RenderOptions { source, speaker, timestamps }) };
            match output {
                Some(path) => {
                    std::fs::write(&path, &text).map_err(|source| CliError::Write { path: path.clone(), source })?;
                    println!("wrote {}", path.display());
                }
                None => print!("{text}"),
            }
        }
        Command::Tasks { command, watch } => {
            let store = JobStore::open_default()?;
            match command {
                Some(TasksCommand::Cancel { id }) => {
                    let id = resolve_job(&store, &id)?;
                    store.cancel(&id)?;
                    println!("cancelling {id}");
                }
                Some(TasksCommand::Clear) => store.clear_finished()?,
                Some(TasksCommand::Run) => {
                    let stop = stop_flag(interrupt_channel()?);
                run_worker_foreground(&store, config, &stop)?;
                }
                None => {
                    let interrupted = interrupt_channel()?;
                    loop {
                        if watch {
                            print!("\x1b[2J\x1b[H");
                        }
                        let jobs = store.list()?;
                        if jobs.is_empty() {
                            println!("no tasks");
                        }
                        for job in &jobs {
                            println!("{}", describe_job(job));
                        }
                        if !watch || interrupted.recv_timeout(Duration::from_secs(1)).is_ok() {
                            break;
                        }
                    }
                }
            }
        }
        Command::Models { command } => {
            let files = models::all(&config.transcription);
            if matches!(command, ModelsCommand::Download) {
                let mut last_pct = u64::MAX;
                models::ensure(
                    &files,
                    &mut |label, done, total| {
                        if let Some(total) = total.filter(|t| *t > 0) {
                            let pct = done * 100 / total;
                            if pct != last_pct && pct % 5 == 0 {
                                last_pct = pct;
                                tracing::info!("downloading {label}: {pct}%");
                            }
                        }
                    },
                    &|| false,
                )?;
            }
            for f in &files {
                println!(
                    "{:<12} {}  {}{}",
                    f.label,
                    if f.present() { "present" } else { "missing" },
                    f.path.display(),
                    if f.explicit { "  (from config)" } else { "" },
                );
            }
        }
    }
    Ok(())
}

fn set_enabled(config: &ResolvedConfig, device: &str, enabled: bool) -> Result<(), CliError> {
    let recorder = start_recorder(config)?;
    let key = resolve_device(&recorder, device)?;
    recorder.set_enabled(&key, enabled)?;
    println!("{} {key}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = match ResolvedConfig::load() {
        Ok(config) => config,
        Err(e) => {
            let _guard = logging::init_from(None);
            tracing::error!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let _guard = logging::init_from(Some(&config));
    let result = match &cli.command {
        Command::Enable { device } => set_enabled(&config, device, true),
        Command::Disable { device } => set_enabled(&config, device, false),
        _ => run(cli, &config),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}

pub mod manifest;
mod writer;

use std::{
    collections::{HashMap, HashSet},
    io,
    num::NonZeroU64,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use audio_recorder_schema::manifest::{Direction as ManifestDirection, Manifest, ManifestDevice, Segment, SegmentStatus};
use crossbeam_channel::{Receiver, Sender, select};
use time::{OffsetDateTime, format_description::well_known::Rfc3339, macros::format_description};

use crate::{
    config::ResolvedConfig,
    device::{DeviceInfo, DeviceKey, DeviceStatus, Direction},
    device_state::{DeviceStateError, DeviceStateStore},
    pw::{EndReason, PwError, PwEvent, PwHandle},
};
use manifest::ManifestError;
use writer::{AudioSubscribers, Internal, WriterCtx};

#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error(transparent)]
    Pw(#[from] PwError),
    #[error(transparent)]
    State(#[from] DeviceStateError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("failed to create {}: {source}", path.display())]
    CreateDir { path: PathBuf, source: io::Error },
    #[error("a session is already recording")]
    SessionActive,
    #[error("no session is recording")]
    NoSession,
    #[error("the session is already paused")]
    AlreadyPaused,
    #[error("the session is not paused")]
    NotPaused,
    #[error("recorder thread is not running")]
    Gone,
}

#[derive(Debug, Clone)]
pub enum RecorderEvent {
    DeviceChanged(DeviceStatus),
    SessionStarted { dir: PathBuf },
    SessionStopped { dir: PathBuf },
    SessionPaused { dir: PathBuf },
    SessionResumed { dir: PathBuf },
    SegmentOpened { key: DeviceKey, file: String },
    SegmentClosed { key: DeviceKey, file: String },
    Level { key: DeviceKey, peak: f32 },
    CaptureError { key: DeviceKey, message: String },
}

/// Raw audio as it is written to disk, in session time. `data` is interleaved.
#[derive(Debug, Clone)]
pub enum LiveAudio {
    Chunk { key: DeviceKey, file: String, offset_ns: u64, rate: u32, channels: u32, data: Arc<[f32]> },
    SegmentEnd { key: DeviceKey, file: String },
}

pub const LIVE_AUDIO_QUEUE: usize = 4096;

#[derive(Debug, Clone)]
pub struct SessionStatus {
    pub dir: PathBuf,
    pub elapsed: Duration,
    pub paused: bool,
    pub bytes: u64,
}

enum Cmd {
    Devices(Sender<Vec<DeviceStatus>>),
    SetEnabled { key: DeviceKey, enabled: bool, reply: Sender<Result<(), RecorderError>> },
    Start(Sender<Result<PathBuf, RecorderError>>),
    Stop(Sender<Result<PathBuf, RecorderError>>),
    Pause(Sender<Result<(), RecorderError>>),
    Resume(Sender<Result<(), RecorderError>>),
    Status(Sender<Option<SessionStatus>>),
    Shutdown,
}

type Subscribers = Arc<Mutex<Vec<Sender<RecorderEvent>>>>;

pub struct Recorder {
    cmd: Sender<Cmd>,
    subscribers: Subscribers,
    audio: AudioSubscribers,
    join: Option<JoinHandle<()>>,
}

impl Recorder {
    pub fn start(config: ResolvedConfig, state_path: &Path) -> Result<Self, RecorderError> {
        let store = DeviceStateStore::load(state_path)?;
        let pw = PwHandle::spawn()?;
        let initial = pw.wait_synced(Duration::from_secs(5))?;
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (internal_tx, internal_rx) = crossbeam_channel::unbounded();
        let subscribers: Subscribers = Arc::default();
        let audio: AudioSubscribers = Arc::default();

        let mut coord = Coordinator {
            config,
            pw,
            store,
            devices: HashMap::new(),
            active: None,
            next_id: 1,
            internal_tx,
            subscribers: subscribers.clone(),
            audio: audio.clone(),
        };
        for (key, description, _) in coord.store.known() {
            coord.devices.entry(key).or_insert(Dev { description, pw_id: None });
        }
        for ev in initial {
            coord.on_pw(ev);
        }

        let join = std::thread::Builder::new()
            .name("recorder".into())
            .spawn(move || coord.run(cmd_rx, internal_rx))
            .map_err(|_| RecorderError::Gone)?;
        Ok(Self { cmd: cmd_tx, subscribers, audio, join: Some(join) })
    }

    pub fn subscribe(&self) -> Receiver<RecorderEvent> {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.subscribers.lock().expect("subscribers lock").push(tx);
        rx
    }

    pub fn subscribe_audio(&self) -> Receiver<LiveAudio> {
        let (tx, rx) = crossbeam_channel::bounded(LIVE_AUDIO_QUEUE);
        self.audio.lock().expect("audio subscribers lock").push(tx);
        rx
    }

    fn call<T>(&self, make: impl FnOnce(Sender<T>) -> Cmd) -> Result<T, RecorderError> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.cmd.send(make(tx)).map_err(|_| RecorderError::Gone)?;
        rx.recv().map_err(|_| RecorderError::Gone)
    }

    pub fn devices(&self) -> Result<Vec<DeviceStatus>, RecorderError> {
        self.call(Cmd::Devices)
    }

    pub fn set_enabled(&self, key: &DeviceKey, enabled: bool) -> Result<(), RecorderError> {
        self.call(|reply| Cmd::SetEnabled { key: key.clone(), enabled, reply })?
    }

    pub fn start_session(&self) -> Result<PathBuf, RecorderError> {
        self.call(Cmd::Start)?
    }

    pub fn stop_session(&self) -> Result<PathBuf, RecorderError> {
        self.call(Cmd::Stop)?
    }

    pub fn pause_session(&self) -> Result<(), RecorderError> {
        self.call(Cmd::Pause)?
    }

    pub fn resume_session(&self) -> Result<(), RecorderError> {
        self.call(Cmd::Resume)?
    }

    pub fn session_status(&self) -> Result<Option<SessionStatus>, RecorderError> {
        self.call(Cmd::Status)
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.cmd.send(Cmd::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct Dev {
    description: String,
    pw_id: Option<u32>,
}

struct Active {
    paused_since: Option<Instant>,
    paused_total: Duration,
    dir: PathBuf,
    manifest: Manifest,
    t0: Instant,
    captures: HashMap<DeviceKey, u64>,
    owners: HashMap<u64, (DeviceKey, String)>,
    segments: HashMap<u64, usize>,
    pending_writers: HashSet<u64>,
    counters: HashMap<DeviceKey, u32>,
}

impl Active {
    fn recorded_elapsed(&self) -> Duration {
        let until = self.paused_since.unwrap_or_else(Instant::now);
        until.saturating_duration_since(self.t0).saturating_sub(self.paused_total)
    }
}

struct Coordinator {
    config: ResolvedConfig,
    pw: PwHandle,
    store: DeviceStateStore,
    devices: HashMap<DeviceKey, Dev>,
    active: Option<Active>,
    next_id: u64,
    internal_tx: Sender<Internal>,
    subscribers: Subscribers,
    audio: AudioSubscribers,
}

fn rfc3339_now() -> String {
    OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default()
}

fn dir_name_now() -> String {
    let fmt = format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    OffsetDateTime::now_utc().format(&fmt).unwrap_or_else(|_| "session".into())
}

impl Coordinator {
    fn run(mut self, cmd_rx: Receiver<Cmd>, internal_rx: Receiver<Internal>) {
        let pw_events = self.pw.events.clone();
        loop {
            select! {
                recv(cmd_rx) -> cmd => match cmd {
                    Ok(Cmd::Shutdown) | Err(_) => break,
                    Ok(cmd) => self.on_cmd(cmd, &internal_rx),
                },
                recv(pw_events) -> ev => match ev {
                    Ok(ev) => self.on_pw(ev),
                    Err(_) => break,
                },
                recv(internal_rx) -> msg => if let Ok(msg) = msg { self.on_internal(msg) },
            }
        }
        if self.active.is_some() {
            let _ = self.stop_session(&internal_rx);
        }
    }

    fn emit(&self, ev: RecorderEvent) {
        let mut subs = self.subscribers.lock().expect("subscribers lock");
        subs.retain(|s| s.send(ev.clone()).is_ok());
    }

    fn status_of(&self, key: &DeviceKey) -> DeviceStatus {
        let dev = self.devices.get(key);
        DeviceStatus {
            key: key.clone(),
            description: dev.map(|d| d.description.clone()).unwrap_or_else(|| key.node_name.clone()),
            present: dev.is_some_and(|d| d.pw_id.is_some()),
            enabled: self.store.is_enabled(key),
            recording: self.active.as_ref().is_some_and(|a| a.captures.contains_key(key)),
        }
    }

    fn on_cmd(&mut self, cmd: Cmd, internal_rx: &Receiver<Internal>) {
        match cmd {
            Cmd::Devices(reply) => {
                let mut list: Vec<_> = self.devices.keys().map(|k| self.status_of(k)).collect();
                list.sort_by(|a, b| (a.key.direction, &a.description).cmp(&(b.key.direction, &b.description)));
                let _ = reply.send(list);
            }
            Cmd::SetEnabled { key, enabled, reply } => {
                let description = self
                    .devices
                    .get(&key)
                    .map(|d| d.description.clone())
                    .unwrap_or_else(|| key.node_name.clone());
                let result = self.store.set_enabled(&key, &description, enabled).map_err(RecorderError::from);
                if result.is_ok() {
                    self.devices.entry(key.clone()).or_insert(Dev { description, pw_id: None });
                    self.reconcile(&key);
                    self.emit(RecorderEvent::DeviceChanged(self.status_of(&key)));
                }
                let _ = reply.send(result);
            }
            Cmd::Start(reply) => {
                let _ = reply.send(self.start_session());
            }
            Cmd::Stop(reply) => {
                let _ = reply.send(self.stop_session(internal_rx));
            }
            Cmd::Pause(reply) => {
                let _ = reply.send(self.pause_session());
            }
            Cmd::Resume(reply) => {
                let _ = reply.send(self.resume_session());
            }
            Cmd::Status(reply) => {
                let _ = reply.send(self.active.as_ref().map(|a| SessionStatus {
                    dir: a.dir.clone(),
                    elapsed: a.recorded_elapsed(),
                    paused: a.paused_since.is_some(),
                    bytes: crate::library::dir_bytes(&a.dir),
                }));
            }
            Cmd::Shutdown => {}
        }
    }

    fn on_pw(&mut self, ev: PwEvent) {
        match ev {
            PwEvent::DeviceAdded(DeviceInfo { key, description, pw_id }) => {
                let _ = self.store.mark_seen(&key, &description);
                self.devices.insert(key.clone(), Dev { description, pw_id: Some(pw_id) });
                self.reconcile(&key);
                self.emit(RecorderEvent::DeviceChanged(self.status_of(&key)));
            }
            PwEvent::DeviceRemoved(key) => {
                if let Some(dev) = self.devices.get_mut(&key) {
                    dev.pw_id = None;
                }
                self.emit(RecorderEvent::DeviceChanged(self.status_of(&key)));
            }
            PwEvent::CaptureEnded { id, reason } => {
                let _ = self.pw.stop_capture(id);
                let Some(active) = self.active.as_mut() else { return };
                let Some((key, _)) = active.owners.get(&id).cloned() else { return };
                if active.captures.get(&key) == Some(&id) {
                    active.captures.remove(&key);
                }
                if let EndReason::Error(message) = reason {
                    self.emit(RecorderEvent::CaptureError { key: key.clone(), message });
                }
                self.emit(RecorderEvent::DeviceChanged(self.status_of(&key)));
            }
            PwEvent::Synced => {}
            PwEvent::Stopped => {}
        }
    }

    fn on_internal(&mut self, msg: Internal) {
        let Some(active) = self.active.as_mut() else { return };
        match msg {
            Internal::Opened { id, start_offset_ns, rate, channels } => {
                let Some((key, file)) = active.owners.get(&id).cloned() else { return };
                let (Some(rate_nz), Some(channels_nz)) = (NonZeroU64::new(rate as u64), NonZeroU64::new(channels as u64))
                else {
                    return;
                };
                active.manifest.segments.push(Segment {
                    channels: channels_nz,
                    device: key.to_string(),
                    end_offset_ns: None,
                    file: file.clone(),
                    frames: 0,
                    sample_rate: rate_nz,
                    start_offset_ns,
                    status: SegmentStatus::Recording,
                });
                active.segments.insert(id, active.manifest.segments.len() - 1);
                let _ = manifest::save(&active.dir, &active.manifest);
                self.emit(RecorderEvent::SegmentOpened { key, file });
            }
            Internal::Level { id, peak } => {
                if let Some((key, _)) = active.owners.get(&id).cloned() {
                    self.emit(RecorderEvent::Level { key, peak });
                }
            }
            Internal::Closed { id, frames, end_offset_ns } => {
                active.pending_writers.remove(&id);
                let Some((key, _)) = active.owners.get(&id).cloned() else { return };
                let Some(idx) = active.segments.get(&id).copied() else { return };
                let seg = &mut active.manifest.segments[idx];
                seg.frames = frames;
                seg.end_offset_ns = Some(end_offset_ns);
                seg.status = SegmentStatus::Complete;
                let file = seg.file.clone();
                let _ = manifest::save(&active.dir, &active.manifest);
                self.emit(RecorderEvent::SegmentClosed { key, file });
            }
            Internal::Failed { id, message } => {
                active.pending_writers.remove(&id);
                let _ = self.pw.stop_capture(id);
                if let Some((key, _)) = active.owners.get(&id).cloned() {
                    if active.captures.get(&key) == Some(&id) {
                        active.captures.remove(&key);
                    }
                    self.emit(RecorderEvent::CaptureError { key, message });
                }
            }
        }
    }

    fn reconcile(&mut self, key: &DeviceKey) {
        if self.active.is_none() {
            return;
        }
        let present = self.devices.get(key).is_some_and(|d| d.pw_id.is_some());
        let paused = self.active.as_ref().is_some_and(|a| a.paused_since.is_some());
        let want = present && !paused && self.store.is_enabled(key);
        let have = self.active.as_ref().is_some_and(|a| a.captures.contains_key(key));
        if want && !have {
            self.begin_capture(key);
        } else if !want && have {
            self.end_capture(key);
        }
    }

    fn begin_capture(&mut self, key: &DeviceKey) {
        let id = self.next_id;
        self.next_id += 1;
        let format = self.config.sample_format;
        let description = self.devices.get(key).map(|d| d.description.clone()).unwrap_or_default();
        let internal = self.internal_tx.clone();
        let Some(active) = self.active.as_mut() else { return };

        let n = active.counters.entry(key.clone()).or_insert(0);
        let live_flac = self.config.compression.live;
        let file = format!("tracks/{}-{}.{}", key.sanitized(), n, if live_flac { "flac" } else { "wav" });
        *n += 1;
        active.manifest.devices.entry(key.to_string()).or_insert(ManifestDevice {
            description,
            direction: match key.direction {
                Direction::Input => ManifestDirection::Input,
                Direction::Output => ManifestDirection::Output,
            },
        });

        let (tx, rx) = crossbeam_channel::bounded(256);
        writer::spawn(
            WriterCtx {
                id,
                path: active.dir.join(&file),
                t0: active.t0,
                format,
                internal,
                key: key.clone(),
                file: file.clone(),
                audio: self.audio.clone(),
                warned: Default::default(),
                live_flac,
            },
            rx,
        );
        active.captures.insert(key.clone(), id);
        active.owners.insert(id, (key.clone(), file));
        active.pending_writers.insert(id);
        if self.pw.start_capture(id, key.clone(), tx).is_err() {
            self.emit(RecorderEvent::CaptureError { key: key.clone(), message: "PipeWire thread is gone".into() });
        }
        self.emit(RecorderEvent::DeviceChanged(self.status_of(key)));
    }

    fn end_capture(&mut self, key: &DeviceKey) {
        let Some(active) = self.active.as_mut() else { return };
        if let Some(id) = active.captures.remove(key) {
            let _ = self.pw.stop_capture(id);
        }
        self.emit(RecorderEvent::DeviceChanged(self.status_of(key)));
    }

    fn start_session(&mut self) -> Result<PathBuf, RecorderError> {
        if self.active.is_some() {
            return Err(RecorderError::SessionActive);
        }
        let base = self.config.sessions_dir.clone();
        let name = dir_name_now();
        let mut dir = base.join(&name);
        let mut suffix = 1;
        while dir.exists() {
            dir = base.join(format!("{name}-{suffix}"));
            suffix += 1;
        }
        std::fs::create_dir_all(dir.join("tracks"))
            .map_err(|source| RecorderError::CreateDir { path: dir.clone(), source })?;
        let _ = std::fs::write(dir.join(manifest::PID_FILE), std::process::id().to_string());

        let manifest = manifest::new_manifest(rfc3339_now());
        manifest::save(&dir, &manifest)?;
        self.active = Some(Active {
            paused_since: None,
            paused_total: Duration::ZERO,
            dir: dir.clone(),
            manifest,
            t0: Instant::now(),
            captures: HashMap::new(),
            owners: HashMap::new(),
            segments: HashMap::new(),
            pending_writers: HashSet::new(),
            counters: HashMap::new(),
        });
        tracing::info!("recording session: {}", dir.display());
        self.emit(RecorderEvent::SessionStarted { dir: dir.clone() });
        let keys: Vec<_> = self.devices.keys().cloned().collect();
        for key in keys {
            self.reconcile(&key);
        }
        Ok(dir)
    }

    fn pause_session(&mut self) -> Result<(), RecorderError> {
        let active = self.active.as_mut().ok_or(RecorderError::NoSession)?;
        if active.paused_since.is_some() {
            return Err(RecorderError::AlreadyPaused);
        }
        active.paused_since = Some(Instant::now());
        let dir = active.dir.clone();
        let keys: Vec<_> = active.captures.keys().cloned().collect();
        for key in &keys {
            self.end_capture(key);
        }
        self.emit(RecorderEvent::SessionPaused { dir });
        Ok(())
    }

    fn resume_session(&mut self) -> Result<(), RecorderError> {
        let active = self.active.as_mut().ok_or(RecorderError::NoSession)?;
        let since = active.paused_since.take().ok_or(RecorderError::NotPaused)?;
        active.paused_total += since.elapsed();
        let dir = active.dir.clone();
        self.emit(RecorderEvent::SessionResumed { dir });
        let keys: Vec<_> = self.devices.keys().cloned().collect();
        for key in keys {
            self.reconcile(&key);
        }
        Ok(())
    }

    fn stop_session(&mut self, internal_rx: &Receiver<Internal>) -> Result<PathBuf, RecorderError> {
        let keys: Vec<_> = self.active.as_ref().ok_or(RecorderError::NoSession)?.captures.keys().cloned().collect();
        for key in &keys {
            self.end_capture(key);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.active.as_ref().is_some_and(|a| !a.pending_writers.is_empty()) {
            let left = deadline.saturating_duration_since(Instant::now());
            match internal_rx.recv_timeout(left) {
                Ok(msg) => self.on_internal(msg),
                Err(_) => break,
            }
        }
        let mut active = self.active.take().ok_or(RecorderError::NoSession)?;
        for seg in &mut active.manifest.segments {
            if seg.status == SegmentStatus::Recording {
                seg.status = SegmentStatus::Truncated;
            }
        }
        active.manifest.ended_at = Some(rfc3339_now());
        manifest::save(&active.dir, &active.manifest)?;
        let _ = std::fs::remove_file(active.dir.join(manifest::PID_FILE));
        self.emit(RecorderEvent::SessionStopped { dir: active.dir.clone() });
        for key in keys {
            self.emit(RecorderEvent::DeviceChanged(self.status_of(&key)));
        }
        Ok(active.dir)
    }
}

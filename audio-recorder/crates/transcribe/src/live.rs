use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use audio_recorder_core::{
    config::TranscriptionSettings,
    device::{DeviceKey, Direction},
    session::{LiveAudio, manifest},
    transcript::{self, TRANSCRIPT_VERSION, TranscriptError},
};
use audio_recorder_schema::transcript::{Snippet, Transcript, TranscriptSource, TranscriptSourceDirection};
use crossbeam_channel::{Receiver, Sender, select};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use whisper_rs::WhisperState;

use crate::{
    assemble::{DeviceInput, Word, assemble},
    audio::{DeviceAudio, RATE, SegmentSpan, StreamResampler},
    models::{self, ModelError},
    whisper::{Decoding, Whisper, WhisperFailure},
};

const SAMPLE_RATE: usize = RATE as usize;
const TRAILING_SILENCE: usize = SAMPLE_RATE * 6 / 10;
const MAX_REGION: usize = SAMPLE_RATE * 20;
const CUT_WINDOW: usize = SAMPLE_RATE * 2;
const MIN_PARTIAL: usize = SAMPLE_RATE / 2;
const MIN_VAD: usize = SAMPLE_RATE / 2;
const KEEP_SILENCE: usize = SAMPLE_RATE;
const MAX_BUFFER: usize = SAMPLE_RATE * 300;
const PARTIAL_EVERY: Duration = Duration::from_millis(1500);
const MAX_PENDING: usize = 4096;
const MAX_BACKLOG_BYTES: usize = 256 << 20;
const TICK_EVERY: Duration = Duration::from_millis(400);

#[derive(Debug, thiserror::Error)]
pub enum LiveError {
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Whisper(#[from] WhisperFailure),
    #[error(transparent)]
    Transcript(#[from] TranscriptError),
    #[error("no live session is active")]
    NoSession,
    #[error("a live session is already active")]
    SessionActive,
    #[error("live model is not available: {0}")]
    Unavailable(String),
    #[error("live transcriber is not running")]
    Gone,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiveStatus {
    Downloading { label: String, fraction: Option<f32> },
    Loading,
    Ready,
    Failed(String),
}

#[derive(Debug, Clone)]
pub enum LiveEvent {
    Snippet { snippet: Snippet, description: String },
    Partial { source: String, description: String, start_ns: u64, text: Option<String> },
    Status(LiveStatus),
}

pub(crate) trait Engine {
    fn regions(&mut self, samples: &[f32]) -> Result<Vec<(usize, usize)>, LiveError>;
    fn decode(&mut self, samples: &[f32]) -> Result<Vec<Word>, LiveError>;
}

struct WhisperEngine {
    whisper: Whisper,
    state: WhisperState,
    cancel: Arc<AtomicBool>,
}

impl Engine for WhisperEngine {
    fn regions(&mut self, samples: &[f32]) -> Result<Vec<(usize, usize)>, LiveError> {
        Ok(self.whisper.regions(samples)?)
    }

    fn decode(&mut self, samples: &[f32]) -> Result<Vec<Word>, LiveError> {
        Ok(self.whisper.decode(&mut self.state, samples, Decoding::Greedy, None, &self.cancel)?)
    }
}

#[derive(Debug)]
enum Out {
    Snippet(Snippet),
    Partial { start_ns: u64, text: Option<String> },
}

struct Tracker {
    key: DeviceKey,
    file: String,
    seg_offset_ns: u64,
    resampler: Option<StreamResampler>,
    buf: Vec<f32>,
    origin: usize,
    dirty: bool,
    last_partial: Option<Instant>,
    partial_shown: bool,
}

fn sample_ns(sample: usize) -> u64 {
    sample as u64 * 1_000_000_000 / RATE as u64
}

fn quiet_cut(buf: &[f32], from: usize, to: usize) -> usize {
    const WINDOW: usize = SAMPLE_RATE / 20;
    let mut best = (f32::MAX, to);
    let mut at = from;
    while at + WINDOW <= to {
        let energy: f32 = buf[at..at + WINDOW].iter().map(|s| s * s).sum();
        if energy < best.0 {
            best = (energy, at + WINDOW / 2);
        }
        at += WINDOW / 2;
    }
    best.1
}

impl Tracker {
    fn new(key: DeviceKey, file: String, offset_ns: u64) -> Self {
        Self {
            key,
            file,
            seg_offset_ns: offset_ns,
            resampler: None,
            buf: Vec::new(),
            origin: 0,
            dirty: false,
            last_partial: None,
            partial_shown: false,
        }
    }

    fn append(&mut self, samples: Vec<f32>) {
        self.buf.extend(samples);
        self.dirty = true;
        if self.buf.len() > MAX_BUFFER {
            let drop = self.buf.len() - MAX_BUFFER;
            tracing::warn!("live transcription is behind; dropping {:.1}s of {}", drop as f32 / RATE as f32, self.file);
            self.buf.drain(..drop);
            self.origin += drop;
        }
    }

    fn ingest(&mut self, rate: u32, channels: u32, data: &[f32]) {
        let mono: Vec<f32> = if channels <= 1 {
            data.to_vec()
        } else {
            data.chunks_exact(channels as usize).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
        };
        let resampler = self.resampler.get_or_insert_with(|| StreamResampler::new(rate, RATE));
        let out = resampler.push(&mono);
        self.append(out);
    }

    fn close(&mut self) {
        if let Some(r) = self.resampler.as_mut() {
            let tail = r.finish();
            self.append(tail);
        }
    }

    fn region_ns(&self, start: usize) -> u64 {
        self.seg_offset_ns + sample_ns(self.origin + start)
    }

    fn consume(&mut self, upto: usize) {
        self.buf.drain(..upto);
        self.origin += upto;
    }

    fn commit(&mut self, engine: &mut dyn Engine, from: usize, to: usize) -> Result<Vec<Snippet>, LiveError> {
        let words = match engine.decode(&self.buf[from..to]) {
            Ok(words) => words,
            Err(LiveError::Whisper(WhisperFailure::Cancelled)) => return Err(WhisperFailure::Cancelled.into()),
            Err(e) => {
                tracing::warn!("live decode failed: {e}");
                Vec::new()
            }
        };
        let device = self.key.to_string();
        let audio = DeviceAudio {
            samples: Vec::new(),
            spans: vec![SegmentSpan { file: self.file.clone(), start_sample: 0, len: to - from, offset_ns: self.region_ns(from) }],
        };
        let input = DeviceInput { device: &device, audio: &audio, words, turns: Vec::new() };
        Ok(assemble(&[input]).0)
    }

    fn step(&mut self, engine: &mut dyn Engine, final_: bool, partials: bool, now: Instant) -> Result<Vec<Out>, LiveError> {
        let mut out = Vec::new();
        if !final_ && (!self.dirty || self.buf.len() < MIN_VAD) {
            return Ok(out);
        }
        self.dirty = false;
        while !self.buf.is_empty() {
            let regions = match engine.regions(&self.buf) {
                Ok(r) => r,
                Err(e) => {
                    self.buf.clear();
                    return Err(e);
                }
            };
            let Some(&(a, b)) = regions.first() else {
                let drop = self.buf.len().saturating_sub(KEEP_SILENCE);
                if drop >= KEEP_SILENCE {
                    self.consume(drop);
                }
                break;
            };
            let closed = regions.len() > 1 || final_ || self.buf.len() - b >= TRAILING_SILENCE;
            let end = if b - a > MAX_REGION {
                quiet_cut(&self.buf, a + MAX_REGION - CUT_WINDOW, a + MAX_REGION)
            } else if closed {
                b
            } else {
                if partials && b - a >= MIN_PARTIAL && self.last_partial.is_none_or(|t| now.duration_since(t) >= PARTIAL_EVERY) {
                    self.last_partial = Some(now);
                    let words = engine.decode(&self.buf[a..b]).unwrap_or_default();
                    let text: String = words.iter().filter(|w| !w.text.trim().starts_with('[')).map(|w| w.text.as_str()).collect();
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        self.partial_shown = true;
                        out.push(Out::Partial { start_ns: self.region_ns(a), text: Some(text) });
                    }
                }
                break;
            };
            let snippets = self.commit(engine, a, end)?;
            self.consume(end);
            if self.partial_shown {
                self.partial_shown = false;
                out.push(Out::Partial { start_ns: self.seg_offset_ns, text: None });
            }
            self.last_partial = None;
            out.extend(snippets.into_iter().map(Out::Snippet));
        }
        if final_ {
            self.buf.clear();
        }
        Ok(out)
    }
}

enum Cmd {
    Start { dir: PathBuf, reply: Sender<Result<(), LiveError>> },
    Finish { reply: Sender<Result<Option<Transcript>, LiveError>> },
    Shutdown,
}

struct Backlog {
    rx: Receiver<LiveAudio>,
    bytes: Arc<AtomicUsize>,
}

fn audio_bytes(msg: &LiveAudio) -> usize {
    match msg {
        LiveAudio::Chunk { data, .. } => data.len() * 4,
        LiveAudio::SegmentEnd { .. } => 0,
    }
}

/// Moves audio off the recorder's bounded queue as fast as it arrives, so a busy decoder never makes the recorder drop it.
fn forward(audio: Receiver<LiveAudio>) -> Backlog {
    let (tx, rx) = crossbeam_channel::unbounded();
    let bytes = Arc::new(AtomicUsize::new(0));
    let counter = bytes.clone();
    std::thread::Builder::new()
        .name("live-audio".into())
        .spawn(move || {
            let mut warned = false;
            while let Ok(msg) = audio.recv() {
                let size = audio_bytes(&msg);
                if size > 0 && counter.load(Ordering::Relaxed) + size > MAX_BACKLOG_BYTES {
                    if !std::mem::replace(&mut warned, true) {
                        tracing::warn!("live transcription is far behind; dropping audio");
                    }
                    continue;
                }
                counter.fetch_add(size, Ordering::Relaxed);
                if tx.send(msg).is_err() {
                    break;
                }
            }
        })
        .expect("spawn live audio thread");
    Backlog { rx, bytes }
}

pub struct LiveTranscriber {
    cmd: Sender<Cmd>,
    events: Receiver<LiveEvent>,
    shutdown: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl LiveTranscriber {
    pub fn spawn(settings: TranscriptionSettings, audio: Receiver<LiveAudio>) -> Self {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded();
        let shutdown = Arc::new(AtomicBool::new(false));
        let flag = shutdown.clone();
        let audio = forward(audio);
        let join = std::thread::Builder::new()
            .name("live-transcribe".into())
            .spawn(move || {
                Worker {
                    settings,
                    audio,
                    events: event_tx,
                    engine: None,
                    loader: None,
                    load_failed: None,
                    shutdown: flag,
                    session: None,
                    pending: VecDeque::new(),
                    last_tick: Instant::now(),
                }
                .run(cmd_rx)
            })
            .expect("spawn live transcriber thread");
        Self { cmd: cmd_tx, events: event_rx, shutdown, join: Some(join) }
    }

    pub fn events(&self) -> Receiver<LiveEvent> {
        self.events.clone()
    }

    fn call<T>(&self, make: impl FnOnce(Sender<T>) -> Cmd) -> Result<T, LiveError> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.cmd.send(make(tx)).map_err(|_| LiveError::Gone)?;
        rx.recv().map_err(|_| LiveError::Gone)
    }

    pub fn start_session(&self, dir: PathBuf) -> Result<(), LiveError> {
        self.call(|reply| Cmd::Start { dir, reply })?
    }

    /// Commits whatever audio is still pending and returns the saved live transcript, if it has any text.
    pub fn finish_session(&self) -> Result<Option<Transcript>, LiveError> {
        self.call(|reply| Cmd::Finish { reply })?
    }
}

impl Drop for LiveTranscriber {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = self.cmd.send(Cmd::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct Session {
    dir: PathBuf,
    transcript: Transcript,
    trackers: HashMap<DeviceKey, Tracker>,
    closing: Vec<Tracker>,
    descriptions: HashMap<String, String>,
}

struct Worker {
    settings: TranscriptionSettings,
    audio: Backlog,
    events: Sender<LiveEvent>,
    engine: Option<Box<dyn Engine>>,
    loader: Option<Receiver<Result<(), ModelError>>>,
    load_failed: Option<String>,
    shutdown: Arc<AtomicBool>,
    session: Option<Session>,
    pending: VecDeque<LiveAudio>,
    last_tick: Instant,
}

impl Worker {
    fn run(mut self, cmd_rx: Receiver<Cmd>) {
        loop {
            select! {
                recv(cmd_rx) -> cmd => match cmd {
                    Ok(Cmd::Start { dir, reply }) => {
                        let _ = reply.send(self.start(dir));
                    }
                    Ok(Cmd::Finish { reply }) => {
                        let _ = reply.send(self.finish());
                    }
                    Ok(Cmd::Shutdown) | Err(_) => break,
                },
                recv(self.audio.rx) -> msg => match msg {
                    Ok(msg) => {
                        self.audio.bytes.fetch_sub(audio_bytes(&msg), Ordering::Relaxed);
                        self.on_audio(msg)
                    }
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                },
                default(TICK_EVERY) => {}
            }
            self.poll_loader();
            if self.last_tick.elapsed() >= TICK_EVERY {
                self.last_tick = Instant::now();
                self.tick();
            }
        }
    }

    fn status(&self, status: LiveStatus) {
        let _ = self.events.send(LiveEvent::Status(status));
    }

    fn start(&mut self, dir: PathBuf) -> Result<(), LiveError> {
        if self.session.is_some() {
            return Err(LiveError::SessionActive);
        }
        let language = &self.settings.language;
        let model = self
            .settings
            .live
            .whisper_model
            .path
            .file_name()
            .map_or_else(|| "unknown".to_string(), |n| n.to_string_lossy().into_owned());
        let transcript = Transcript {
            version: TRANSCRIPT_VERSION,
            created_at: OffsetDateTime::now_utc().format(&Rfc3339).unwrap_or_default(),
            model,
            language: (language != "auto").then(|| language.clone()),
            diarized: false,
            sources: HashMap::new(),
            speakers: HashMap::new(),
            snippets: Vec::new(),
        };
        self.session = Some(Session { dir, transcript, trackers: HashMap::new(), closing: Vec::new(), descriptions: HashMap::new() });
        if self.engine.is_some() {
            self.status(LiveStatus::Ready);
        } else if self.loader.is_none() {
            self.begin_load();
        }
        while let Some(msg) = self.pending.pop_front() {
            self.on_audio(msg);
        }
        Ok(())
    }

    fn begin_load(&mut self) {
        self.load_failed = None;
        let files = models::live_required(&self.settings);
        if files.iter().all(|f| f.present()) {
            self.load_engine();
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let events = self.events.clone();
        let cancel = self.shutdown.clone();
        std::thread::spawn(move || {
            let mut last = 0u64;
            let result = models::ensure(
                &files,
                &mut |label, done, total| {
                    if done < last || done - last > 1 << 20 || Some(done) == total {
                        last = done;
                        let _ = events.send(LiveEvent::Status(LiveStatus::Downloading {
                            label: label.to_string(),
                            fraction: total.map(|t| done as f32 / t as f32),
                        }));
                    }
                },
                &|| cancel.load(Ordering::Relaxed),
            );
            let _ = tx.send(result);
        });
        self.loader = Some(rx);
    }

    fn poll_loader(&mut self) {
        let Some(rx) = &self.loader else { return };
        match rx.try_recv() {
            Ok(Ok(())) => {
                self.loader = None;
                self.load_engine();
            }
            Ok(Err(e)) => {
                self.loader = None;
                self.fail_load(e.to_string());
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.loader = None;
                self.fail_load("model download thread died".into());
            }
        }
    }

    fn fail_load(&mut self, message: String) {
        tracing::error!("live transcription unavailable: {message}");
        self.status(LiveStatus::Failed(message.clone()));
        self.load_failed = Some(message);
    }

    fn load_engine(&mut self) {
        self.status(LiveStatus::Loading);
        let s = &self.settings;
        let loaded = Whisper::load(&s.live.whisper_model.path, &s.vad_model.path, &s.language, s.gpu).and_then(|whisper| {
            let state = whisper.new_state()?;
            Ok(WhisperEngine { whisper, state, cancel: self.shutdown.clone() })
        });
        match loaded {
            Ok(engine) => {
                self.engine = Some(Box::new(engine));
                self.status(LiveStatus::Ready);
            }
            Err(e) => self.fail_load(e.to_string()),
        }
    }

    fn on_audio(&mut self, msg: LiveAudio) {
        let Some(session) = self.session.as_mut() else {
            if self.pending.len() >= MAX_PENDING {
                self.pending.pop_front();
            }
            self.pending.push_back(msg);
            return;
        };
        match msg {
            LiveAudio::Chunk { key, file, offset_ns, rate, channels, data } => {
                if session.trackers.get(&key).is_some_and(|t| t.file != file)
                    && let Some(mut old) = session.trackers.remove(&key)
                {
                    old.close();
                    session.closing.push(old);
                }
                session
                    .trackers
                    .entry(key.clone())
                    .or_insert_with(|| Tracker::new(key, file, offset_ns))
                    .ingest(rate, channels, &data);
            }
            LiveAudio::SegmentEnd { key, file } => {
                if session.trackers.get(&key).is_some_and(|t| t.file == file)
                    && let Some(mut old) = session.trackers.remove(&key)
                {
                    old.close();
                    session.closing.push(old);
                }
            }
        }
    }

    fn tick(&mut self) {
        if self.engine.is_none() || self.session.is_none() {
            return;
        }
        if let Err(e) = self.run_steps(false) {
            tracing::warn!("live transcription step failed: {e}");
        }
    }

    fn run_steps(&mut self, all_final: bool) -> Result<(), LiveError> {
        let partials = self.settings.live.partials;
        let (Some(engine), Some(session)) = (self.engine.as_mut(), self.session.as_mut()) else { return Ok(()) };
        let now = Instant::now();
        let mut produced: Vec<(DeviceKey, Out)> = Vec::new();
        let mut first_error = None;

        let mut closing = std::mem::take(&mut session.closing);
        for tracker in closing.iter_mut() {
            match tracker.step(engine.as_mut(), true, partials, now) {
                Ok(outs) => produced.extend(outs.into_iter().map(|o| (tracker.key.clone(), o))),
                Err(e) => first_error = first_error.or(Some(e)),
            }
        }
        for tracker in session.trackers.values_mut() {
            match tracker.step(engine.as_mut(), all_final, partials, now) {
                Ok(outs) => produced.extend(outs.into_iter().map(|o| (tracker.key.clone(), o))),
                Err(e) => first_error = first_error.or(Some(e)),
            }
        }
        if all_final {
            session.trackers.clear();
        }

        let mut changed = false;
        for (key, out) in produced {
            match out {
                Out::Snippet(snippet) => {
                    let description = Self::describe(session, &key);
                    session.transcript.sources.entry(key.to_string()).or_insert_with(|| TranscriptSource {
                        description: description.clone(),
                        direction: match key.direction {
                            Direction::Input => TranscriptSourceDirection::Input,
                            Direction::Output => TranscriptSourceDirection::Output,
                        },
                    });
                    let at = session.transcript.snippets.partition_point(|s| s.start_ns <= snippet.start_ns);
                    session.transcript.snippets.insert(at, snippet.clone());
                    changed = true;
                    let _ = self.events.send(LiveEvent::Snippet { snippet, description });
                }
                Out::Partial { start_ns, text } => {
                    let description = Self::describe(session, &key);
                    let _ = self.events.send(LiveEvent::Partial { source: key.to_string(), description, start_ns, text });
                }
            }
        }
        if changed {
            transcript::save_live(&session.dir, &session.transcript)?;
        }
        first_error.map_or(Ok(()), Err)
    }

    fn describe(session: &mut Session, key: &DeviceKey) -> String {
        let id = key.to_string();
        if let Some(d) = session.descriptions.get(&id) {
            return d.clone();
        }
        match manifest::load_recovering(&session.dir).ok().and_then(|m| m.devices.get(&id).map(|d| d.description.clone())) {
            Some(d) => {
                session.descriptions.insert(id, d.clone());
                d
            }
            None => key.node_name.clone(),
        }
    }

    fn finish(&mut self) -> Result<Option<Transcript>, LiveError> {
        if self.session.is_none() {
            return Err(LiveError::NoSession);
        }
        while let Ok(msg) = self.audio.rx.try_recv() {
            self.audio.bytes.fetch_sub(audio_bytes(&msg), Ordering::Relaxed);
            self.on_audio(msg);
        }
        if let Some(session) = self.session.as_mut() {
            for (_, mut t) in session.trackers.drain() {
                t.close();
                session.closing.push(t);
            }
        }
        while self.engine.is_none() && self.load_failed.is_none() && !self.shutdown.load(Ordering::Relaxed) {
            self.poll_loader();
            std::thread::sleep(Duration::from_millis(50));
        }
        let result = if self.engine.is_some() {
            self.run_steps(true)
        } else {
            Err(LiveError::Unavailable(self.load_failed.clone().unwrap_or_else(|| "shutting down".into())))
        };
        let session = self.session.take().ok_or(LiveError::NoSession)?;
        result?;
        Ok((!session.transcript.snippets.is_empty()).then_some(session.transcript))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub;

    impl Engine for Stub {
        fn regions(&mut self, samples: &[f32]) -> Result<Vec<(usize, usize)>, LiveError> {
            const BLOCK: usize = 160;
            let mut regions: Vec<(usize, usize)> = Vec::new();
            for (i, block) in samples.chunks(BLOCK).enumerate() {
                let rms = (block.iter().map(|s| s * s).sum::<f32>() / block.len() as f32).sqrt();
                if rms > 0.05 {
                    let (start, end) = (i * BLOCK, i * BLOCK + block.len());
                    match regions.last_mut() {
                        Some(last) if start - last.1 < SAMPLE_RATE => last.1 = end,
                        _ => regions.push((start, end)),
                    }
                }
            }
            Ok(regions)
        }

        fn decode(&mut self, samples: &[f32]) -> Result<Vec<Word>, LiveError> {
            Ok(vec![Word { start_s: 0.0, end_s: samples.len() as f64 / RATE as f64, text: " hi".into() }])
        }
    }

    fn tone(secs: f32) -> Vec<f32> {
        (0..(secs * RATE as f32) as usize).map(|i| (i as f32 * 0.3).sin() * 0.5).collect()
    }

    fn silence(secs: f32) -> Vec<f32> {
        vec![0.0; (secs * RATE as f32) as usize]
    }

    fn tracker(offset_ns: u64) -> Tracker {
        Tracker::new(DeviceKey::new(Direction::Input, "mic"), "tracks/mic-0.wav".into(), offset_ns)
    }

    fn feed(t: &mut Tracker, samples: &[f32]) {
        for chunk in samples.chunks(320) {
            t.ingest(RATE, 1, chunk);
        }
    }

    fn snippets(outs: &[Out]) -> Vec<&Snippet> {
        outs.iter().filter_map(|o| if let Out::Snippet(s) = o { Some(s) } else { None }).collect()
    }

    #[test]
    fn commits_after_trailing_silence_with_session_time() {
        let mut t = tracker(5_000_000_000);
        feed(&mut t, &silence(0.5));
        feed(&mut t, &tone(1.0));
        feed(&mut t, &silence(1.0));
        let outs = t.step(&mut Stub, false, false, Instant::now()).unwrap();
        let s = snippets(&outs);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].start_ns, 5_500_000_000);
        assert_eq!(s[0].source, "input:mic");
        assert_eq!(s[0].segment, "tracks/mic-0.wav");
        assert_eq!(s[0].text, "hi");
    }

    #[test]
    fn open_speech_waits_then_emits_partial_and_commits_on_final() {
        let mut t = tracker(0);
        feed(&mut t, &tone(2.0));
        let outs = t.step(&mut Stub, false, true, Instant::now()).unwrap();
        assert!(snippets(&outs).is_empty());
        assert!(matches!(outs.as_slice(), [Out::Partial { start_ns: 0, text: Some(t) }] if t == "hi"), "{outs:?}");

        let outs = t.step(&mut Stub, true, true, Instant::now()).unwrap();
        assert_eq!(snippets(&outs).len(), 1);
        assert!(outs.iter().any(|o| matches!(o, Out::Partial { text: None, .. })));
    }

    #[test]
    fn long_speech_is_cut_before_it_ends() {
        let mut t = tracker(0);
        feed(&mut t, &tone(45.0));
        let outs = t.step(&mut Stub, false, false, Instant::now()).unwrap();
        let s = snippets(&outs);
        assert!(s.len() >= 2, "{}", s.len());
        assert!(s.windows(2).all(|w| w[0].start_ns < w[1].start_ns));
    }

    #[test]
    fn leading_silence_is_trimmed_and_time_stays_aligned() {
        let mut t = tracker(0);
        feed(&mut t, &silence(10.0));
        t.step(&mut Stub, false, false, Instant::now()).unwrap();
        assert!(t.buf.len() <= KEEP_SILENCE + SAMPLE_RATE);
        feed(&mut t, &tone(1.0));
        feed(&mut t, &silence(1.0));
        let outs = t.step(&mut Stub, false, false, Instant::now()).unwrap();
        let start = snippets(&outs)[0].start_ns;
        assert!((10_000_000_000..10_100_000_000).contains(&start), "{start}");
    }

    #[test]
    fn resamples_and_downmixes_chunks() {
        let mut t = tracker(0);
        let stereo: Vec<f32> = (0..96_000).flat_map(|i| [(i as f32 * 0.05).sin() * 0.5, 0.0]).collect();
        for chunk in stereo.chunks(1920) {
            t.ingest(48_000, 2, chunk);
        }
        assert!((t.buf.len() as i64 - 32_000).abs() < 400, "{}", t.buf.len());
    }
}

use std::{
    num::NonZero,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use audio_recorder_schema::mix::MixSettings;
use crossbeam_channel::{Receiver, Sender};
use rodio::{ChannelCount, DeviceSinkBuilder, Player, SampleRate, Source};

use crate::{
    device::DeviceKey,
    balance::{self, BalanceError},
    mix::{self, MixError},
    mixdown::{MixSource, MixdownError, OUT_CHANNELS, OUT_RATE},
};

const BLOCK_FRAMES: usize = 2048;
const NO_SEEK: u64 = u64::MAX;

#[derive(Debug, thiserror::Error)]
pub enum PlaybackError {
    #[error(transparent)]
    Mixdown(#[from] MixdownError),
    #[error(transparent)]
    Mix(#[from] MixError),
    #[error(transparent)]
    Balance(#[from] BalanceError),
    #[error("nothing to play in this session")]
    Empty,
    #[error("could not open audio output: {0}")]
    Device(String),
    #[error("playback thread is not running")]
    Gone,
}

#[derive(Debug, Clone, Default)]
pub struct PlaybackState {
    pub session: Option<PathBuf>,
    pub track: Option<DeviceKey>,
    pub position: Duration,
    pub duration: Duration,
    pub paused: bool,
    pub playing: bool,
}

struct Shared {
    seek_frame: AtomicU64,
    position_frames: AtomicU64,
    finished: AtomicBool,
    pending_mix: Mutex<Option<MixSettings>>,
}

struct MixPlayback {
    shared: Arc<Shared>,
    mix: MixSource,
    pos: u64,
    buf: Vec<f32>,
    idx: usize,
}

impl Iterator for MixPlayback {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.idx >= self.buf.len() {
            let seek = self.shared.seek_frame.swap(NO_SEEK, Ordering::AcqRel);
            if seek != NO_SEEK {
                self.pos = seek.min(self.mix.total_frames());
            }
            if let Ok(mut pending) = self.shared.pending_mix.try_lock() {
                if let Some(settings) = pending.take() {
                    self.mix.set_mix(&settings);
                }
            }
            let total = self.mix.total_frames();
            if self.pos >= total {
                self.shared.position_frames.store(total, Ordering::Release);
                self.shared.finished.store(true, Ordering::Release);
                return None;
            }
            let n = (total - self.pos).min(BLOCK_FRAMES as u64) as usize;
            self.buf.resize(n * OUT_CHANNELS, 0.0);
            if self.mix.fill(self.pos, &mut self.buf).is_err() {
                self.shared.finished.store(true, Ordering::Release);
                return None;
            }
            self.pos += n as u64;
            self.shared.position_frames.store(self.pos, Ordering::Release);
            self.idx = 0;
        }
        let s = self.buf[self.idx];
        self.idx += 1;
        Some(s)
    }
}

impl Source for MixPlayback {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        NonZero::new(OUT_CHANNELS as u16).expect("nonzero")
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(OUT_RATE).expect("nonzero")
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

enum Cmd {
    Play { dir: PathBuf, track: Option<DeviceKey>, reply: Sender<Result<(), PlaybackError>> },
    SetMix { dir: PathBuf, mix: MixSettings },
    Pause,
    Resume,
    Seek(Duration),
    Stop,
    Shutdown,
}

pub struct Playback {
    cmd: Sender<Cmd>,
    state: Arc<Mutex<PlaybackState>>,
    join: Option<JoinHandle<()>>,
}

struct Current {
    player: Player,
    shared: Arc<Shared>,
    total_frames: u64,
}

impl Playback {
    pub fn start() -> Self {
        let (cmd, rx) = crossbeam_channel::unbounded();
        let state = Arc::new(Mutex::new(PlaybackState::default()));
        let thread_state = state.clone();
        let join = std::thread::Builder::new()
            .name("playback".into())
            .spawn(move || run(rx, thread_state))
            .expect("spawn playback thread");
        Self { cmd, state, join: Some(join) }
    }

    pub fn play(&self, dir: PathBuf, track: Option<DeviceKey>) -> Result<(), PlaybackError> {
        tracing::info!("playing session: {}", dir.display());
        let (reply, rx) = crossbeam_channel::bounded(1);
        self.cmd.send(Cmd::Play { dir, track, reply }).map_err(|_| PlaybackError::Gone)?;
        rx.recv().map_err(|_| PlaybackError::Gone)?
    }

    pub fn set_mix(&self, dir: PathBuf, mix: MixSettings) {
        let _ = self.cmd.send(Cmd::SetMix { dir, mix });
    }

    pub fn pause(&self) {
        let _ = self.cmd.send(Cmd::Pause);
    }

    pub fn resume(&self) {
        let _ = self.cmd.send(Cmd::Resume);
    }

    pub fn seek(&self, to: Duration) {
        let _ = self.cmd.send(Cmd::Seek(to));
    }

    pub fn stop(&self) {
        let _ = self.cmd.send(Cmd::Stop);
    }

    pub fn state(&self) -> PlaybackState {
        self.state.lock().expect("playback state lock").clone()
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        let _ = self.cmd.send(Cmd::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn frames_to_duration(frames: u64) -> Duration {
    Duration::from_nanos((frames as u128 * 1_000_000_000 / OUT_RATE as u128) as u64)
}

fn run(rx: Receiver<Cmd>, state: Arc<Mutex<PlaybackState>>) {
    let mut sink: Option<rodio::MixerDeviceSink> = None;
    let mut current: Option<Current> = None;

    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Cmd::Shutdown) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Ok(Cmd::Play { dir, track, reply }) => {
                current = None;
                let result = (|| {
                    balance::ensure_default(&dir)?;
                    let settings = mix::load(&dir)?;
                    let mix = MixSource::open(&dir, track.as_ref(), &settings)?;
                    if mix.is_empty() || mix.total_frames() == 0 {
                        return Err(PlaybackError::Empty);
                    }
                    if sink.is_none() {
                        let mut s = DeviceSinkBuilder::open_default_sink().map_err(|e| PlaybackError::Device(e.to_string()))?;
                        s.log_on_drop(false);
                        sink = Some(s);
                    }
                    let total_frames = mix.total_frames();
                    let shared = Arc::new(Shared {
                        seek_frame: AtomicU64::new(NO_SEEK),
                        position_frames: AtomicU64::new(0),
                        finished: AtomicBool::new(false),
                        pending_mix: Mutex::new(None),
                    });
                    let player = Player::connect_new(sink.as_ref().expect("sink opened").mixer());
                    player.append(MixPlayback { shared: shared.clone(), mix, pos: 0, buf: Vec::new(), idx: 0 });
                    let mut st = state.lock().expect("playback state lock");
                    *st = PlaybackState {
                        session: Some(dir),
                        track,
                        position: Duration::ZERO,
                        duration: frames_to_duration(total_frames),
                        paused: false,
                        playing: true,
                    };
                    current = Some(Current { player, shared, total_frames });
                    Ok(())
                })();
                let _ = reply.send(result);
            }
            Ok(Cmd::SetMix { dir, mix }) => {
                let same = state.lock().expect("playback state lock").session.as_deref() == Some(dir.as_path());
                if let (true, Some(c)) = (same, &current) {
                    *c.shared.pending_mix.lock().expect("pending mix lock") = Some(mix);
                }
            }
            Ok(Cmd::Pause) => {
                if let Some(c) = &current {
                    c.player.pause();
                    state.lock().expect("playback state lock").paused = true;
                }
            }
            Ok(Cmd::Resume) => {
                if let Some(c) = &current {
                    c.player.play();
                    state.lock().expect("playback state lock").paused = false;
                }
            }
            Ok(Cmd::Seek(to)) => {
                if let Some(c) = &current {
                    let frame = (to.as_nanos() * OUT_RATE as u128 / 1_000_000_000) as u64;
                    c.shared.seek_frame.store(frame.min(c.total_frames), Ordering::Release);
                    c.shared.position_frames.store(frame.min(c.total_frames), Ordering::Release);
                    c.shared.finished.store(false, Ordering::Release);
                }
            }
            Ok(Cmd::Stop) => {
                current = None;
                let mut st = state.lock().expect("playback state lock");
                st.playing = false;
                st.paused = false;
                st.position = Duration::ZERO;
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }

        if let Some(c) = &current {
            let mut st = state.lock().expect("playback state lock");
            st.position = frames_to_duration(c.shared.position_frames.load(Ordering::Acquire));
            if c.shared.finished.load(Ordering::Acquire) && c.player.empty() {
                st.playing = false;
            }
        }
    }
}

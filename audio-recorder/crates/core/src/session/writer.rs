use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use audio_recorder_schema::config::SampleFormat;
use crossbeam_channel::{Receiver, Sender};

use super::LiveAudio;
use crate::{
    device::DeviceKey,
    flac_writer::{FLAC_FLOAT_BITS, FlacWriter, f32_to_16, f32_to_24},
    pw::CaptureMsg,
};

pub(crate) type AudioSubscribers = Arc<Mutex<Vec<Sender<LiveAudio>>>>;

const GAP_THRESHOLD: Duration = Duration::from_millis(250);
const FLUSH_EVERY_FRAMES_SECS: u64 = 1;
const LEVEL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub(crate) enum Internal {
    Opened { id: u64, start_offset_ns: u64, rate: u32, channels: u32 },
    Level { id: u64, peak: f32 },
    Closed { id: u64, frames: u64, end_offset_ns: u64 },
    Failed { id: u64, message: String },
}

pub(crate) struct WriterCtx {
    pub id: u64,
    pub path: PathBuf,
    pub t0: Instant,
    pub format: SampleFormat,
    pub internal: Sender<Internal>,
    pub key: DeviceKey,
    pub file: String,
    pub audio: AudioSubscribers,
    pub warned: AtomicBool,
    pub live_flac: bool,
}

impl WriterCtx {
    fn tap(&self, make: impl FnOnce() -> LiveAudio) {
        let mut subs = self.audio.lock().expect("audio subscribers lock");
        if subs.is_empty() {
            return;
        }
        let msg = make();
        subs.retain(|s| match s.try_send(msg.clone()) {
            Ok(()) => true,
            Err(crossbeam_channel::TrySendError::Full(_)) => {
                if !self.warned.swap(true, Ordering::Relaxed) {
                    tracing::warn!("live audio subscriber is behind; dropping audio of {}", self.file);
                }
                true
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => false,
        });
    }

    fn segment_end(&self) {
        self.tap(|| LiveAudio::SegmentEnd { key: self.key.clone(), file: self.file.clone() });
    }
}

enum Output {
    F32(hound::WavWriter<std::io::BufWriter<std::fs::File>>),
    I16(hound::WavWriter<std::io::BufWriter<std::fs::File>>),
    Flac { writer: FlacWriter, int16: bool, scratch: Vec<i32> },
}

impl Output {
    fn create(ctx: &WriterCtx, rate: u32, channels: u32) -> Result<Self, String> {
        let int16 = ctx.format == SampleFormat::I16;
        if ctx.live_flac {
            let bits = if int16 { 16 } else { FLAC_FLOAT_BITS };
            let writer = FlacWriter::create(&ctx.path, rate, channels as usize, bits).map_err(|e| e.to_string())?;
            return Ok(Output::Flac { writer, int16, scratch: Vec::new() });
        }
        let spec = hound::WavSpec {
            channels: channels as u16,
            sample_rate: rate,
            bits_per_sample: if int16 { 16 } else { 32 },
            sample_format: if int16 { hound::SampleFormat::Int } else { hound::SampleFormat::Float },
        };
        let writer = hound::WavWriter::create(&ctx.path, spec).map_err(|e| e.to_string())?;
        Ok(if int16 { Output::I16(writer) } else { Output::F32(writer) })
    }

    fn write(&mut self, data: &[f32]) -> Result<(), String> {
        match self {
            Output::F32(w) => data.iter().try_for_each(|&s| w.write_sample(s)).map_err(|e| e.to_string()),
            Output::I16(w) => data
                .iter()
                .try_for_each(|&s| w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16))
                .map_err(|e| e.to_string()),
            Output::Flac { writer, int16, scratch } => {
                scratch.clear();
                scratch.extend(data.iter().map(|&s| if *int16 { f32_to_16(s) } else { f32_to_24(s) }));
                writer.write(scratch).map_err(|e| e.to_string())
            }
        }
    }

    fn flush(&mut self) -> Result<(), String> {
        match self {
            Output::F32(w) | Output::I16(w) => w.flush().map_err(|e| e.to_string()),
            Output::Flac { writer, .. } => writer.flush().map_err(|e| e.to_string()),
        }
    }

    fn finalize(self) -> Result<(), String> {
        match self {
            Output::F32(w) | Output::I16(w) => w.finalize().map_err(|e| e.to_string()),
            Output::Flac { writer, .. } => writer.finalize().map_err(|e| e.to_string()),
        }
    }
}

struct Open {
    out: Output,
    start: Instant,
    rate: u32,
    channels: u32,
    frames: u64,
    flushed_at: u64,
}

pub(crate) fn spawn(ctx: WriterCtx, rx: Receiver<CaptureMsg>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name(format!("writer-{}", ctx.id))
        .spawn(move || run(ctx, rx))
        .expect("spawn writer thread")
}

fn frame_offset_ns(ctx: &WriterCtx, o: &Open) -> u64 {
    o.start.saturating_duration_since(ctx.t0).as_nanos() as u64 + o.frames * 1_000_000_000 / o.rate as u64
}

fn run(ctx: WriterCtx, rx: Receiver<CaptureMsg>) {
    let mut format: Option<(u32, u32)> = None;
    let mut open: Option<Open> = None;
    let mut peak = 0f32;
    let mut last_level = Instant::now();

    let fail = |message: String| {
        ctx.segment_end();
        let _ = ctx.internal.send(Internal::Failed { id: ctx.id, message });
    };

    while let Ok(msg) = rx.recv() {
        match msg {
            CaptureMsg::Format { rate, channels } => format = Some((rate, channels)),
            CaptureMsg::Samples { arrived, data } => {
                let Some((rate, channels)) = format else { continue };
                let frames_in = (data.len() / channels as usize) as u64;
                let chunk_dur = Duration::from_nanos(frames_in * 1_000_000_000 / rate as u64);
                let chunk_start = arrived.checked_sub(chunk_dur).unwrap_or(arrived);

                if open.is_none() {
                    if let Some(parent) = ctx.path.parent()
                        && let Err(e) = std::fs::create_dir_all(parent)
                    {
                        fail(e.to_string());
                        return;
                    }
                    let out = match Output::create(&ctx, rate, channels) {
                        Ok(o) => o,
                        Err(e) => {
                            fail(e);
                            return;
                        }
                    };
                    let start_offset_ns = chunk_start.saturating_duration_since(ctx.t0).as_nanos() as u64;
                    let _ = ctx.internal.send(Internal::Opened { id: ctx.id, start_offset_ns, rate, channels });
                    open = Some(Open { out, start: chunk_start, rate, channels, frames: 0, flushed_at: 0 });
                }
                let Some(o) = open.as_mut() else { continue };

                let expected = o.start + Duration::from_nanos(o.frames * 1_000_000_000 / o.rate as u64);
                let gap = chunk_start.saturating_duration_since(expected);
                if gap > GAP_THRESHOLD {
                    let pad_frames = gap.as_nanos() as u64 * o.rate as u64 / 1_000_000_000;
                    let silence = vec![0.0f32; (pad_frames * o.channels as u64) as usize];
                    if let Err(e) = o.out.write(&silence) {
                        fail(e);
                        return;
                    }
                    ctx.tap(|| LiveAudio::Chunk {
                        key: ctx.key.clone(),
                        file: ctx.file.clone(),
                        offset_ns: frame_offset_ns(&ctx, o),
                        rate: o.rate,
                        channels: o.channels,
                        data: silence.as_slice().into(),
                    });
                    o.frames += pad_frames;
                }
                ctx.tap(|| LiveAudio::Chunk {
                    key: ctx.key.clone(),
                    file: ctx.file.clone(),
                    offset_ns: frame_offset_ns(&ctx, o),
                    rate: o.rate,
                    channels: o.channels,
                    data: data.as_slice().into(),
                });

                peak = data.iter().fold(peak, |p, s| p.max(s.abs()));
                if let Err(e) = o.out.write(&data) {
                    fail(e);
                    return;
                }
                o.frames += frames_in;

                if o.frames - o.flushed_at >= o.rate as u64 * FLUSH_EVERY_FRAMES_SECS {
                    o.flushed_at = o.frames;
                    if let Err(e) = o.out.flush() {
                        fail(e.to_string());
                        return;
                    }
                }
                if last_level.elapsed() >= LEVEL_INTERVAL {
                    last_level = Instant::now();
                    let _ = ctx.internal.send(Internal::Level { id: ctx.id, peak });
                    peak = 0.0;
                }
            }
        }
    }

    if open.is_some() {
        ctx.segment_end();
    }
    if let Some(o) = open {
        let start_ns = o.start.saturating_duration_since(ctx.t0).as_nanos() as u64;
        let end_offset_ns = start_ns + o.frames * 1_000_000_000 / o.rate as u64;
        let frames = o.frames;
        match o.out.finalize() {
            Ok(()) => {
                let _ = ctx.internal.send(Internal::Closed { id: ctx.id, frames, end_offset_ns });
            }
            Err(e) => fail(e),
        }
    } else {
        let _ = ctx.internal.send(Internal::Closed { id: ctx.id, frames: 0, end_offset_ns: 0 });
    }
}

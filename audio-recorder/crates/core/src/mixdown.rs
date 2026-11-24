use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

use audio_recorder_schema::{manifest::Segment, mix::MixSettings};

use crate::{
    audio_file::{self, AudioFileError, SegmentReader},
    device::DeviceKey,
    mix::{self, MixError},
    session::manifest::{self, ManifestError},
};

pub const OUT_RATE: u32 = 48_000;
pub const OUT_CHANNELS: usize = 2;
const BLOCK_FRAMES: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum MixdownError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Mix(#[from] MixError),
    #[error(transparent)]
    Audio(#[from] AudioFileError),
    #[error("failed to write {}: {source}", path.display())]
    Write { path: PathBuf, source: hound::Error },
}

struct SegReader {
    reader: SegmentReader,
    src_frames: u64,
    step: f64,
    prev: [f32; 2],
    next: [f32; 2],
    src_pos: u64,
    frac: f64,
    next_out: Option<u64>,
}

impl SegReader {
    fn open(path: PathBuf) -> Result<Self, MixdownError> {
        let reader = SegmentReader::open(&path)?;
        Ok(Self {
            src_frames: reader.frames(),
            step: reader.rate() as f64 / OUT_RATE as f64,
            reader,
            prev: [0.0; 2],
            next: [0.0; 2],
            src_pos: 0,
            frac: 0.0,
            next_out: None,
        })
    }

    fn read_frame(&mut self) -> Result<[f32; 2], MixdownError> {
        let Some(frame) = self.reader.next_frame()? else { return Ok([0.0; 2]) };
        Ok(match frame.len() {
            1 => [frame[0], frame[0]],
            2 => [frame[0], frame[1]],
            n => {
                let avg = frame.iter().sum::<f32>() / n as f32;
                [avg, avg]
            }
        })
    }

    fn seek_to_out(&mut self, out_frame: u64) -> Result<(), MixdownError> {
        let exact = out_frame as f64 * self.step;
        let src = (exact.floor() as u64).min(self.src_frames);
        self.reader.seek(src)?;
        self.src_pos = src;
        self.frac = exact - src as f64;
        self.prev = if src < self.src_frames { self.read_frame()? } else { [0.0; 2] };
        self.next = if src + 1 < self.src_frames { self.read_frame()? } else { [0.0; 2] };
        self.next_out = Some(out_frame);
        Ok(())
    }

    fn add_into(&mut self, local_start: u64, gain: f32, out: &mut [f32]) -> Result<(), MixdownError> {
        if self.next_out != Some(local_start) {
            self.seek_to_out(local_start)?;
        }
        let frames = out.len() / OUT_CHANNELS;
        for i in 0..frames {
            if self.src_pos >= self.src_frames {
                break;
            }
            let f = self.frac as f32;
            out[i * 2] += (self.prev[0] + (self.next[0] - self.prev[0]) * f) * gain;
            out[i * 2 + 1] += (self.prev[1] + (self.next[1] - self.prev[1]) * f) * gain;
            self.frac += self.step;
            while self.frac >= 1.0 {
                self.frac -= 1.0;
                self.prev = self.next;
                self.src_pos += 1;
                self.next = if self.src_pos + 1 < self.src_frames { self.read_frame()? } else { [0.0; 2] };
            }
        }
        self.next_out = Some(local_start + frames as u64);
        Ok(())
    }
}

pub fn read_segment_mono(path: &Path) -> Result<(Vec<f32>, u32), MixdownError> {
    Ok(audio_file::read_all_mono(path)?)
}

const LEVEL_TARGET_RMS: f32 = 0.1;
const LEVEL_GATE_RMS: f32 = 0.003_162;
const LEVEL_MAX_GAIN: f32 = 2.0;
const LEVEL_MIN_GAIN: f32 = 0.251;
const LEVEL_BLOCK_FRAMES: usize = 2400;
const LEVEL_FLOOR_BLOCKS: usize = 100;
const LEVEL_FLOOR_MARGIN_POWER: f32 = 10.0;
const LIMIT_CEILING: f32 = 0.98;

fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= LIMIT_CEILING {
        return x;
    }
    let knee = 1.0 - LIMIT_CEILING;
    x.signum() * (LIMIT_CEILING + knee * ((a - LIMIT_CEILING) / knee).tanh())
}

fn time_coef(seconds: f32) -> f32 {
    1.0 - (-1.0 / (seconds * OUT_RATE as f32)).exp()
}

struct Leveler {
    gain: f32,
    env: f32,
    primed: bool,
    block_power: f32,
    block_len: usize,
    floor_ring: VecDeque<f32>,
    floor: f32,
    env_coef: f32,
    attack_coef: f32,
    release_coef: f32,
}

impl Leveler {
    fn new() -> Self {
        Self {
            gain: 1.0,
            env: 0.0,
            primed: false,
            block_power: 0.0,
            block_len: 0,
            floor_ring: VecDeque::new(),
            floor: 0.0,
            env_coef: time_coef(0.05),
            attack_coef: time_coef(0.02),
            release_coef: time_coef(0.8),
        }
    }

    fn reset(&mut self) {
        self.primed = false;
        self.block_power = 0.0;
        self.block_len = 0;
        self.floor_ring.clear();
    }

    fn push_block(&mut self, mean_power: f32) {
        if self.floor_ring.len() == LEVEL_FLOOR_BLOCKS {
            self.floor_ring.pop_front();
        }
        self.floor_ring.push_back(mean_power);
        self.floor = self.floor_ring.iter().copied().fold(f32::MAX, f32::min);
    }

    fn desired(&self) -> Option<f32> {
        let rms = self.env.sqrt();
        let above_floor = self.env >= self.floor * LEVEL_FLOOR_MARGIN_POWER;
        (rms >= LEVEL_GATE_RMS && above_floor).then(|| (LEVEL_TARGET_RMS / rms).clamp(LEVEL_MIN_GAIN, LEVEL_MAX_GAIN))
    }

    fn process(&mut self, buf: &mut [f32]) {
        if !self.primed {
            let head = &buf[..buf.len().min(LEVEL_BLOCK_FRAMES * OUT_CHANNELS)];
            let power = head.iter().map(|x| x * x).sum::<f32>() / head.len().max(1) as f32;
            self.env = power;
            self.push_block(power);
            self.gain = 1.0;
            self.primed = true;
        }
        for frame in buf.chunks_exact_mut(OUT_CHANNELS) {
            let power = (frame[0] * frame[0] + frame[1] * frame[1]) * 0.5;
            self.env += self.env_coef * (power - self.env);
            self.block_power += power;
            self.block_len += 1;
            if self.block_len == LEVEL_BLOCK_FRAMES {
                self.push_block(self.block_power / LEVEL_BLOCK_FRAMES as f32);
                self.block_power = 0.0;
                self.block_len = 0;
            }
            if let Some(want) = self.desired() {
                let coef = if want < self.gain { self.attack_coef } else { self.release_coef };
                self.gain += (want - self.gain) * coef;
            }
            frame[0] *= self.gain;
            frame[1] *= self.gain;
        }
    }
}

struct Track {
    device_idx: usize,
    device: String,
    file: String,
    gain: f32,
    path: PathBuf,
    start_frame: u64,
    len_frames: u64,
    reader: Option<SegReader>,
}

pub struct MixSource {
    tracks: Vec<Track>,
    levelers: Vec<Leveler>,
    total_frames: u64,
    single_device: bool,
    leveling: bool,
    scratch: Vec<f32>,
    limiter: f32,
    limiter_attack: f32,
    limiter_release: f32,
    next_start: Option<u64>,
}

fn ns_to_frames(ns: u64) -> u64 {
    (ns as u128 * OUT_RATE as u128 / 1_000_000_000) as u64
}

fn segment_len_frames(seg: &Segment) -> u64 {
    match seg.end_offset_ns {
        Some(end) => ns_to_frames(end.saturating_sub(seg.start_offset_ns)),
        None => seg.frames * OUT_RATE as u64 / seg.sample_rate.get(),
    }
}

impl MixSource {
    pub fn open(session_dir: &Path, only: Option<&DeviceKey>, mix: &MixSettings) -> Result<Self, MixdownError> {
        let manifest = manifest::load_recovering(session_dir)?;
        let want = only.map(|k| k.to_string());
        let mut tracks = Vec::new();
        let mut device_names: Vec<&str> = Vec::new();
        let mut total_frames = 0;
        for seg in &manifest.segments {
            if want.as_ref().is_some_and(|w| *w != seg.device) {
                continue;
            }
            let path = session_dir.join(&seg.file);
            if !path.is_file() {
                continue;
            }
            let start_frame = ns_to_frames(seg.start_offset_ns);
            let len_frames = segment_len_frames(seg);
            total_frames = total_frames.max(start_frame + len_frames);
            let device_idx = match device_names.iter().position(|d| *d == seg.device) {
                Some(i) => i,
                None => {
                    device_names.push(&seg.device);
                    device_names.len() - 1
                }
            };
            let gain = mix::segment_gain(mix, &seg.device, &seg.file, only.is_none());
            tracks.push(Track {
                device_idx,
                device: seg.device.clone(),
                file: seg.file.clone(),
                gain,
                path,
                start_frame,
                len_frames,
                reader: None,
            });
        }
        Ok(Self {
            levelers: device_names.iter().map(|_| Leveler::new()).collect(),
            tracks,
            total_frames,
            single_device: only.is_some(),
            leveling: mix.leveling,
            scratch: Vec::new(),
            limiter: 1.0,
            limiter_attack: time_coef(0.001),
            limiter_release: time_coef(0.1),
            next_start: None,
        })
    }

    pub fn set_mix(&mut self, mix: &MixSettings) {
        self.leveling = mix.leveling;
        for t in &mut self.tracks {
            t.gain = mix::segment_gain(mix, &t.device, &t.file, !self.single_device);
        }
    }

    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    pub fn fill(&mut self, start_frame: u64, out: &mut [f32]) -> Result<(), MixdownError> {
        out.fill(0.0);
        let frames = (out.len() / OUT_CHANNELS) as u64;
        let end = start_frame + frames;
        if self.next_start != Some(start_frame) {
            self.limiter = 1.0;
            self.levelers.iter_mut().for_each(Leveler::reset);
        }
        self.next_start = Some(end);
        self.scratch.resize(out.len(), 0.0);
        for di in 0..self.levelers.len() {
            self.scratch.fill(0.0);
            let mut any = false;
            for track in self.tracks.iter_mut().filter(|t| t.device_idx == di) {
                let t_end = track.start_frame + track.len_frames;
                let from = start_frame.max(track.start_frame);
                let to = end.min(t_end);
                if from >= to || track.gain == 0.0 {
                    continue;
                }
                if track.reader.is_none() {
                    track.reader = Some(SegReader::open(track.path.clone())?);
                }
                let reader = track.reader.as_mut().expect("opened above");
                let lo = ((from - start_frame) as usize) * OUT_CHANNELS;
                let hi = ((to - start_frame) as usize) * OUT_CHANNELS;
                reader.add_into(from - track.start_frame, track.gain, &mut self.scratch[lo..hi])?;
                any = true;
            }
            if !any {
                self.levelers[di].reset();
                continue;
            }
            if self.leveling {
                self.levelers[di].process(&mut self.scratch);
            }
            for (o, s) in out.iter_mut().zip(&self.scratch) {
                *o += *s;
            }
        }
        for frame in out.chunks_exact_mut(OUT_CHANNELS) {
            let peak = frame[0].abs().max(frame[1].abs());
            let need = if peak > LIMIT_CEILING { LIMIT_CEILING / peak } else { 1.0 };
            let coef = if need < self.limiter { self.limiter_attack } else { self.limiter_release };
            self.limiter += (need - self.limiter) * coef;
            frame[0] = soft_clip(frame[0] * self.limiter);
            frame[1] = soft_clip(frame[1] * self.limiter);
        }
        Ok(())
    }
}

pub(crate) fn read_resampled(path: &Path, sink: &mut dyn FnMut(&[f32])) -> Result<(), MixdownError> {
    let mut reader = SegReader::open(path.to_path_buf())?;
    let total = reader.src_frames * OUT_RATE as u64 / reader.reader.rate() as u64;
    let mut buf = vec![0f32; BLOCK_FRAMES * OUT_CHANNELS];
    let mut pos = 0u64;
    while pos < total {
        let n = (total - pos).min(BLOCK_FRAMES as u64) as usize;
        let block = &mut buf[..n * OUT_CHANNELS];
        block.fill(0.0);
        reader.add_into(pos, 1.0, block)?;
        sink(block);
        pos += n as u64;
    }
    Ok(())
}

pub fn mixdown_to_file(session_dir: &Path, only: Option<&DeviceKey>, out_path: &Path) -> Result<(), MixdownError> {
    let settings = mix::load(session_dir)?;
    let mut mix = MixSource::open(session_dir, only, &settings)?;
    let spec = hound::WavSpec {
        channels: OUT_CHANNELS as u16,
        sample_rate: OUT_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let write_err = |source| MixdownError::Write { path: out_path.to_path_buf(), source };
    let mut writer = hound::WavWriter::create(out_path, spec).map_err(write_err)?;
    let mut buf = vec![0f32; BLOCK_FRAMES * OUT_CHANNELS];
    let mut pos = 0u64;
    while pos < mix.total_frames() {
        let n = (mix.total_frames() - pos).min(BLOCK_FRAMES as u64) as usize;
        let block = &mut buf[..n * OUT_CHANNELS];
        mix.fill(pos, block)?;
        for &s in block.iter() {
            writer.write_sample(s).map_err(write_err)?;
        }
        pos += n as u64;
    }
    writer.finalize().map_err(write_err)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use audio_recorder_schema::manifest::{Direction, ManifestDevice, SegmentStatus};

    use super::*;

    fn write_wav(path: &Path, rate: u32, channels: u16, frames: usize, value: f32) {
        let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for _ in 0..frames * channels as usize {
            w.write_sample(value).unwrap();
        }
        w.finalize().unwrap();
    }

    fn session(name: &str, segs: &[(&str, &str, u64, u32, u16, usize, f32)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("audio-recorder-mix-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tracks")).unwrap();
        let mut m = manifest::new_manifest("2026-01-01T00:00:00Z".into());
        m.ended_at = Some("2026-01-01T00:00:10Z".into());
        for (device, file, start_ns, rate, channels, frames, value) in segs {
            write_wav(&dir.join(file), *rate, *channels, *frames, *value);
            m.devices.insert(
                device.to_string(),
                ManifestDevice { description: device.to_string(), direction: Direction::Input },
            );
            m.segments.push(Segment {
                channels: NonZeroU64::new(*channels as u64).unwrap(),
                device: device.to_string(),
                end_offset_ns: Some(start_ns + *frames as u64 * 1_000_000_000 / *rate as u64),
                file: file.to_string(),
                frames: *frames as u64,
                sample_rate: NonZeroU64::new(*rate as u64).unwrap(),
                start_offset_ns: *start_ns,
                status: SegmentStatus::Complete,
            });
        }
        manifest::save(&dir, &m).unwrap();
        dir
    }

    #[test]
    fn places_segments_at_offsets_and_sums_overlap() {
        let dir = session(
            "place",
            &[
                ("input:a", "tracks/a.wav", 0, 48000, 2, 48000, 0.25),
                ("input:b", "tracks/b.wav", 500_000_000, 48000, 1, 24000, 0.5),
            ],
        );
        let mut mix = MixSource::open(&dir, None, &mix::default_settings()).unwrap();
        assert_eq!(mix.total_frames(), 48000);
        let mut out = vec![0f32; 48000 * 2];
        mix.fill(0, &mut out).unwrap();
        assert!((out[2 * 1000] - 0.25).abs() < 1e-5);
        assert!((out[2 * 23999] - 0.25).abs() < 1e-5);
        assert!((out[2 * 24100] - 0.75).abs() < 1e-5);
        assert!((out[2 * 24100 + 1] - 0.75).abs() < 1e-5);
    }

    #[test]
    fn filter_selects_one_device_and_gaps_are_silent() {
        let dir = session(
            "filter",
            &[
                ("input:a", "tracks/a.wav", 0, 48000, 1, 1000, 0.5),
                ("input:a", "tracks/a2.wav", 1_000_000_000, 48000, 1, 1000, 0.5),
                ("input:b", "tracks/b.wav", 0, 48000, 1, 1000, 0.5),
            ],
        );
        let key: DeviceKey = "input:a".parse().unwrap();
        let mut mix = MixSource::open(&dir, Some(&key), &mix::default_settings()).unwrap();
        let mut out = vec![0f32; 2 * 2000];
        mix.fill(0, &mut out).unwrap();
        assert!((out[2 * 10] - 0.5).abs() < 1e-5);
        assert_eq!(out[2 * 1500], 0.0);
        let mut later = vec![0f32; 2 * 100];
        mix.fill(48000 + 10, &mut later).unwrap();
        assert!((later[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn gains_scale_and_mute_and_update_live() {
        use audio_recorder_schema::mix::ChannelGain;
        let dir = session(
            "gain",
            &[("input:a", "tracks/a.wav", 0, 48000, 1, 48000, 0.2), ("input:b", "tracks/b.wav", 0, 48000, 1, 48000, 0.1)],
        );
        let mut settings = mix::default_settings();
        settings.devices.insert("input:a".into(), ChannelGain { gain: 0.5, muted: false });
        settings.segments.insert("tracks/b.wav".into(), ChannelGain { gain: 2.0, muted: false });
        let mut m = MixSource::open(&dir, None, &settings).unwrap();
        let mut out = vec![0f32; 2 * 100];
        m.fill(1000, &mut out).unwrap();
        assert!((out[0] - 0.3).abs() < 1e-5);

        settings.devices.insert("input:a".into(), ChannelGain { gain: 0.5, muted: true });
        m.set_mix(&settings);
        m.fill(1100, &mut out).unwrap();
        assert!((out[0] - 0.2).abs() < 1e-5);

        settings.devices.insert("input:a".into(), ChannelGain { gain: 1.0, muted: false });
        m.set_mix(&settings);
        m.fill(1200, &mut out).unwrap();
        assert!((out[0] - 0.4).abs() < 1e-5);
        m.fill(5000, &mut out).unwrap();
        assert!((out[0] - 0.4).abs() < 1e-5);
    }

    #[test]
    fn single_device_ignores_device_mute() {
        use audio_recorder_schema::mix::ChannelGain;
        let dir = session("solo", &[("input:a", "tracks/a.wav", 0, 48000, 1, 1000, 0.5)]);
        let mut settings = mix::default_settings();
        settings.devices.insert("input:a".into(), ChannelGain { gain: 1.0, muted: true });
        let key: DeviceKey = "input:a".parse().unwrap();
        let mut m = MixSource::open(&dir, Some(&key), &settings).unwrap();
        let mut out = vec![0f32; 2 * 10];
        m.fill(0, &mut out).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-5);
        let mut all = MixSource::open(&dir, None, &settings).unwrap();
        all.fill(0, &mut out).unwrap();
        assert_eq!(out[0], 0.0);
    }

    fn rms(out: &[f32]) -> f32 {
        (out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32).sqrt()
    }

    #[test]
    fn leveler_lifts_speech_but_not_the_noise_floor() {
        let mut settings = mix::default_settings();
        settings.leveling = true;
        let dir = session(
            "level",
            &[
                ("input:a", "tracks/a.wav", 0, 48000, 1, 144_000, 0.0005),
                ("input:a", "tracks/a2.wav", 3_000_000_000, 48000, 1, 96_000, 0.05),
                ("input:a", "tracks/a3.wav", 5_000_000_000, 48000, 1, 96_000, 0.4),
            ],
        );
        let mut m = MixSource::open(&dir, None, &settings).unwrap();
        let mut out = vec![0f32; 2 * 4800];
        let (mut noise, mut quiet, mut loud) = (0.0, 0.0, 0.0);
        let mut f = 0;
        while f < 336_000 {
            m.fill(f, &mut out).unwrap();
            let r = rms(&out);
            if f == 100_800 { noise = r }
            if f == 235_200 { quiet = r }
            if f == 331_200 { loud = r }
            f += 4800;
        }
        assert!(noise < 0.001, "noise {noise}");
        assert!(quiet > 0.075 && quiet < 0.11, "quiet {quiet}");
        assert!((loud - 0.1).abs() < 0.01, "loud {loud}");
    }

    #[test]
    fn limiter_keeps_summed_boost_under_ceiling() {
        use audio_recorder_schema::mix::ChannelGain;
        let dir = session(
            "limit",
            &[("input:a", "tracks/a.wav", 0, 48000, 1, 9600, 0.8), ("input:b", "tracks/b.wav", 0, 48000, 1, 9600, 0.8)],
        );
        let mut settings = mix::default_settings();
        settings.devices.insert("input:a".into(), ChannelGain { gain: 1.5, muted: false });
        let mut m = MixSource::open(&dir, None, &settings).unwrap();
        let mut out = vec![0f32; 2 * 4800];
        m.fill(0, &mut out).unwrap();
        assert!(out.iter().all(|s| s.abs() <= 1.0));
        assert!(out[2 * 1000..].iter().all(|s| s.abs() <= 0.985));
        assert!(out[2 * 4000] > 0.9);
        let max_step = out.chunks_exact(2).map(|f| f[0]).collect::<Vec<_>>().windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(max_step < 0.05, "step {max_step}");
    }

    #[test]
    fn resamples_to_output_rate() {
        let dir = session("resample", &[("input:a", "tracks/a.wav", 0, 24000, 1, 24000, 0.5)]);
        let mut mix = MixSource::open(&dir, None, &mix::default_settings()).unwrap();
        assert_eq!(mix.total_frames(), 48000);
        let mut out = vec![0f32; 2 * 4800];
        mix.fill(10_000, &mut out).unwrap();
        assert!((out[0] - 0.5).abs() < 1e-4);
        assert!((out[2 * 4000] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn read_segment_mono_downmixes_at_native_rate() {
        let dir = session("mono", &[("input:a", "tracks/a.wav", 0, 16000, 2, 1600, 0.5)]);
        let (samples, rate) = read_segment_mono(&dir.join("tracks/a.wav")).unwrap();
        assert_eq!(rate, 16000);
        assert_eq!(samples.len(), 1600);
        assert!((samples[10] - 0.5).abs() < 1e-3);
    }

    #[test]
    fn seek_backwards_rereads() {
        let dir = session("seek", &[("input:a", "tracks/a.wav", 0, 48000, 1, 48000, 0.3)]);
        let mut mix = MixSource::open(&dir, None, &mix::default_settings()).unwrap();
        let mut out = vec![0f32; 2 * 100];
        mix.fill(40_000, &mut out).unwrap();
        mix.fill(100, &mut out).unwrap();
        assert!((out[0] - 0.3).abs() < 1e-5);
    }

    #[test]
    fn mixdown_file_has_expected_length() {
        let dir = session("file", &[("input:a", "tracks/a.wav", 250_000_000, 48000, 1, 4800, 0.5)]);
        let out = dir.join("mix.wav");
        mixdown_to_file(&dir, None, &out).unwrap();
        let r = hound::WavReader::open(&out).unwrap();
        assert_eq!(r.duration(), 12000 + 4800);
    }
}

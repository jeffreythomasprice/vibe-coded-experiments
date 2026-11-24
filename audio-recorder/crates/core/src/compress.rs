use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufWriter, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use audio_recorder_schema::{config::Codec, manifest::Segment};
use crate::{
    audio_file::{AudioFileError, Format, OPUS_RATE, SegmentReader},
    balance::{self, BalanceError},
    config::CompressionSettings,
    flac_writer::{FLAC_BLOCK, FLAC_FLOAT_BITS, FlacWriteError, FlacWriter, f32_to_24},
    mix::{self, MixError},
    session::manifest::{self, ManifestError},
    transcript::{self, TranscriptError},
};

const OPUS_FRAME: usize = 960;
const OPUS_PACKETS_PER_PAGE: usize = 25;
const OPUS_MAX_PACKET: usize = 4000;
const OPUS_SERIAL: u32 = 0x4152_4543;
const VERIFY_TOLERANCE: f32 = 1e-6;
const VERIFY_BLOCK_FRAMES: usize = 8192;

#[derive(Debug, thiserror::Error)]
pub enum CompressError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Mix(#[from] MixError),
    #[error(transparent)]
    Transcript(#[from] TranscriptError),
    #[error(transparent)]
    Balance(#[from] BalanceError),
    #[error(transparent)]
    Audio(#[from] AudioFileError),
    #[error("invalid WAV {}: {source}", path.display())]
    Wav { path: PathBuf, source: hound::Error },
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Flac(#[from] FlacWriteError),
    #[error("Opus encoding of {} failed: {source}", path.display())]
    Opus { path: PathBuf, source: opus::Error },
    #[error("unsupported WAV format in {}: {detail}", path.display())]
    Unsupported { path: PathBuf, detail: String },
    #[error("verification of {} failed: {detail}", path.display())]
    Verify { path: PathBuf, detail: String },
    #[error("the session is still recording")]
    SessionActive,
    #[error("cancelled")]
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompressSummary {
    pub segments: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

fn target_for(seg: &Segment, settings: &CompressionSettings) -> Option<Format> {
    let target = target_format(settings, seg.sample_rate.get() as u32, seg.channels.get() as u16);
    match Format::of(Path::new(&seg.file))? {
        Format::Wav => Some(target),
        Format::Flac if target == Format::Opus => Some(target),
        _ => None,
    }
}

pub fn pending_segments(dir: &Path, settings: &CompressionSettings) -> Result<usize, ManifestError> {
    let manifest = manifest::load(dir)?;
    Ok(manifest.segments.iter().filter(|s| target_for(s, settings).is_some()).count())
}

trait F32Source {
    fn frames(&self) -> u64;
    fn channels(&self) -> usize;
    fn read_f32(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<usize, CompressError>;
}

impl F32Source for Source {
    fn frames(&self) -> u64 {
        self.reader.duration() as u64
    }

    fn channels(&self) -> usize {
        self.spec.channels as usize
    }

    fn read_f32(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<usize, CompressError> {
        Source::read_f32(self, frames, out)
    }
}

impl F32Source for SegmentReader {
    fn frames(&self) -> u64 {
        SegmentReader::frames(self)
    }

    fn channels(&self) -> usize {
        SegmentReader::channels(self)
    }

    fn read_f32(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<usize, CompressError> {
        Ok(self.read(out, frames)?)
    }
}

fn open_f32(path: &Path) -> Result<Box<dyn F32Source>, CompressError> {
    Ok(match Format::of(path) {
        Some(Format::Wav) => Box::new(Source::open(path)?),
        _ => Box::new(SegmentReader::open(path)?),
    })
}

struct Tick<'a> {
    progress: &'a dyn Fn(f32),
    cancel: &'a AtomicBool,
    done: u64,
    total: u64,
}

impl Tick<'_> {
    fn advance(&mut self, frames: u64) -> Result<(), CompressError> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(CompressError::Cancelled);
        }
        self.done += frames;
        (self.progress)((self.done as f32 / self.total.max(1) as f32).min(1.0));
        Ok(())
    }
}

struct Source {
    path: PathBuf,
    reader: hound::WavReader<io::BufReader<File>>,
    spec: hound::WavSpec,
}

impl Source {
    fn open(path: &Path) -> Result<Self, CompressError> {
        let reader = hound::WavReader::open(path).map_err(|source| CompressError::Wav { path: path.to_path_buf(), source })?;
        let spec = reader.spec();
        if !matches!((spec.sample_format, spec.bits_per_sample), (hound::SampleFormat::Float, 32) | (hound::SampleFormat::Int, 16)) {
            return Err(CompressError::Unsupported {
                path: path.to_path_buf(),
                detail: format!("{}-bit {:?}", spec.bits_per_sample, spec.sample_format),
            });
        }
        Ok(Self { path: path.to_path_buf(), reader, spec })
    }

    fn read_f32(&mut self, frames: usize, out: &mut Vec<f32>) -> Result<usize, CompressError> {
        let want = frames * self.spec.channels as usize;
        let before = out.len();
        let err = |source| CompressError::Wav { path: self.path.clone(), source };
        match self.spec.sample_format {
            hound::SampleFormat::Float => {
                for s in self.reader.samples::<f32>().take(want) {
                    out.push(s.map_err(err)?);
                }
            }
            hound::SampleFormat::Int => {
                for s in self.reader.samples::<i16>().take(want) {
                    out.push(s.map_err(err)? as f32 / 32768.0);
                }
            }
        }
        let whole = (out.len() - before) / self.spec.channels as usize;
        out.truncate(before + whole * self.spec.channels as usize);
        Ok(whole)
    }

    fn read_i32(&mut self, frames: usize, out: &mut Vec<i32>) -> Result<usize, CompressError> {
        let want = frames * self.spec.channels as usize;
        let before = out.len();
        let err = |source| CompressError::Wav { path: self.path.clone(), source };
        match self.spec.sample_format {
            hound::SampleFormat::Float => {
                for s in self.reader.samples::<f32>().take(want) {
                    out.push(f32_to_24(s.map_err(err)?));
                }
            }
            hound::SampleFormat::Int => {
                for s in self.reader.samples::<i16>().take(want) {
                    out.push(s.map_err(err)? as i32);
                }
            }
        }
        let whole = (out.len() - before) / self.spec.channels as usize;
        out.truncate(before + whole * self.spec.channels as usize);
        Ok(whole)
    }
}

fn io_err(path: &Path) -> impl Fn(io::Error) -> CompressError + '_ {
    move |source| CompressError::Io { path: path.to_path_buf(), source }
}

fn encode_flac(src: &mut Source, dst: &Path, tick: &mut Tick) -> Result<(), CompressError> {
    let bits = if src.spec.sample_format == hound::SampleFormat::Int { 16 } else { FLAC_FLOAT_BITS };
    let mut writer = FlacWriter::create(dst, src.spec.sample_rate, src.spec.channels as usize, bits)?;
    let mut samples = Vec::with_capacity(FLAC_BLOCK * src.spec.channels as usize);
    loop {
        samples.clear();
        let n = src.read_i32(FLAC_BLOCK, &mut samples)?;
        if n == 0 {
            break;
        }
        writer.write(&samples)?;
        tick.advance(n as u64)?;
    }
    Ok(writer.finalize()?)
}

fn opus_head(channels: u8, pre_skip: u16, input_rate: u32) -> Vec<u8> {
    let mut v = b"OpusHead".to_vec();
    v.extend([1, channels]);
    v.extend(pre_skip.to_le_bytes());
    v.extend(input_rate.to_le_bytes());
    v.extend(0i16.to_le_bytes());
    v.push(0);
    v
}

fn opus_tags() -> Vec<u8> {
    let vendor = b"audio-recorder";
    let mut v = b"OpusTags".to_vec();
    v.extend((vendor.len() as u32).to_le_bytes());
    v.extend(vendor);
    v.extend(0u32.to_le_bytes());
    v
}

fn encode_opus(src: &mut dyn F32Source, dst: &Path, bitrate_kbps: u32, tick: &mut Tick) -> Result<(), CompressError> {
    let opus_err = |source| CompressError::Opus { path: dst.to_path_buf(), source };
    let channels = src.channels();
    let layout = if channels == 1 { opus::Channels::Mono } else { opus::Channels::Stereo };
    let mut encoder = opus::Encoder::new(OPUS_RATE, layout, opus::Application::Audio).map_err(opus_err)?;
    encoder.set_bitrate(opus::Bitrate::Bits((bitrate_kbps * 1000 * channels as u32) as i32)).map_err(opus_err)?;
    let pre_skip = encoder.get_lookahead().map_err(opus_err)?.max(0) as u64;
    let frames = src.frames();

    let file = File::create(dst).map_err(io_err(dst))?;
    let mut writer = ogg::PacketWriter::new(BufWriter::new(file));
    writer
        .write_packet(opus_head(channels as u8, pre_skip as u16, OPUS_RATE), OPUS_SERIAL, ogg::PacketWriteEndInfo::EndPage, 0)
        .map_err(io_err(dst))?;
    writer.write_packet(opus_tags(), OPUS_SERIAL, ogg::PacketWriteEndInfo::EndPage, 0).map_err(io_err(dst))?;

    let end_granule = frames + pre_skip;
    let blocks = end_granule.div_ceil(OPUS_FRAME as u64);
    let mut input = Vec::with_capacity(OPUS_FRAME * channels);
    let mut packet = vec![0u8; OPUS_MAX_PACKET];
    for block in 0..blocks {
        input.clear();
        let got = src.read_f32(OPUS_FRAME, &mut input)?;
        input.resize(OPUS_FRAME * channels, 0.0);
        let n = encoder.encode_float(&input, &mut packet).map_err(opus_err)?;
        let granule = ((block + 1) * OPUS_FRAME as u64).min(end_granule);
        let end = if block + 1 == blocks {
            ogg::PacketWriteEndInfo::EndStream
        } else if (block + 1) % OPUS_PACKETS_PER_PAGE as u64 == 0 {
            ogg::PacketWriteEndInfo::EndPage
        } else {
            ogg::PacketWriteEndInfo::NormalPacket
        };
        writer.write_packet(packet[..n].to_vec(), OPUS_SERIAL, end, granule).map_err(io_err(dst))?;
        tick.advance(got as u64)?;
    }
    let mut inner = writer.into_inner();
    inner.flush().map_err(io_err(dst))?;
    inner.into_inner().map_err(|e| io_err(dst)(e.into_error()))?.sync_all().map_err(io_err(dst))
}

fn verify(src_path: &Path, encoded: &Path, lossless: bool, frames: u64) -> Result<(), CompressError> {
    let fail = |detail: String| CompressError::Verify { path: encoded.to_path_buf(), detail };
    let mut decoded = SegmentReader::open(encoded)?;
    if decoded.frames() != frames {
        return Err(fail(format!("expected {frames} frames, found {}", decoded.frames())));
    }
    let mut src = open_f32(src_path)?;
    let channels = src.channels();
    let (mut a, mut b) = (Vec::new(), Vec::new());
    let mut seen = 0u64;
    loop {
        a.clear();
        b.clear();
        let n = src.read_f32(VERIFY_BLOCK_FRAMES, &mut a)?;
        let m = decoded.read(&mut b, VERIFY_BLOCK_FRAMES)?;
        if n != m {
            return Err(fail(format!("decoded {} frames, expected {}", seen + m as u64, seen + n as u64)));
        }
        if n == 0 {
            return Ok(());
        }
        if lossless && let Some(i) = a.iter().zip(&b).position(|(x, y)| (x.clamp(-1.0, 1.0) - y).abs() > VERIFY_TOLERANCE) {
            return Err(fail(format!("sample mismatch at frame {}", seen + (i / channels) as u64)));
        }
        seen += n as u64;
    }
}

fn target_format(settings: &CompressionSettings, rate: u32, channels: u16) -> Format {
    match settings.codec {
        Codec::Opus if rate == OPUS_RATE && channels <= 2 => Format::Opus,
        _ => Format::Flac,
    }
}

fn temp_name(dst: &Path) -> PathBuf {
    let ext = dst.extension().and_then(|e| e.to_str()).unwrap_or_default();
    dst.with_extension(format!("tmp.{ext}"))
}

fn compress_file(src_path: &Path, settings: &CompressionSettings, tick: &mut Tick) -> Result<Format, CompressError> {
    let from = Format::of(src_path);
    let (rate, channels) = match from {
        Some(Format::Wav) => {
            let spec = Source::open(src_path)?.spec;
            (spec.sample_rate, spec.channels)
        }
        _ => {
            let r = SegmentReader::open(src_path)?;
            (r.rate(), r.channels() as u16)
        }
    };
    let format = target_format(settings, rate, channels);
    if settings.codec == Codec::Opus && format != Format::Opus {
        tracing::warn!("{}: {rate} Hz x {channels} is not supported by Opus here; using FLAC", src_path.display());
    }
    let dst = src_path.with_extension(format.extension());
    let tmp = temp_name(&dst);
    let mut src = open_f32(src_path)?;
    let frames = src.frames();
    let result = match (format, from) {
        (Format::Opus, _) => encode_opus(&mut *src, &tmp, settings.opus_bitrate_kbps, tick),
        _ => {
            drop(src);
            encode_flac(&mut Source::open(src_path)?, &tmp, tick)
        }
    }
    .and_then(|()| verify(src_path, &tmp, format == Format::Flac, frames))
    .and_then(|()| std::fs::rename(&tmp, &dst).map_err(io_err(&dst)));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map(|()| format)
}

fn rename_references(dir: &Path, map: &HashMap<String, String>) -> Result<(), CompressError> {
    let mut mix = mix::load(dir)?;
    if mix.segments.keys().any(|k| map.contains_key(k)) {
        mix.segments = std::mem::take(&mut mix.segments).into_iter().map(|(k, v)| (map.get(&k).cloned().unwrap_or(k), v)).collect();
        mix::save(dir, &mix)?;
    }
    let remap = |mut t: audio_recorder_schema::transcript::Transcript| {
        let mut changed = false;
        for s in &mut t.snippets {
            if let Some(new) = map.get(&s.segment) {
                s.segment = new.clone();
                changed = true;
            }
        }
        (t, changed)
    };
    if let Some((t, true)) = transcript::load(dir)?.map(remap) {
        transcript::save(dir, &t)?;
    }
    if let Some((t, true)) = transcript::load_live(dir)?.map(remap) {
        transcript::save_live(dir, &t)?;
    }
    balance::rename_files(dir, map)?;
    Ok(())
}

pub fn compress_session(
    dir: &Path,
    settings: &CompressionSettings,
    progress: &dyn Fn(f32),
    cancel: &AtomicBool,
) -> Result<CompressSummary, CompressError> {
    if manifest::is_recording(dir) {
        return Err(CompressError::SessionActive);
    }
    let mut manifest = manifest::load_recovering(dir)?;
    let mut todo = Vec::new();
    let mut total = 0u64;
    for (i, seg) in manifest.segments.iter().enumerate() {
        if target_for(seg, settings).is_none() {
            continue;
        }
        let path = dir.join(&seg.file);
        if !path.is_file() {
            tracing::warn!("not compressing missing segment {}", path.display());
            continue;
        }
        total += seg.frames;
        todo.push(i);
    }

    let mut tick = Tick { progress, cancel, done: 0, total };
    let mut summary = CompressSummary::default();
    let mut map = HashMap::new();
    for i in todo {
        let old = manifest.segments[i].file.clone();
        let src_path = dir.join(&old);
        let formats = compress_file(&src_path, settings, &mut tick)?;
        let new = Path::new(&old).with_extension(formats.extension()).to_string_lossy().into_owned();
        summary.segments += 1;
        summary.bytes_before += std::fs::metadata(&src_path).map_err(io_err(&src_path))?.len();
        summary.bytes_after += std::fs::metadata(dir.join(&new)).map_err(io_err(&dir.join(&new)))?.len();
        map.insert(old, new);
    }
    if map.is_empty() {
        return Ok(summary);
    }

    rename_references(dir, &map)?;
    for seg in &mut manifest.segments {
        if let Some(new) = map.get(&seg.file) {
            seg.file = new.clone();
        }
    }
    manifest::save(dir, &manifest)?;
    for old in map.keys() {
        let path = dir.join(old);
        if let Err(e) = std::fs::remove_file(&path) {
            tracing::warn!("could not remove {}: {e}", path.display());
        }
    }
    progress(1.0);
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use audio_recorder_schema::{
        manifest::{Direction, ManifestDevice, Segment, SegmentStatus},
        mix::ChannelGain,
    };

    use super::*;
    use crate::mixdown::MixSource;

    fn signal(i: usize, rate: u32, c: u16) -> f32 {
        let t = i as f32 / rate as f32;
        let tone = 0.3 * (2.0 * std::f32::consts::PI * (220.0 + 40.0 * c as f32) * t).sin();
        let env = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
        tone + 0.1 * env * (2.0 * std::f32::consts::PI * 1330.0 * t).sin()
    }

    fn write_wav(path: &Path, rate: u32, channels: u16, frames: usize, int16: bool) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: if int16 { 16 } else { 32 },
            sample_format: if int16 { hound::SampleFormat::Int } else { hound::SampleFormat::Float },
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..frames {
            for c in 0..channels {
                let v = signal(i, rate, c);
                if int16 {
                    w.write_sample((v * 32767.0) as i16).unwrap();
                } else {
                    w.write_sample(v).unwrap();
                }
            }
        }
        w.finalize().unwrap();
    }

    fn session(name: &str, segs: &[(&str, u32, u16, usize, bool)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("audio-recorder-compress-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tracks")).unwrap();
        let mut m = manifest::new_manifest("2026-01-01T00:00:00Z".into());
        m.ended_at = Some("2026-01-01T00:00:10Z".into());
        for (i, (device, rate, channels, frames, int16)) in segs.iter().enumerate() {
            let file = format!("tracks/seg-{i}.wav");
            write_wav(&dir.join(&file), *rate, *channels, *frames, *int16);
            m.devices.insert(device.to_string(), ManifestDevice { description: device.to_string(), direction: Direction::Input });
            m.segments.push(Segment {
                channels: NonZeroU64::new(*channels as u64).unwrap(),
                device: device.to_string(),
                end_offset_ns: Some(*frames as u64 * 1_000_000_000 / *rate as u64),
                file,
                frames: *frames as u64,
                sample_rate: NonZeroU64::new(*rate as u64).unwrap(),
                start_offset_ns: 0,
                status: SegmentStatus::Complete,
            });
        }
        manifest::save(&dir, &m).unwrap();
        dir
    }

    fn settings(codec: Codec) -> CompressionSettings {
        CompressionSettings { auto: true, live: true, codec, opus_bitrate_kbps: 48 }
    }

    fn run(dir: &Path, codec: Codec) -> Result<CompressSummary, CompressError> {
        compress_session(dir, &settings(codec), &|_| {}, &AtomicBool::new(false))
    }

    fn render(dir: &Path) -> Vec<f32> {
        let mut mix = MixSource::open(dir, None, &mix::load(dir).unwrap()).unwrap();
        let mut out = vec![0f32; 2 * 24_000];
        mix.fill(10_000, &mut out).unwrap();
        out
    }

    #[test]
    fn flac_session_is_smaller_lossless_and_rewritten_everywhere() {
        let dir = session("flac", &[("input:a", 48_000, 2, 96_000, false), ("input:b", 48_000, 1, 48_000, true)]);
        let mut settings = mix::default_settings();
        settings.segments.insert("tracks/seg-0.wav".into(), ChannelGain { gain: 0.5, muted: false });
        mix::save(&dir, &settings).unwrap();
        let before = render(&dir);

        let summary = run(&dir, Codec::Flac).unwrap();
        assert_eq!(summary.segments, 2);
        assert!(summary.bytes_after < summary.bytes_before);

        let m = manifest::load(&dir).unwrap();
        assert_eq!(m.segments[0].file, "tracks/seg-0.flac");
        assert_eq!(m.segments[1].file, "tracks/seg-1.flac");
        assert!(!dir.join("tracks/seg-0.wav").exists() && !dir.join("tracks/seg-1.wav").exists());
        assert!(mix::load(&dir).unwrap().segments.contains_key("tracks/seg-0.flac"));

        let after = render(&dir);
        let err = before.iter().zip(&after).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(err < 1e-4, "max error {err}");
        assert_eq!(run(&dir, Codec::Flac).unwrap().segments, 0);
    }

    #[test]
    fn i16_flac_is_bit_exact() {
        let dir = session("i16", &[("input:a", 16_000, 1, 40_000, true)]);
        let original = dir.join("tracks/seg-0.wav");
        let mut src = Source::open(&original).unwrap();
        let mut expected = Vec::new();
        src.read_f32(usize::MAX, &mut expected).unwrap();
        run(&dir, Codec::Flac).unwrap();
        let mut r = SegmentReader::open(&dir.join("tracks/seg-0.flac")).unwrap();
        let mut got = Vec::new();
        r.read(&mut got, usize::MAX).unwrap();
        assert_eq!(got, expected);
    }

    #[test]
    fn flac_seek_matches_sequential_decode() {
        let dir = session("flacseek", &[("input:a", 48_000, 2, 480_000, false)]);
        run(&dir, Codec::Flac).unwrap();
        let info = claxon::FlacReader::open(dir.join("tracks/seg-0.flac")).unwrap().streaminfo();
        assert_eq!(info.min_block_size, info.max_block_size);
        let mut r = SegmentReader::open(&dir.join("tracks/seg-0.flac")).unwrap();
        let mut all = Vec::new();
        r.read(&mut all, usize::MAX).unwrap();
        for target in [300_123usize, 5, 470_000, 131_072, 200_000] {
            r.seek(target as u64).unwrap();
            let mut got = Vec::new();
            assert_eq!(r.read(&mut got, 1000).unwrap(), 1000);
            assert_eq!(got, all[target * 2..(target + 1000) * 2], "target {target}");
        }
    }

    #[test]
    fn opus_keeps_length_and_seeks_accurately() {
        let dir = session("opus", &[("input:a", 48_000, 1, 480_000, false)]);
        let summary = run(&dir, Codec::Opus).unwrap();
        assert!(summary.bytes_after * 20 < summary.bytes_before);
        let path = dir.join("tracks/seg-0.opus");
        let mut r = SegmentReader::open(&path).unwrap();
        assert_eq!(r.frames(), 480_000);
        let mut all = Vec::new();
        assert_eq!(r.read(&mut all, usize::MAX).unwrap(), 480_000);

        let signal_power: f32 = all.iter().map(|v| v * v).sum::<f32>() / all.len() as f32;
        let mut r = SegmentReader::open(&path).unwrap();
        let mut original = Vec::new();
        let wav = temp_wav_for(&dir);
        Source::open(&wav).unwrap().read_f32(usize::MAX, &mut original).unwrap();
        let noise: f32 = all.iter().zip(&original).map(|(a, b)| (a - b) * (a - b)).sum::<f32>() / all.len() as f32;
        assert!(10.0 * (signal_power / noise).log10() > 15.0, "snr too low");

        for target in [123_456usize, 477_000, 20_000] {
            r.seek(target as u64).unwrap();
            let mut got = Vec::new();
            assert_eq!(r.read(&mut got, 2000).unwrap(), 2000);
            let diff = got.iter().zip(&all[target..target + 2000]).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
            assert!(diff < 2e-3, "target {target} diff {diff}");
        }
    }

    #[test]
    fn flac_sessions_are_left_alone_for_flac_and_transcoded_for_opus() {
        let dir = session("flac-to-opus", &[("input:a", 48_000, 1, 96_000, false)]);
        run(&dir, Codec::Flac).unwrap();
        assert_eq!(pending_segments(&dir, &settings(Codec::Flac)).unwrap(), 0);
        assert_eq!(run(&dir, Codec::Flac).unwrap().segments, 0);
        assert_eq!(pending_segments(&dir, &settings(Codec::Opus)).unwrap(), 1);

        let summary = run(&dir, Codec::Opus).unwrap();
        assert_eq!(summary.segments, 1);
        assert!(summary.bytes_after < summary.bytes_before);
        assert_eq!(manifest::load(&dir).unwrap().segments[0].file, "tracks/seg-0.opus");
        assert!(!dir.join("tracks/seg-0.flac").exists());
        assert_eq!(pending_segments(&dir, &settings(Codec::Opus)).unwrap(), 0);
        assert_eq!(SegmentReader::open(&dir.join("tracks/seg-0.opus")).unwrap().frames(), 96_000);
    }

    fn temp_wav_for(dir: &Path) -> PathBuf {
        let path = dir.join("reference.wav");
        write_wav(&path, 48_000, 1, 480_000, false);
        path
    }

    #[test]
    fn opus_falls_back_to_flac_for_unsupported_rates() {
        let dir = session("fallback", &[("input:a", 44_100, 1, 44_100, false)]);
        run(&dir, Codec::Opus).unwrap();
        assert_eq!(manifest::load(&dir).unwrap().segments[0].file, "tracks/seg-0.flac");
    }

    #[test]
    fn cancel_leaves_the_session_untouched() {
        let dir = session("cancel", &[("input:a", 48_000, 1, 48_000, false)]);
        let err = compress_session(&dir, &settings(Codec::Flac), &|_| {}, &AtomicBool::new(true)).unwrap_err();
        assert!(matches!(err, CompressError::Cancelled));
        let names: Vec<_> = std::fs::read_dir(dir.join("tracks")).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1);
        assert_eq!(manifest::load(&dir).unwrap().segments[0].file, "tracks/seg-0.wav");
    }

    #[test]
    fn rerun_after_a_crash_before_the_manifest_commit_converges() {
        let dir = session("crash", &[("input:a", 48_000, 1, 48_000, false)]);
        std::fs::copy(dir.join("tracks/seg-0.wav"), dir.join("tracks/seg-0.wav.keep")).unwrap();
        run(&dir, Codec::Flac).unwrap();
        let committed = std::fs::read(dir.join("manifest.json")).unwrap();
        std::fs::rename(dir.join("tracks/seg-0.wav.keep"), dir.join("tracks/seg-0.wav")).unwrap();
        std::fs::write(dir.join("manifest.json"), String::from_utf8(committed).unwrap().replace("seg-0.flac", "seg-0.wav")).unwrap();
        assert_eq!(run(&dir, Codec::Flac).unwrap().segments, 1);
        assert_eq!(manifest::load(&dir).unwrap().segments[0].file, "tracks/seg-0.flac");
        assert!(!dir.join("tracks/seg-0.wav").exists());
    }
}

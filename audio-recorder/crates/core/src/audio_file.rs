use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use claxon::{frame::FrameReader, input::ReadBytes};

pub const OPUS_RATE: u32 = 48_000;
pub const OPUS_PREROLL: u64 = 3_840;
const FLAC_CHECKPOINT_EVERY: u64 = 1 << 17;
const OPUS_MAX_FRAME: usize = 5_760;

#[derive(Debug, thiserror::Error)]
pub enum AudioFileError {
    #[error("failed to open {}: {source}", path.display())]
    Open { path: PathBuf, source: io::Error },
    #[error("failed to read {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("invalid WAV {}: {source}", path.display())]
    Wav { path: PathBuf, source: hound::Error },
    #[error("invalid FLAC {}: {source}", path.display())]
    Flac { path: PathBuf, source: claxon::Error },
    #[error("invalid Ogg {}: {source}", path.display())]
    Ogg { path: PathBuf, source: ogg::OggReadError },
    #[error("invalid Opus {}: {source}", path.display())]
    Opus { path: PathBuf, source: opus::Error },
    #[error("unsupported audio in {}: {detail}", path.display())]
    Unsupported { path: PathBuf, detail: String },
    #[error("corrupt audio in {}: {detail}", path.display())]
    Corrupt { path: PathBuf, detail: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Wav,
    Flac,
    Opus,
}

impl Format {
    pub fn of(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "wav" => Some(Self::Wav),
            "flac" => Some(Self::Flac),
            "opus" => Some(Self::Opus),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Opus => "opus",
        }
    }
}

pub struct SegmentReader {
    path: PathBuf,
    backend: Backend,
    rate: u32,
    channels: usize,
    frames: u64,
    position: u64,
    pending: Vec<f32>,
    pending_pos: usize,
}

enum Backend {
    Wav(Wav),
    Flac(Flac),
    Opus(Opus),
}

impl SegmentReader {
    pub fn open(path: &Path) -> Result<Self, AudioFileError> {
        let format = Format::of(path).ok_or_else(|| AudioFileError::Unsupported {
            path: path.to_path_buf(),
            detail: "unknown file extension".into(),
        })?;
        let (backend, rate, channels, frames) = match format {
            Format::Wav => {
                let (b, r, c, f) = Wav::open(path)?;
                (Backend::Wav(b), r, c, f)
            }
            Format::Flac => {
                let (b, r, c, f) = Flac::open(path)?;
                (Backend::Flac(b), r, c, f)
            }
            Format::Opus => {
                let (b, r, c, f) = Opus::open(path)?;
                (Backend::Opus(b), r, c, f)
            }
        };
        Ok(Self { path: path.to_path_buf(), backend, rate, channels, frames, position: 0, pending: Vec::new(), pending_pos: 0 })
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    fn refill(&mut self) -> Result<bool, AudioFileError> {
        self.pending.clear();
        self.pending_pos = 0;
        loop {
            let got = match &mut self.backend {
                Backend::Wav(b) => b.decode(&self.path, &mut self.pending)?,
                Backend::Flac(b) => b.decode(&self.path, &mut self.pending)?,
                Backend::Opus(b) => b.decode(&self.path, &mut self.pending)?,
            };
            if !got {
                return Ok(false);
            }
            if !self.pending.is_empty() {
                return Ok(true);
            }
        }
    }

    pub fn next_frame(&mut self) -> Result<Option<&[f32]>, AudioFileError> {
        if self.pending_pos >= self.pending.len() && !self.refill()? {
            return Ok(None);
        }
        let start = self.pending_pos;
        self.pending_pos += self.channels;
        self.position += 1;
        Ok(Some(&self.pending[start..start + self.channels]))
    }

    pub fn read(&mut self, out: &mut Vec<f32>, max_frames: usize) -> Result<usize, AudioFileError> {
        let mut done = 0;
        while done < max_frames {
            if self.pending_pos >= self.pending.len() && !self.refill()? {
                break;
            }
            let avail = (self.pending.len() - self.pending_pos) / self.channels;
            let take = avail.min(max_frames - done);
            let end = self.pending_pos + take * self.channels;
            out.extend_from_slice(&self.pending[self.pending_pos..end]);
            self.pending_pos = end;
            done += take;
        }
        self.position += done as u64;
        Ok(done)
    }

    pub fn seek(&mut self, frame: u64) -> Result<(), AudioFileError> {
        let target = frame.min(self.frames);
        let at = match &mut self.backend {
            Backend::Wav(b) => b.seek(&self.path, target)?,
            Backend::Flac(b) => b.seek(&self.path, target)?,
            Backend::Opus(b) => b.seek(&self.path, target)?,
        };
        self.pending.clear();
        self.pending_pos = 0;
        self.position = at;
        let mut skip = target - at;
        while skip > 0 {
            if self.pending_pos >= self.pending.len() && !self.refill()? {
                break;
            }
            let avail = ((self.pending.len() - self.pending_pos) / self.channels) as u64;
            let take = avail.min(skip);
            self.pending_pos += take as usize * self.channels;
            self.position += take;
            skip -= take;
        }
        Ok(())
    }
}

pub fn read_all_mono(path: &Path) -> Result<(Vec<f32>, u32), AudioFileError> {
    let mut reader = SegmentReader::open(path)?;
    let channels = reader.channels();
    let mut interleaved = Vec::with_capacity(reader.frames() as usize * channels);
    reader.read(&mut interleaved, usize::MAX)?;
    let mono = interleaved.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect();
    Ok((mono, reader.rate()))
}

const WAV_BLOCK_FRAMES: usize = 4096;

struct Wav {
    reader: hound::WavReader<BufReader<File>>,
    spec: hound::WavSpec,
}

impl Wav {
    fn open(path: &Path) -> Result<(Self, u32, usize, u64), AudioFileError> {
        let reader = hound::WavReader::open(path).map_err(|source| AudioFileError::Wav { path: path.to_path_buf(), source })?;
        let spec = reader.spec();
        if !matches!((spec.sample_format, spec.bits_per_sample), (hound::SampleFormat::Float, 32) | (hound::SampleFormat::Int, 16)) {
            return Err(AudioFileError::Unsupported {
                path: path.to_path_buf(),
                detail: format!("{}-bit {:?} WAV", spec.bits_per_sample, spec.sample_format),
            });
        }
        let frames = reader.duration() as u64;
        Ok((Self { reader, spec }, spec.sample_rate, spec.channels as usize, frames))
    }

    fn decode(&mut self, path: &Path, out: &mut Vec<f32>) -> Result<bool, AudioFileError> {
        let want = WAV_BLOCK_FRAMES * self.spec.channels as usize;
        let wav_err = |source| AudioFileError::Wav { path: path.to_path_buf(), source };
        let before = out.len();
        match self.spec.sample_format {
            hound::SampleFormat::Float => {
                for s in self.reader.samples::<f32>().take(want) {
                    out.push(s.map_err(wav_err)?);
                }
            }
            hound::SampleFormat::Int => {
                for s in self.reader.samples::<i16>().take(want) {
                    out.push(s.map_err(wav_err)? as f32 / 32768.0);
                }
            }
        }
        let whole = (out.len() - before) / self.spec.channels as usize * self.spec.channels as usize;
        out.truncate(before + whole);
        Ok(whole > 0)
    }

    fn seek(&mut self, path: &Path, frame: u64) -> Result<u64, AudioFileError> {
        self.reader
            .seek(frame as u32)
            .map_err(|source| AudioFileError::Io { path: path.to_path_buf(), source })?;
        Ok(frame)
    }
}

struct PosReader {
    file: File,
    buf: Box<[u8]>,
    pos: usize,
    len: usize,
    base: u64,
    frame_start: Arc<AtomicU64>,
}

impl PosReader {
    fn open_at(path: &Path, offset: u64, frame_start: Arc<AtomicU64>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        Ok(Self { file, buf: vec![0; 1 << 16].into_boxed_slice(), pos: 0, len: 0, base: offset, frame_start })
    }

    fn fill(&mut self) -> io::Result<bool> {
        self.base += self.len as u64;
        self.pos = 0;
        self.len = self.file.read(&mut self.buf)?;
        Ok(self.len > 0)
    }

    fn eof(what: &str) -> io::Error {
        io::Error::new(io::ErrorKind::UnexpectedEof, what.to_string())
    }
}

impl ReadBytes for PosReader {
    fn read_u8(&mut self) -> io::Result<u8> {
        if self.pos == self.len && !self.fill()? {
            return Err(Self::eof("expected one more byte"));
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_u8_or_eof(&mut self) -> io::Result<Option<u8>> {
        if self.frame_start.load(Ordering::Relaxed) == u64::MAX {
            self.frame_start.store(self.base + self.pos as u64, Ordering::Relaxed);
        }
        if self.pos == self.len && !self.fill()? {
            return Ok(None);
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(Some(b))
    }

    fn read_into(&mut self, buffer: &mut [u8]) -> io::Result<()> {
        let mut done = 0;
        while done < buffer.len() {
            if self.pos == self.len && !self.fill()? {
                return Err(Self::eof("expected more bytes"));
            }
            let n = (buffer.len() - done).min(self.len - self.pos);
            buffer[done..done + n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
            self.pos += n;
            done += n;
        }
        Ok(())
    }

    fn skip(&mut self, mut amount: u32) -> io::Result<()> {
        while amount > 0 {
            if self.pos == self.len && !self.fill()? {
                return Err(Self::eof("expected more bytes"));
            }
            let n = (amount as usize).min(self.len - self.pos);
            self.pos += n;
            amount -= n as u32;
        }
        Ok(())
    }
}

struct Flac {
    reader: FrameReader<PosReader>,
    frame_start: Arc<AtomicU64>,
    data_start: u64,
    scale: f32,
    channels: usize,
    block: Vec<i32>,
    checkpoints: Vec<(u64, u64)>,
}

impl Flac {
    fn open(path: &Path) -> Result<(Self, u32, usize, u64), AudioFileError> {
        let flac_err = |source| AudioFileError::Flac { path: path.to_path_buf(), source };
        let meta = claxon::FlacReader::open(path).map_err(flac_err)?;
        let info = meta.streaminfo();
        drop(meta);
        let data_start = flac_data_start(path)?;
        let frames = match info.samples {
            Some(frames) => frames,
            None if std::fs::metadata(path).is_ok_and(|m| m.len() == data_start) => 0,
            None => {
                return Err(AudioFileError::Unsupported { path: path.to_path_buf(), detail: "FLAC without a total sample count".into() });
            }
        };
        if info.bits_per_sample == 0 || info.bits_per_sample > 24 {
            return Err(AudioFileError::Unsupported { path: path.to_path_buf(), detail: format!("{}-bit FLAC", info.bits_per_sample) });
        }
        let frame_start = Arc::new(AtomicU64::new(0));
        let input = PosReader::open_at(path, data_start, frame_start.clone())
            .map_err(|source| AudioFileError::Open { path: path.to_path_buf(), source })?;
        let channels = info.channels as usize;
        Ok((
            Self {
                reader: FrameReader::new(input),
                frame_start,
                data_start,
                scale: 1.0 / (1u64 << (info.bits_per_sample - 1)) as f32,
                channels,
                block: Vec::new(),
                checkpoints: vec![(0, data_start)],
            },
            info.sample_rate,
            channels,
            frames,
        ))
    }

    fn decode(&mut self, path: &Path, out: &mut Vec<f32>) -> Result<bool, AudioFileError> {
        let buffer = std::mem::take(&mut self.block);
        self.frame_start.store(u64::MAX, Ordering::Relaxed);
        let block = match self.reader.read_next_or_eof(buffer) {
            Ok(Some(b)) => b,
            Ok(None) => return Ok(false),
            Err(source) => return Err(AudioFileError::Flac { path: path.to_path_buf(), source }),
        };
        let start = self.frame_start.load(Ordering::Relaxed);
        if self.checkpoints.last().is_none_or(|(t, _)| block.time() >= t + FLAC_CHECKPOINT_EVERY) {
            self.checkpoints.push((block.time(), start));
        }
        if block.channels() as usize != self.channels {
            return Err(AudioFileError::Corrupt { path: path.to_path_buf(), detail: "channel count changed mid-stream".into() });
        }
        let n = block.duration();
        out.reserve(n as usize * self.channels);
        for i in 0..n {
            for c in 0..self.channels as u32 {
                out.push(block.sample(c, i) as f32 * self.scale);
            }
        }
        self.block = block.into_buffer();
        Ok(true)
    }

    fn seek(&mut self, path: &Path, frame: u64) -> Result<u64, AudioFileError> {
        let idx = self.checkpoints.partition_point(|(t, _)| *t <= frame).saturating_sub(1);
        let (time, offset) = self.checkpoints.get(idx).copied().unwrap_or((0, self.data_start));
        let input = PosReader::open_at(path, offset, self.frame_start.clone())
            .map_err(|source| AudioFileError::Open { path: path.to_path_buf(), source })?;
        self.reader = FrameReader::new(input);
        Ok(time)
    }
}

fn flac_data_start(path: &Path) -> Result<u64, AudioFileError> {
    let io_err = |source| AudioFileError::Io { path: path.to_path_buf(), source };
    let mut file = File::open(path).map_err(|source| AudioFileError::Open { path: path.to_path_buf(), source })?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).map_err(io_err)?;
    if &magic != b"fLaC" {
        return Err(AudioFileError::Corrupt { path: path.to_path_buf(), detail: "missing fLaC marker".into() });
    }
    let mut offset = 4u64;
    loop {
        let mut header = [0u8; 4];
        file.read_exact(&mut header).map_err(io_err)?;
        let len = u32::from_be_bytes([0, header[1], header[2], header[3]]) as u64;
        offset += 4 + len;
        if header[0] & 0x80 != 0 {
            return Ok(offset);
        }
        file.seek(SeekFrom::Current(len as i64)).map_err(io_err)?;
    }
}

const FLAC_STREAMINFO_OFFSET: u64 = 8;
const FLAC_TOTAL_SAMPLES_OFFSET: u64 = FLAC_STREAMINFO_OFFSET + 13;

/// Truncates a FLAC file left by a crashed recorder to its last intact frame and
/// writes the sample count into STREAMINFO. Returns the number of frames kept.
pub fn repair_flac(path: &Path) -> Result<u64, AudioFileError> {
    let io_err = |source| AudioFileError::Io { path: path.to_path_buf(), source };
    let data_start = flac_data_start(path)?;
    let frame_start = Arc::new(AtomicU64::new(data_start));
    let input = PosReader::open_at(path, data_start, frame_start.clone())
        .map_err(|source| AudioFileError::Open { path: path.to_path_buf(), source })?;
    let mut reader = FrameReader::new(input);
    let mut buffer = Vec::new();
    let mut frames = 0u64;
    loop {
        frame_start.store(u64::MAX, Ordering::Relaxed);
        match reader.read_next_or_eof(buffer) {
            Ok(Some(block)) => {
                frames += block.duration() as u64;
                buffer = block.into_buffer();
            }
            Ok(None) | Err(_) => break,
        }
    }
    let good_end = match frame_start.load(Ordering::Relaxed) {
        u64::MAX => data_start,
        at => at,
    };
    let mut file = std::fs::OpenOptions::new().read(true).write(true).open(path).map_err(io_err)?;
    file.set_len(good_end).map_err(io_err)?;
    file.seek(SeekFrom::Start(FLAC_TOTAL_SAMPLES_OFFSET)).map_err(io_err)?;
    let mut head = [0u8; 5];
    file.read_exact(&mut head).map_err(io_err)?;
    head[0] = (head[0] & 0xF0) | ((frames >> 32) & 0x0F) as u8;
    head[1..].copy_from_slice(&(frames as u32).to_be_bytes());
    file.seek(SeekFrom::Start(FLAC_TOTAL_SAMPLES_OFFSET)).map_err(io_err)?;
    file.write_all(&head).map_err(io_err)?;
    file.sync_all().map_err(io_err)?;
    Ok(frames)
}

struct Counted<R> {
    inner: R,
    count: Arc<AtomicU64>,
}

impl<R: Read> Read for Counted<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

impl<R: Seek> Seek for Counted<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let at = self.inner.seek(pos)?;
        self.count.store(at, Ordering::Relaxed);
        Ok(at)
    }
}

struct Opus {
    reader: ogg::PacketReader<Counted<BufReader<File>>>,
    count: Arc<AtomicU64>,
    decoder: opus::Decoder,
    channels: usize,
    pre_skip: u64,
    total: u64,
    data_start: u64,
    checkpoints: Vec<(u64, u64)>,
    discard: u64,
    next_frame: u64,
    scratch: Vec<f32>,
}

fn open_ogg(path: &Path, count: &Arc<AtomicU64>) -> Result<ogg::PacketReader<Counted<BufReader<File>>>, AudioFileError> {
    let file = File::open(path).map_err(|source| AudioFileError::Open { path: path.to_path_buf(), source })?;
    count.store(0, Ordering::Relaxed);
    Ok(ogg::PacketReader::new(Counted { inner: BufReader::new(file), count: count.clone() }))
}

impl Opus {
    fn open(path: &Path) -> Result<(Self, u32, usize, u64), AudioFileError> {
        let ogg_err = |source| AudioFileError::Ogg { path: path.to_path_buf(), source };
        let corrupt = |detail: &str| AudioFileError::Corrupt { path: path.to_path_buf(), detail: detail.into() };
        let count = Arc::new(AtomicU64::new(0));
        let mut reader = open_ogg(path, &count)?;
        let head = reader.read_packet().map_err(ogg_err)?.ok_or_else(|| corrupt("empty stream"))?;
        if head.data.len() < 19 || &head.data[..8] != b"OpusHead" {
            return Err(corrupt("missing OpusHead"));
        }
        let channels = head.data[9] as usize;
        let pre_skip = u16::from_le_bytes([head.data[10], head.data[11]]) as u64;
        if !(1..=2).contains(&channels) || head.data[18] != 0 {
            return Err(AudioFileError::Unsupported { path: path.to_path_buf(), detail: "Opus channel mapping".into() });
        }
        let tags = reader.read_packet().map_err(ogg_err)?.ok_or_else(|| corrupt("missing OpusTags"))?;
        if !tags.data.starts_with(b"OpusTags") {
            return Err(corrupt("missing OpusTags"));
        }
        let data_start = count.load(Ordering::Relaxed);
        let mut checkpoints = vec![(0u64, data_start)];
        let mut last_granule = 0u64;
        while let Some(p) = reader.read_packet().map_err(ogg_err)? {
            if p.last_in_page() {
                last_granule = p.absgp_page();
                checkpoints.push((last_granule, count.load(Ordering::Relaxed)));
            }
        }
        checkpoints.pop();
        let total = last_granule.checked_sub(pre_skip).ok_or_else(|| corrupt("granule position below pre-skip"))?;
        let ch = if channels == 1 { opus::Channels::Mono } else { opus::Channels::Stereo };
        let decoder = opus::Decoder::new(OPUS_RATE, ch).map_err(|source| AudioFileError::Opus { path: path.to_path_buf(), source })?;
        let mut opus = Self {
            reader,
            count,
            decoder,
            channels,
            pre_skip,
            total,
            data_start,
            checkpoints,
            discard: pre_skip,
            next_frame: 0,
            scratch: vec![0.0; OPUS_MAX_FRAME * channels],
        };
        opus.seek(path, 0)?;
        Ok((opus, OPUS_RATE, channels, total))
    }

    fn decode(&mut self, path: &Path, out: &mut Vec<f32>) -> Result<bool, AudioFileError> {
        if self.next_frame >= self.total {
            return Ok(false);
        }
        let Some(packet) = self.reader.read_packet().map_err(|source| AudioFileError::Ogg { path: path.to_path_buf(), source })? else {
            return Ok(false);
        };
        let n = self
            .decoder
            .decode_float(&packet.data, &mut self.scratch, false)
            .map_err(|source| AudioFileError::Opus { path: path.to_path_buf(), source })?;
        let skip = (n as u64).min(self.discard);
        self.discard -= skip;
        let keep = (n as u64 - skip).min(self.total - self.next_frame);
        let from = skip as usize * self.channels;
        out.extend_from_slice(&self.scratch[from..from + keep as usize * self.channels]);
        self.next_frame += keep;
        Ok(true)
    }

    fn seek(&mut self, path: &Path, frame: u64) -> Result<u64, AudioFileError> {
        let want = frame + self.pre_skip;
        let idx = self.checkpoints.partition_point(|(g, _)| g + OPUS_PREROLL <= want).saturating_sub(1);
        let (granule, offset) = self.checkpoints.get(idx).copied().unwrap_or((0, self.data_start));
        self.reader
            .seek_bytes(SeekFrom::Start(offset))
            .map_err(|source| AudioFileError::Io { path: path.to_path_buf(), source })?;
        self.count.store(offset, Ordering::Relaxed);
        self.decoder.reset_state().map_err(|source| AudioFileError::Opus { path: path.to_path_buf(), source })?;
        if granule == 0 {
            self.discard = self.pre_skip;
            self.next_frame = 0;
        } else {
            self.discard = OPUS_PREROLL;
            self.next_frame = granule + OPUS_PREROLL - self.pre_skip;
        }
        Ok(self.next_frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn write_wav(path: &Path, rate: u32, channels: u16, frames: usize, f: impl Fn(usize, u16) -> f32) {
        let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..frames {
            for c in 0..channels {
                w.write_sample(f(i, c)).unwrap();
            }
        }
        w.finalize().unwrap();
    }

    #[test]
    fn wav_reads_seeks_and_downmixes() {
        let dir = std::env::temp_dir().join(format!("audio-recorder-af-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.wav");
        write_wav(&path, 8000, 2, 10_000, |i, c| i as f32 / 10_000.0 * if c == 0 { 1.0 } else { -1.0 });
        let mut r = SegmentReader::open(&path).unwrap();
        assert_eq!((r.rate(), r.channels(), r.frames()), (8000, 2, 10_000));
        r.seek(7_000).unwrap();
        let f = r.next_frame().unwrap().unwrap();
        assert!((f[0] - 0.7).abs() < 1e-6 && (f[1] + 0.7).abs() < 1e-6);
        let (mono, rate) = read_all_mono(&path).unwrap();
        assert_eq!((mono.len(), rate), (10_000, 8000));
        assert!(mono.iter().all(|v| v.abs() < 1e-6));
    }

    #[test]
    fn unknown_extension_is_unsupported() {
        assert!(matches!(SegmentReader::open(Path::new("x.mp3")), Err(AudioFileError::Unsupported { .. })));
    }
}

use std::{
    fs::File,
    io::{self, BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use flacenc::{
    bitsink::MemSink,
    component::{BitRepr, Stream},
    config::Encoder,
    error::Verify,
    source::{Context, Fill, FrameBuf},
};

pub const FLAC_BLOCK: usize = 4096;
pub const FLAC_HEADER_BYTES: usize = 42;
pub const FLAC_FLOAT_BITS: usize = 24;

#[derive(Debug, thiserror::Error)]
pub enum FlacWriteError {
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("FLAC encoding of {} failed: {detail}", path.display())]
    Encode { path: PathBuf, detail: String },
}

pub fn f32_to_24(sample: f32) -> i32 {
    let max = ((1i32 << (FLAC_FLOAT_BITS - 1)) - 1) as f32;
    (sample.clamp(-1.0, 1.0) * max).round() as i32
}

pub fn f32_to_16(sample: f32) -> i32 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16 as i32
}

pub struct FlacWriter {
    path: PathBuf,
    out: BufWriter<File>,
    config: flacenc::error::Verified<flacenc::config::Encoder>,
    stream: Stream,
    framebuf: (FrameBuf, Context),
    channels: usize,
    pending: Vec<i32>,
}

impl FlacWriter {
    pub fn create(path: &Path, rate: u32, channels: usize, bits: usize) -> Result<Self, FlacWriteError> {
        let encode = |detail: String| FlacWriteError::Encode { path: path.to_path_buf(), detail };
        let io = |source| FlacWriteError::Io { path: path.to_path_buf(), source };
        let config = Encoder::default().into_verified().map_err(|e| encode(format!("{e:?}")))?;
        let mut stream = Stream::new(rate as usize, channels, bits).map_err(|e| encode(format!("{e:?}")))?;
        stream.stream_info_mut().set_block_sizes(FLAC_BLOCK, FLAC_BLOCK).map_err(|e| encode(format!("{e:?}")))?;
        let placeholder = header(path, &stream)?;
        let mut out = BufWriter::new(File::create(path).map_err(io)?);
        out.write_all(&placeholder).and_then(|()| out.flush()).map_err(|source| FlacWriteError::Io { path: path.to_path_buf(), source })?;
        let framebuf = (
            FrameBuf::with_size(channels, FLAC_BLOCK).map_err(|e| encode(format!("{e:?}")))?,
            Context::new(bits, channels),
        );
        Ok(Self {
            path: path.to_path_buf(),
            out,
            config,
            stream,
            framebuf,
            channels,
            pending: Vec::with_capacity(FLAC_BLOCK * channels * 2),
        })
    }

    fn encode_error(&self, detail: impl std::fmt::Debug) -> FlacWriteError {
        FlacWriteError::Encode { path: self.path.clone(), detail: format!("{detail:?}") }
    }

    fn io_error(&self, source: io::Error) -> FlacWriteError {
        FlacWriteError::Io { path: self.path.clone(), source }
    }

    pub fn write(&mut self, samples: &[i32]) -> Result<(), FlacWriteError> {
        self.pending.extend_from_slice(samples);
        let block = FLAC_BLOCK * self.channels;
        let mut done = 0;
        while self.pending.len() - done >= block {
            self.encode_block(done, done + block)?;
            done += block;
        }
        self.pending.drain(..done);
        Ok(())
    }

    fn encode_block(&mut self, from: usize, to: usize) -> Result<(), FlacWriteError> {
        self.framebuf.fill_interleaved(&self.pending[from..to]).map_err(|e| self.encode_error(e))?;
        let number = self.framebuf.1.current_frame_number().ok_or_else(|| self.encode_error("no frame number"))?;
        let frame = flacenc::encode_fixed_size_frame(&self.config, &self.framebuf.0, number, self.stream.stream_info())
            .map_err(|e| self.encode_error(e))?;
        self.stream.stream_info_mut().update_frame_info(&frame);
        let mut sink = MemSink::<u8>::new();
        frame.write(&mut sink).map_err(|e| self.encode_error(e))?;
        self.out.write_all(sink.as_slice()).map_err(|e| self.io_error(e))
    }

    pub fn flush(&mut self) -> Result<(), FlacWriteError> {
        self.out.flush().map_err(|e| self.io_error(e))
    }

    pub fn finalize(mut self) -> Result<(), FlacWriteError> {
        if !self.pending.is_empty() {
            let whole = self.pending.len() / self.channels * self.channels;
            self.pending.truncate(whole);
            if whole > 0 {
                self.encode_block(0, whole)?;
            }
        }
        let total = self.framebuf.1.total_samples();
        let info = self.stream.stream_info_mut();
        if total > 0 {
            let block = total.min(FLAC_BLOCK);
            info.set_block_sizes(block, block).map_err(|e| FlacWriteError::Encode { path: self.path.clone(), detail: format!("{e:?}") })?;
        }
        info.set_md5_digest(&self.framebuf.1.md5_digest());
        info.set_total_samples(total);
        let final_header = header(&self.path, &self.stream)?;
        let path = self.path.clone();
        let mut file = self.out.into_inner().map_err(|e| FlacWriteError::Io { path: path.clone(), source: e.into_error() })?;
        let io = |source| FlacWriteError::Io { path: path.clone(), source };
        file.seek(SeekFrom::Start(0)).map_err(io)?;
        file.write_all(&final_header[..FLAC_HEADER_BYTES]).map_err(io)?;
        file.sync_all().map_err(io)
    }
}

fn header(path: &Path, stream: &Stream) -> Result<Vec<u8>, FlacWriteError> {
    let encode = |detail: String| FlacWriteError::Encode { path: path.to_path_buf(), detail };
    let mut sink = MemSink::<u8>::new();
    stream.write(&mut sink).map_err(|e| encode(format!("{e:?}")))?;
    let bytes = sink.into_inner();
    if bytes.len() < FLAC_HEADER_BYTES {
        return Err(encode(format!("unexpected header size {}", bytes.len())));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_file::SegmentReader;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("audio-recorder-flacw-{name}-{}.flac", std::process::id()))
    }

    fn tone(frames: usize, channels: usize) -> Vec<f32> {
        (0..frames * channels)
            .map(|i| 0.4 * (i as f32 * 0.01 * (1 + i % channels) as f32).sin())
            .collect()
    }

    #[test]
    fn i16_round_trips_bit_exact_across_odd_chunk_sizes() {
        let path = tmp("i16");
        let data = tone(10_000, 2);
        let ints: Vec<i32> = data.iter().map(|&s| f32_to_16(s)).collect();
        let mut w = FlacWriter::create(&path, 16_000, 2, 16).unwrap();
        for chunk in ints.chunks(1_001 * 2) {
            w.write(chunk).unwrap();
        }
        w.flush().unwrap();
        w.finalize().unwrap();
        let mut r = SegmentReader::open(&path).unwrap();
        assert_eq!(r.frames(), 10_000);
        let mut got = Vec::new();
        r.read(&mut got, usize::MAX).unwrap();
        let expected: Vec<f32> = ints.iter().map(|&v| v as f32 / 32768.0).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn f32_round_trips_within_24_bit_precision() {
        let path = tmp("f32");
        let data = tone(9_000, 1);
        let ints: Vec<i32> = data.iter().map(|&s| f32_to_24(s)).collect();
        let mut w = FlacWriter::create(&path, 48_000, 1, FLAC_FLOAT_BITS).unwrap();
        w.write(&ints).unwrap();
        w.finalize().unwrap();
        let mut r = SegmentReader::open(&path).unwrap();
        let mut got = Vec::new();
        r.read(&mut got, usize::MAX).unwrap();
        assert_eq!(got.len(), data.len());
        assert!(got.iter().zip(&data).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn repair_drops_a_torn_frame() {
        let path = tmp("torn");
        let mut w = FlacWriter::create(&path, 8_000, 1, 16).unwrap();
        w.write(&vec![7i32; FLAC_BLOCK * 3]).unwrap();
        w.flush().unwrap();
        std::mem::forget(w);
        let whole = std::fs::metadata(&path).unwrap().len();
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(&[0xFF, 0xF8, 0xC9, 0x18, 0x00, 0x01, 0x02]).unwrap();
        drop(f);
        assert_eq!(crate::audio_file::repair_flac(&path).unwrap(), FLAC_BLOCK as u64 * 3);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), whole);
        let mut r = SegmentReader::open(&path).unwrap();
        assert_eq!(r.frames(), FLAC_BLOCK as u64 * 3);
        let mut got = Vec::new();
        assert_eq!(r.read(&mut got, usize::MAX).unwrap(), FLAC_BLOCK * 3);
    }

    #[test]
    fn repair_of_a_file_without_frames_opens_empty() {
        let path = tmp("noframes");
        let w = FlacWriter::create(&path, 8_000, 1, 16).unwrap();
        std::mem::forget(w);
        assert_eq!(crate::audio_file::repair_flac(&path).unwrap(), 0);
        assert_eq!(SegmentReader::open(&path).unwrap().frames(), 0);
    }

    #[test]
    fn empty_file_finalizes() {
        let path = tmp("empty");
        FlacWriter::create(&path, 48_000, 1, 16).unwrap().finalize().unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len() as usize, FLAC_HEADER_BYTES);
    }
}

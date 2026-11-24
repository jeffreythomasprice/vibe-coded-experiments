use std::path::Path;

use audio_recorder_core::mixdown::{self, MixdownError};
use audio_recorder_schema::manifest::Segment;

pub const RATE: u32 = 16_000;
const GAP_SAMPLES: usize = 8_000;
const SILENT_RMS: f32 = 1e-4;
const SINC_ZEROS: f64 = 12.0;

#[derive(Debug, Clone)]
pub struct SegmentSpan {
    pub file: String,
    pub start_sample: usize,
    pub len: usize,
    pub offset_ns: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceAudio {
    pub samples: Vec<f32>,
    pub spans: Vec<SegmentSpan>,
}

impl DeviceAudio {
    pub fn load(session_dir: &Path, segments: &[&Segment]) -> Result<Self, MixdownError> {
        let mut audio = Self::default();
        let mut ordered: Vec<&&Segment> = segments.iter().collect();
        ordered.sort_by_key(|s| s.start_offset_ns);
        for seg in ordered {
            let path = session_dir.join(&seg.file);
            if !path.is_file() {
                tracing::warn!("skipping missing segment {}", path.display());
                continue;
            }
            let (mono, rate) = mixdown::read_segment_mono(&path)?;
            let resampled = resample(&mono, rate, RATE);
            if !audio.samples.is_empty() {
                audio.samples.resize(audio.samples.len() + GAP_SAMPLES, 0.0);
            }
            audio.spans.push(SegmentSpan {
                file: seg.file.clone(),
                start_sample: audio.samples.len(),
                len: resampled.len(),
                offset_ns: seg.start_offset_ns,
            });
            audio.samples.extend(resampled);
        }
        Ok(audio)
    }

    pub fn is_silent(&self) -> bool {
        if self.samples.is_empty() {
            return true;
        }
        let power = self.samples.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / self.samples.len() as f64;
        (power.sqrt() as f32) < SILENT_RMS
    }

    pub fn locate(&self, sample: usize) -> Option<(u64, &str)> {
        let idx = self.spans.partition_point(|s| s.start_sample <= sample).checked_sub(1)?;
        let span = &self.spans[idx];
        let within = sample.saturating_sub(span.start_sample).min(span.len);
        Some((span.offset_ns + (within as u64 * 1_000_000_000) / RATE as u64, span.file.as_str()))
    }
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

struct Filters {
    step: usize,
    phases: usize,
    taps: Vec<(i64, Vec<f32>)>,
}

impl Filters {
    fn new(from: u32, to: u32) -> Self {
        let g = gcd(from as u64, to as u64);
        let step = (from as u64 / g) as usize;
        let phases = (to as u64 / g) as usize;
        let ratio = from as f64 / to as f64;
        let cutoff = (1.0 / ratio).min(1.0);
        let half = (SINC_ZEROS / cutoff).ceil() as i64;

        let taps = (0..phases)
            .map(|p| {
                let pos = p as f64 * ratio;
                let base = pos.floor() as i64;
                let frac = pos - base as f64;
                let mut taps: Vec<f64> = (base - half + 1..=base + half)
                    .map(|i| {
                        let x = i as f64 - base as f64 - frac;
                        let arg = std::f64::consts::PI * cutoff * x;
                        let sinc = if arg.abs() < 1e-12 { 1.0 } else { arg.sin() / arg };
                        let w = (x / half as f64).abs();
                        let window = if w >= 1.0 { 0.0 } else { 0.5 * (1.0 + (std::f64::consts::PI * w).cos()) };
                        sinc * window
                    })
                    .collect();
                let sum: f64 = taps.iter().sum();
                if sum.abs() > 1e-12 {
                    taps.iter_mut().for_each(|t| *t /= sum);
                }
                (base - half + 1, taps.into_iter().map(|t| t as f32).collect())
            })
            .collect();
        Self { step, phases, taps }
    }

    fn start(&self, n: usize) -> (i64, &[f32]) {
        let (first, taps) = &self.taps[n % self.phases];
        ((n / self.phases * self.step) as i64 + first, taps)
    }

    fn sample(&self, input: &[f32], base: i64, n: usize) -> f32 {
        let (start, taps) = self.start(n);
        let start = start - base;
        if start >= 0 && start as usize + taps.len() <= input.len() {
            return input[start as usize..start as usize + taps.len()].iter().zip(taps).map(|(s, t)| s * t).sum();
        }
        let mut acc = 0f32;
        for (k, t) in taps.iter().enumerate() {
            let idx = start + k as i64;
            if idx >= 0 && (idx as usize) < input.len() {
                acc += input[idx as usize] * t;
            }
        }
        acc
    }
}

pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let filters = Filters::new(from, to);
    let out_len = (input.len() as u64 * to as u64 / from as u64) as usize;
    (0..out_len).map(|n| filters.sample(input, 0, n)).collect()
}

/// Resamples a stream chunk by chunk; output matches `resample` over the concatenated input except for
/// the last few milliseconds, which `finish` pads out with silence.
pub struct StreamResampler {
    passthrough: bool,
    filters: Filters,
    buf: Vec<f32>,
    base: i64,
    next: usize,
}

impl StreamResampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self { passthrough: from == to, filters: Filters::new(from, to), buf: Vec::new(), base: 0, next: 0 }
    }

    pub fn push(&mut self, input: &[f32]) -> Vec<f32> {
        if self.passthrough {
            return input.to_vec();
        }
        self.buf.extend_from_slice(input);
        let end = self.base + self.buf.len() as i64;
        let mut out = Vec::new();
        loop {
            let (start, taps) = self.filters.start(self.next);
            if start + taps.len() as i64 > end {
                break;
            }
            out.push(self.filters.sample(&self.buf, self.base, self.next));
            self.next += 1;
        }
        let keep_from = self.filters.start(self.next).0.max(self.base);
        let drop = (keep_from - self.base) as usize;
        self.buf.drain(..drop.min(self.buf.len()));
        self.base = keep_from;
        out
    }

    pub fn finish(&mut self) -> Vec<f32> {
        if self.passthrough {
            return Vec::new();
        }
        let pad = self.filters.taps.iter().map(|(_, t)| t.len()).max().unwrap_or(0);
        self.push(&vec![0.0; pad])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, rate: u32, secs: f64) -> Vec<f32> {
        (0..(rate as f64 * secs) as usize)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin() as f32 * 0.5)
            .collect()
    }

    fn rms(s: &[f32]) -> f32 {
        (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt()
    }

    #[test]
    fn passband_survives_and_stopband_is_removed() {
        for rate in [48_000, 44_100] {
            let kept = resample(&sine(1000.0, rate, 1.0), rate, RATE);
            assert_eq!(kept.len(), 16_000);
            assert!((rms(&kept[2000..14000]) - 0.3535).abs() < 0.02, "{rate}");
            let aliased = resample(&sine(10_000.0, rate, 1.0), rate, RATE);
            assert!(rms(&aliased[2000..14000]) < 0.01, "{rate}");
        }
    }

    #[test]
    fn streaming_matches_one_shot() {
        for rate in [48_000, 44_100] {
            let input = sine(440.0, rate, 1.0);
            let whole = resample(&input, rate, RATE);
            let mut stream = StreamResampler::new(rate, RATE);
            let mut out = Vec::new();
            for chunk in input.chunks(1021) {
                out.extend(stream.push(chunk));
            }
            let n = whole.len() - 200;
            assert!(out.len() >= n, "{rate}: {} < {n}", out.len());
            for i in 0..n {
                assert!((out[i] - whole[i]).abs() < 1e-5, "{rate} sample {i}");
            }
            assert!(stream.finish().len() > 0);
        }
    }

    #[test]
    fn locate_maps_samples_back_to_segment_time() {
        let audio = DeviceAudio {
            samples: vec![0.0; 100],
            spans: vec![
                SegmentSpan { file: "a".into(), start_sample: 0, len: 16_000, offset_ns: 5_000_000_000 },
                SegmentSpan { file: "b".into(), start_sample: 24_000, len: 16_000, offset_ns: 60_000_000_000 },
            ],
        };
        assert_eq!(audio.locate(8_000), Some((5_500_000_000, "a")));
        assert_eq!(audio.locate(24_000 + 16_000), Some((61_000_000_000, "b")));
        assert_eq!(audio.locate(20_000), Some((6_000_000_000, "a")));
    }
}

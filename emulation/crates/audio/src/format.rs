//! Sample-format helpers that don't touch the audio device: streaming rate
//! conversion for interleaved stereo `f32`. Pure and unit-tested — the analog of
//! `video::format`'s pixel conversion.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SampleFormatError {
    #[error("source and destination sample rates must both be non-zero")]
    ZeroRate,
}

/// A streaming linear resampler for interleaved stereo `f32` frames.
///
/// It carries the fractional phase and the previous input frame across calls, so
/// feeding it successive chunks produces a seamless output stream (no glitch at
/// chunk boundaries). Each output frame is a linear interpolation between two
/// adjacent input frames.
#[derive(Debug, Clone)]
pub struct Resampler {
    /// Source frames to advance per output frame (`src_rate / dst_rate`).
    step: f64,
    /// Fractional position in `[0, 1)` between `prev` and the next input frame.
    phase: f64,
    /// The previous input frame, held for interpolation across chunk boundaries.
    prev: (f32, f32),
}

impl Resampler {
    pub fn new(src_rate: u32, dst_rate: u32) -> Result<Resampler, SampleFormatError> {
        if src_rate == 0 || dst_rate == 0 {
            return Err(SampleFormatError::ZeroRate);
        }
        Ok(Resampler {
            step: src_rate as f64 / dst_rate as f64,
            phase: 0.0,
            prev: (0.0, 0.0),
        })
    }

    /// Resample `input` (interleaved stereo `f32`) to the destination rate,
    /// appending the interleaved output frames to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        for frame in input.chunks_exact(2) {
            let cur = (frame[0], frame[1]);
            while self.phase < 1.0 {
                let t = self.phase as f32;
                out.push(lerp(self.prev.0, cur.0, t));
                out.push(lerp(self.prev.1, cur.1, t));
                self.phase += self.step;
            }
            self.phase -= 1.0;
            self.prev = cur;
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_rate_is_an_error() {
        assert!(Resampler::new(0, 48_000).is_err());
        assert!(Resampler::new(48_000, 0).is_err());
    }

    #[test]
    fn doubling_the_rate_yields_twice_as_many_frames() {
        let mut r = Resampler::new(1, 2).unwrap();
        let mut out = Vec::new();
        // Four source frames -> about eight output frames.
        r.process(&[1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0], &mut out);
        assert_eq!(out.len(), 16, "8 stereo frames = 16 interleaved samples");
    }

    #[test]
    fn halving_the_rate_yields_half_as_many_frames() {
        let mut r = Resampler::new(2, 1).unwrap();
        let mut out = Vec::new();
        // Eight source frames -> about four output frames.
        let input: Vec<f32> = (0..16).map(|i| i as f32).collect();
        r.process(&input, &mut out);
        assert_eq!(out.len(), 8, "4 stereo frames = 8 interleaved samples");
    }

    #[test]
    fn equal_rates_pass_frames_through_with_one_frame_delay() {
        let mut r = Resampler::new(48_000, 48_000).unwrap();
        let mut out = Vec::new();
        r.process(&[0.5, 0.25, -0.5, -0.25], &mut out);
        // Delayed by one frame: the first output is the initial (0,0) prev.
        assert_eq!(out, vec![0.0, 0.0, 0.5, 0.25]);
    }

    #[test]
    fn chunked_processing_matches_a_single_call() {
        let input: Vec<f32> = (0..40).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut whole = Vec::new();
        Resampler::new(3, 7).unwrap().process(&input, &mut whole);

        let mut chunked = Vec::new();
        let mut r = Resampler::new(3, 7).unwrap();
        r.process(&input[..20], &mut chunked);
        r.process(&input[20..], &mut chunked);

        assert_eq!(whole, chunked, "phase carries across chunk boundaries");
    }

    #[test]
    fn interpolates_between_source_frames_when_upsampling() {
        let mut r = Resampler::new(1, 2).unwrap();
        let mut out = Vec::new();
        r.process(&[0.0, 0.0, 1.0, 0.0], &mut out);
        // prev starts at 0; frame0 = (0,0), frame1 = (1,0). Left channel outputs:
        // lerp(0,0,0)=0, lerp(0,0,.5)=0, lerp(0,1,0)=0, lerp(0,1,.5)=0.5.
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert_eq!(left, vec![0.0, 0.0, 0.0, 0.5]);
    }
}

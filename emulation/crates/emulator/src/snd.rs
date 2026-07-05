//! Audio host: the desktop shell's owner of the live audio output device.
//!
//! [`Snd`] is the audio analog of [`Gfx`](crate::gfx::Gfx). It wraps the
//! `audio` crate's [`CpalAudioOutput`], opening a real output stream from the
//! loaded [`AudioSettings`]. [`Snd::submit_frame`] is the seam a future run loop
//! calls each frame with `SystemBus::take_audio_samples`; like
//! [`Gfx::update_frame`](crate::gfx::Gfx::update_frame) it is dead for now,
//! since there is no top-level CPU+PPU+APU run loop yet, so the stream simply
//! plays silence.

use audio::{AudioError, AudioOutput, CpalAudioOutput};
use common::AudioSettings;
use gameboy::APU_SAMPLE_RATE;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SndError {
    #[error("failed to initialize audio output: {0}")]
    Audio(#[from] AudioError),
}

/// Owns the live audio output device. Constructed from settings; the stream runs
/// for the lifetime of this value.
pub struct Snd {
    output: CpalAudioOutput,
}

impl Snd {
    /// Open the default output device at the configured volume. The APU emits
    /// samples at [`APU_SAMPLE_RATE`], which the output resamples to the device
    /// rate.
    pub fn new(settings: &AudioSettings) -> Result<Snd, SndError> {
        let output = CpalAudioOutput::new(APU_SAMPLE_RATE, settings.volume)?;
        tracing::info!(volume = settings.volume, "audio output initialized");
        Ok(Snd { output })
    }

    /// Feed a frame's worth of interleaved-stereo APU samples to the device.
    ///
    /// Driven by the run loop with `SystemBus::take_audio_samples` each frame the
    /// core produces.
    pub fn submit_frame(&mut self, samples: &[f32]) -> Result<(), SndError> {
        self.output.submit(samples)?;
        Ok(())
    }

    /// Update the master output gain (`0.0..=1.0`) at runtime.
    #[allow(dead_code)]
    pub fn set_volume(&mut self, volume: f32) {
        self.output.set_volume(volume);
    }
}

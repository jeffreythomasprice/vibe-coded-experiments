//! Generic real-device audio output for emulator cores.
//!
//! [`AudioOutput`] is the system-agnostic trait a UI shell drives to play the
//! audio an emulator core generates. [`CpalAudioOutput`] is the first (and, for
//! now, only) implementation: it opens a [cpal](https://docs.rs/cpal) output
//! stream, resampling submitted samples to the device rate through a shared ring
//! buffer. Keeping cpal behind this trait mirrors how the `video` crate keeps
//! wgpu behind `VideoOutput`, so a headless build can swap in a no-op sink
//! without touching the core.
//!
//! The trait covers only *playback*: how the samples are produced (the Game Boy
//! APU) is the caller's concern, just as `VideoOutput` covers only drawing and
//! leaves pixel production to the concrete type.

mod format;
mod stream;

pub use format::{Resampler, SampleFormatError};
pub use stream::CpalAudioOutput;

/// Any sink that can play a stream of interleaved stereo `f32` samples produced
/// by an emulator core.
pub trait AudioOutput {
    /// Submit interleaved stereo samples (`L, R, L, R, …`, each in
    /// `[-1.0, 1.0]`) at the source rate the sink was created with. The sink
    /// resamples to the output device rate.
    fn submit(&mut self, samples: &[f32]) -> Result<(), AudioError>;
}

/// Errors from opening or driving an audio output device.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no audio output device is available")]
    NoDevice,
    #[error("the output device reports an unsupported sample format: {0:?}")]
    UnsupportedFormat(cpal::SampleFormat),
    #[error("failed to query the default output config: {0}")]
    DefaultConfig(#[from] cpal::DefaultStreamConfigError),
    #[error("failed to build the output stream: {0}")]
    BuildStream(#[from] cpal::BuildStreamError),
    #[error("failed to start the output stream: {0}")]
    PlayStream(#[from] cpal::PlayStreamError),
    #[error("invalid sample rate: {0}")]
    Format(#[from] SampleFormatError),
}

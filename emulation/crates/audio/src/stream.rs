//! The cpal-backed [`AudioOutput`] implementation. This is the one module that
//! actually touches the audio device library, keeping cpal out of the rest of
//! the workspace (the audio analog of `video::pixel_buffer`).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::format::Resampler;
use crate::{AudioError, AudioOutput};

/// Shared, device-rate interleaved-stereo sample queue between the submitting
/// thread and the cpal callback thread.
type SharedRing = Arc<Mutex<VecDeque<f32>>>;

/// An audio output backed by the default system output device via cpal.
///
/// [`CpalAudioOutput::new`] opens the default device, starts a stream that
/// drains an internal ring buffer (playing silence when it is empty), and holds
/// the stream alive for the lifetime of this value. [`submit`](AudioOutput::submit)
/// scales samples by the current volume, resamples them from the source rate to
/// the device rate, and pushes them into the ring.
pub struct CpalAudioOutput {
    /// Held only to keep the stream alive; dropping it stops playback.
    _stream: cpal::Stream,
    ring: SharedRing,
    resampler: Resampler,
    volume: f32,
    /// Ring-buffer cap (in samples) that bounds output latency: once reached,
    /// the oldest samples are dropped as new ones arrive.
    max_samples: usize,
}

impl CpalAudioOutput {
    /// Open the default output device and start playing. `source_rate` is the
    /// rate of the samples that will later be passed to [`submit`]
    /// (`gameboy::APU_SAMPLE_RATE`); `volume` is an initial gain in `[0.0, 1.0]`.
    pub fn new(source_rate: u32, volume: f32) -> Result<CpalAudioOutput, AudioError> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(AudioError::NoDevice)?;
        let supported = device.default_output_config()?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let device_rate = config.sample_rate.0;
        let channels = config.channels as usize;
        tracing::info!(
            device_rate,
            channels,
            ?sample_format,
            "opening audio output device"
        );

        let ring: SharedRing = Arc::new(Mutex::new(VecDeque::new()));
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, ring.clone(), channels),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, ring.clone(), channels),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, ring.clone(), channels),
            other => return Err(AudioError::UnsupportedFormat(other)),
        }?;
        stream.play()?;

        Ok(CpalAudioOutput {
            _stream: stream,
            ring,
            resampler: Resampler::new(source_rate, device_rate)?,
            volume: volume.clamp(0.0, 1.0),
            // Roughly a quarter second of stereo headroom keeps latency low.
            max_samples: device_rate as usize,
        })
    }

    /// Set the output gain in `[0.0, 1.0]`.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }
}

impl AudioOutput for CpalAudioOutput {
    fn submit(&mut self, samples: &[f32]) -> Result<(), AudioError> {
        let scaled: Vec<f32> = samples.iter().map(|s| s * self.volume).collect();
        let mut resampled = Vec::new();
        self.resampler.process(&scaled, &mut resampled);

        let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        for sample in resampled {
            if ring.len() >= self.max_samples {
                ring.pop_front();
            }
            ring.push_back(sample);
        }
        Ok(())
    }
}

/// Build an output stream whose callback drains `ring` into the device buffer,
/// converting each `f32` sample to the device's sample type `T` and writing the
/// stereo pair across the first two channels (silence on any extras).
fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    ring: SharedRing,
    channels: usize,
) -> Result<cpal::Stream, AudioError>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut ring = ring.lock().unwrap_or_else(|e| e.into_inner());
            for frame in data.chunks_mut(channels) {
                let (left, right) = pop_pair(&mut ring);
                for (channel, slot) in frame.iter_mut().enumerate() {
                    let value = match channel {
                        0 => left,
                        1 => right,
                        _ => 0.0,
                    };
                    *slot = T::from_sample(value);
                }
            }
        },
        |err| tracing::error!(%err, "audio output stream error"),
        None,
    )?;
    Ok(stream)
}

/// Pop one interleaved stereo frame from the ring, or `(0.0, 0.0)` (silence)
/// when it has run dry. The ring only ever holds whole frames, so pairs stay
/// aligned.
fn pop_pair(ring: &mut VecDeque<f32>) -> (f32, f32) {
    match (ring.pop_front(), ring.pop_front()) {
        (Some(left), Some(right)) => (left, right),
        _ => (0.0, 0.0),
    }
}

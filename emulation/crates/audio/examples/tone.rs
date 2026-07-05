//! Minimal standalone demo of the `audio` crate: open the default output device
//! and play a 440 Hz sine tone for two seconds, proving the crate works without
//! the emulator (the audio analog of `video`'s `greyscale` example).
//!
//! Run with: `cargo run -p audio --example tone`

use std::f32::consts::TAU;
use std::thread::sleep;
use std::time::Duration;

use audio::{AudioOutput, CpalAudioOutput};

/// The rate we generate our source samples at (arbitrary; the crate resamples to
/// the device rate).
const SOURCE_RATE: u32 = 48_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let mut output = CpalAudioOutput::new(SOURCE_RATE, 0.2)?;

    // Generate one second of a 440 Hz sine as interleaved stereo, submit it in
    // small chunks, then let it play out.
    let freq = 440.0;
    let mut samples = Vec::new();
    for i in 0..SOURCE_RATE {
        let t = i as f32 / SOURCE_RATE as f32;
        let s = (freq * t * TAU).sin();
        samples.push(s);
        samples.push(s);
    }

    for chunk in samples.chunks(2 * 1024) {
        output.submit(chunk)?;
        sleep(Duration::from_millis(20));
    }

    sleep(Duration::from_secs(1));
    Ok(())
}

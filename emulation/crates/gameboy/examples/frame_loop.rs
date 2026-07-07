//! Headless frame-loop profiler: run a ROM flat-out for N frames under a
//! signal-based sampling profiler and write a flamegraph SVG. This is the
//! headless analog of the desktop shell's run loop (just `Emulator::run_frame`
//! back-to-back, no pacing, windowing, or audio device), so it isolates the
//! cost of emulation itself — which is what caps "as fast as possible".
//!
//! Usage (build with debug symbols so the flamegraph symbolicates):
//!   CARGO_PROFILE_RELEASE_DEBUG=true cargo run --release -p gameboy \
//!       --example frame_loop -- <rom> [frames] [out.svg]

use std::error::Error;
use std::hint::black_box;
use std::time::Instant;

use gameboy::{Cartridge, Emulator};

/// DMG refresh rate, to report the measured speed as a multiple of real time.
const NATIVE_HZ: f64 = 59.7275;

/// Frames to run before sampling starts, so we profile steady-state gameplay
/// rather than boot / logo scroll.
const WARMUP_FRAMES: u32 = 600;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let rom = args
        .next()
        .ok_or("usage: frame_loop <rom> [frames] [out.svg]")?;
    let frames: u32 = match args.next() {
        Some(s) => s.parse()?,
        None => 8_000,
    };
    let out = args.next().unwrap_or_else(|| "flamegraph.svg".to_string());

    let bytes = std::fs::read(&rom)?;
    let cartridge = Cartridge::from_bytes(&bytes)?;
    let mut emu = Emulator::new(cartridge);

    for _ in 0..WARMUP_FRAMES {
        emu.run_frame();
        emu.take_audio_samples();
    }

    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(1000)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()?;

    let start = Instant::now();
    let mut acc = 0u64;
    let mut window_start = start;
    for i in 0..frames {
        emu.run_frame();
        // Touch the per-frame outputs so the optimizer can't elide the work a
        // real host consumes (audio drain + framebuffer read).
        acc = acc.wrapping_add(emu.take_audio_samples().len() as u64);
        acc = acc.wrapping_add(emu.framebuffer()[0] as u64);
        // Per-window fps, to expose halt-inflation (a ROM that stops executing
        // instructions gets much cheaper per frame — not representative of a game).
        if (i + 1) % 1000 == 0 {
            let w = window_start.elapsed().as_secs_f64();
            eprintln!(
                "  frames {:>6}..{:<6} {:>6.0} fps ({:>5.1}x)",
                i + 1 - 1000,
                i + 1,
                1000.0 / w,
                1000.0 / w / NATIVE_HZ
            );
            window_start = Instant::now();
        }
    }
    let elapsed = start.elapsed();
    black_box(acc);

    let fps = frames as f64 / elapsed.as_secs_f64();
    eprintln!(
        "ran {frames} frames in {elapsed:?} = {fps:.0} fps = {:.2}x real time",
        fps / NATIVE_HZ
    );

    let report = guard.report().build()?;
    let file = std::fs::File::create(&out)?;
    report.flamegraph(file)?;
    eprintln!("wrote flamegraph to {out}");
    Ok(())
}

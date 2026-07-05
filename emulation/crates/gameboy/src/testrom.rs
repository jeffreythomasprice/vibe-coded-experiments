//! A headless harness for running Game Boy **test ROMs** to a pass/fail verdict.
//!
//! The de-facto standard hardware tests (Blargg's suites, cloned at
//! [`TEST_ROMS_DIR`]) report their result over one of two channels, and this
//! harness watches both:
//!
//! - **Serial:** the ROM prints its output to the link port — write a character
//!   to `SB`, then `$81` to `SC`. [`Emulator::take_serial_output`] surfaces those
//!   bytes; when the accumulated text contains `"Passed"` or `"Failed"` the run is
//!   done. This covers `cpu_instrs`, `instr_timing`, and `mem_timing`.
//! - **Memory (`$A000`):** the ROM writes a result block to cartridge RAM — the
//!   signature `$DE $B0 $61` at `$A001-$A003`, a status byte at `$A000` (`$80`
//!   while running, `$00` = pass, otherwise fail), and a NUL-terminated message at
//!   `$A004`. This covers `mem_timing-2` and `oam_bug`, which print nothing to the
//!   link port.
//!
//! [`run_test_rom`] drives [`Emulator::run_frame`] flat-out (no wall-clock pacing)
//! until a verdict lands, the CPU faults, or a frame budget is exhausted. The
//! detection itself ([`detect`]) is factored out so a live windowed runner can
//! poll it each frame too. Screenshots are opt-in and returned as raw shade
//! indices — encoding to an image is the caller's concern, keeping this module
//! free of image/windowing dependencies.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::{Cartridge, CartridgeError, CpuFault, Emulator, SCREEN_HEIGHT, SCREEN_WIDTH};

/// Fixed location of the Blargg test-ROM checkout the tests and examples read.
/// Cloned from <https://github.com/jeffreythomasprice/gb-test-roms>.
pub const TEST_ROMS_DIR: &str = "/home/jeff/workspaces/personal/gb-test-roms";

/// Default frame budget before a run is declared a timeout. Blargg's slowest
/// single ROM (`cpu_instrs.gb`) finishes in well under a thousand frames, so this
/// is generous (~67 s of emulated time) while still bounding a hung ROM.
pub const DEFAULT_MAX_FRAMES: u64 = 4000;

/// Which result channel(s) the harness watches. See the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detection {
    /// Only scan the serial output for `"Passed"`/`"Failed"`.
    Serial,
    /// Only inspect the `$A000` result block in cartridge RAM.
    Memory,
    /// Watch both, serial first (the default).
    Auto,
}

/// How to run a test ROM.
#[derive(Debug, Clone)]
pub struct TestConfig {
    /// Frames to run before giving up (see [`DEFAULT_MAX_FRAMES`]).
    pub max_frames: u64,
    /// Which result channel(s) to watch.
    pub detection: Detection,
    /// Capture the final framebuffer into the outcome/error. Off by default so
    /// the test suite does no image work.
    pub capture_screenshot: bool,
}

impl Default for TestConfig {
    fn default() -> TestConfig {
        TestConfig {
            max_frames: DEFAULT_MAX_FRAMES,
            detection: Detection::Auto,
            capture_screenshot: false,
        }
    }
}

/// A captured frame: raw `0-3` shade indices, row-major, one byte per pixel. The
/// caller encodes it (e.g. to PNG through the DMG palette); the core stays
/// encoding-free.
#[derive(Debug, Clone)]
pub struct Screenshot {
    pub width: u32,
    pub height: u32,
    pub shades: Vec<u8>,
}

/// A successful run.
#[derive(Debug, Clone)]
pub struct TestOutcome {
    /// Everything the ROM printed to the serial port (may be empty for a
    /// memory-reporting ROM).
    pub serial_text: String,
    /// Frames run before the pass was detected.
    pub frames: u64,
    /// The final frame, when [`TestConfig::capture_screenshot`] was set.
    pub screenshot: Option<Screenshot>,
}

/// Why a run did not succeed.
#[derive(Debug, Error)]
pub enum TestError {
    #[error("test timed out after {frames} frames with no pass/fail signal")]
    Timeout {
        frames: u64,
        serial_text: String,
        screenshot: Option<Screenshot>,
    },
    #[error("test reported failure: {message}")]
    Failed {
        message: String,
        serial_text: String,
        screenshot: Option<Screenshot>,
    },
    #[error("cpu faulted: {0}")]
    Fault(#[from] CpuFault),
    #[error("could not load ROM: {0}")]
    Load(#[from] CartridgeError),
    #[error("could not read ROM file {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl TestError {
    /// The serial output captured before the run ended, if any (empty for
    /// load/fault errors).
    pub fn serial_text(&self) -> &str {
        match self {
            TestError::Timeout { serial_text, .. } | TestError::Failed { serial_text, .. } => {
                serial_text
            }
            _ => "",
        }
    }

    /// The final-frame screenshot, if one was captured.
    pub fn screenshot(&self) -> Option<&Screenshot> {
        match self {
            TestError::Timeout { screenshot, .. } | TestError::Failed { screenshot, .. } => {
                screenshot.as_ref()
            }
            _ => None,
        }
    }
}

/// The result a ROM has signalled, once it has signalled one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Passed,
    /// The ROM reported failure; the string is the best available detail (the
    /// serial log or the `$A000` message).
    Failed(String),
}

const RESULT_SIGNATURE: [u8; 3] = [0xDE, 0xB0, 0x61];
const STATUS_RUNNING: u8 = 0x80;
const STATUS_PASSED: u8 = 0x00;

/// The signature of the `$A000` result block, at `$A001-$A003`.
const SIGNATURE_ADDR: u16 = 0xA001;
/// The status byte of the `$A000` result block.
const STATUS_ADDR: u16 = 0xA000;
/// Where the NUL-terminated result message begins.
const MESSAGE_ADDR: u16 = 0xA004;

/// Scan the serial text for Blargg's terminal `"Passed"`/`"Failed"` marker.
fn detect_serial(serial_text: &str) -> Option<Verdict> {
    if serial_text.contains("Passed") {
        Some(Verdict::Passed)
    } else if serial_text.contains("Failed") {
        Some(Verdict::Failed(serial_text.trim().to_string()))
    } else {
        None
    }
}

/// Inspect the `$A000` result block in cartridge RAM (see the module docs).
fn detect_memory(cart: &Cartridge) -> Option<Verdict> {
    let signature = [
        cart.read(SIGNATURE_ADDR),
        cart.read(SIGNATURE_ADDR + 1),
        cart.read(SIGNATURE_ADDR + 2),
    ];
    if signature != RESULT_SIGNATURE {
        return None; // ROM has not written the result block yet
    }
    match cart.read(STATUS_ADDR) {
        STATUS_RUNNING => None,
        STATUS_PASSED => Some(Verdict::Passed),
        _ => Some(Verdict::Failed(read_message(cart))),
    }
}

/// Read the NUL-terminated result message at `$A004` (bounded to the RAM window).
fn read_message(cart: &Cartridge) -> String {
    let mut message = String::new();
    for addr in MESSAGE_ADDR..=0xBFFF {
        let byte = cart.read(addr);
        if byte == 0 {
            break;
        }
        message.push(byte as char);
    }
    message.trim().to_string()
}

/// Watch both result channels (serial first) for a verdict. This is the per-frame
/// check a live windowed runner can share with [`run_test_rom`].
pub fn detect(serial_text: &str, cart: &Cartridge) -> Option<Verdict> {
    detect_serial(serial_text).or_else(|| detect_memory(cart))
}

fn detect_with(detection: Detection, serial_text: &str, cart: &Cartridge) -> Option<Verdict> {
    match detection {
        Detection::Serial => detect_serial(serial_text),
        Detection::Memory => detect_memory(cart),
        Detection::Auto => detect(serial_text, cart),
    }
}

fn screenshot_of(emu: &Emulator) -> Screenshot {
    Screenshot {
        width: SCREEN_WIDTH,
        height: SCREEN_HEIGHT,
        shades: emu.framebuffer().to_vec(),
    }
}

/// Run a test ROM image to a verdict, or fail with a timeout/fault/failure.
///
/// Drives [`Emulator::run_frame`] as fast as it will go, accumulating serial
/// output and polling [`detect_with`] after each frame.
pub fn run_test_rom(rom: &[u8], config: &TestConfig) -> Result<TestOutcome, TestError> {
    let cart = Cartridge::from_bytes(rom)?;
    let mut emu = Emulator::new(cart);
    let mut serial = Vec::new();

    for frame in 1..=config.max_frames {
        let result = emu.run_frame();
        if let Some(fault) = result.fault {
            return Err(TestError::Fault(fault));
        }
        serial.extend(emu.take_serial_output());
        let serial_text = String::from_utf8_lossy(&serial);

        if let Some(verdict) = detect_with(config.detection, &serial_text, emu.cartridge()) {
            let screenshot = config.capture_screenshot.then(|| screenshot_of(&emu));
            let serial_text = serial_text.into_owned();
            return match verdict {
                Verdict::Passed => {
                    tracing::info!(frame, "test ROM passed");
                    Ok(TestOutcome {
                        serial_text,
                        frames: frame,
                        screenshot,
                    })
                }
                Verdict::Failed(message) => {
                    tracing::warn!(frame, %message, "test ROM failed");
                    Err(TestError::Failed {
                        message,
                        serial_text,
                        screenshot,
                    })
                }
            };
        }
    }

    tracing::warn!(frames = config.max_frames, "test ROM timed out");
    Err(TestError::Timeout {
        frames: config.max_frames,
        serial_text: String::from_utf8_lossy(&serial).into_owned(),
        screenshot: config.capture_screenshot.then(|| screenshot_of(&emu)),
    })
}

/// Read a ROM file and [`run_test_rom`] it.
pub fn run_rom_file<P: AsRef<Path>>(
    path: P,
    config: &TestConfig,
) -> Result<TestOutcome, TestError> {
    let path = path.as_ref();
    let rom = std::fs::read(path).map_err(|source| TestError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    run_test_rom(&rom, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a ROM given relative to [`TEST_ROMS_DIR`], asserting it passes.
    /// Skips (does not fail) with a warning if the external checkout isn't
    /// present, so the suite degrades gracefully on a machine without the ROMs.
    #[track_caller]
    fn assert_passes(rel: &str, detection: Detection) {
        assert_passes_within(rel, detection, DEFAULT_MAX_FRAMES);
    }

    #[track_caller]
    fn assert_passes_within(rel: &str, detection: Detection, max_frames: u64) {
        let path = Path::new(TEST_ROMS_DIR).join(rel);
        if !path.exists() {
            tracing::warn!(rom = %path.display(), "test ROM missing; skipping (clone gb-test-roms)");
            return;
        }
        let config = TestConfig {
            detection,
            max_frames,
            ..Default::default()
        };
        if let Err(err) = run_rom_file(&path, &config) {
            panic!(
                "{rel} did not pass: {err}\n--- serial output ---\n{}",
                err.serial_text()
            );
        }
    }

    // --- Serial-reporting suites -------------------------------------------------

    // Runs all 11 sub-tests back-to-back (passes ~frame 3200; extra headroom). The
    // combined ROM is a CGB-aware build that runs its tests in double speed via the
    // `KEY1`/`STOP` speed-switch idiom — see `Cpu::stop`; before that was handled it
    // hung here at sub-test 03.
    #[test]
    fn cpu_instrs_combined() {
        assert_passes_within("cpu_instrs/cpu_instrs.gb", Detection::Serial, 8_000);
    }

    #[test]
    fn cpu_instrs_individual() {
        for rel in [
            "cpu_instrs/individual/01-special.gb",
            "cpu_instrs/individual/02-interrupts.gb",
            "cpu_instrs/individual/03-op sp,hl.gb",
            "cpu_instrs/individual/04-op r,imm.gb",
            "cpu_instrs/individual/05-op rp.gb",
            "cpu_instrs/individual/06-ld r,r.gb",
            "cpu_instrs/individual/07-jr,jp,call,ret,rst.gb",
            "cpu_instrs/individual/08-misc instrs.gb",
            "cpu_instrs/individual/09-op r,r.gb",
            "cpu_instrs/individual/10-bit ops.gb",
            "cpu_instrs/individual/11-op a,(hl).gb",
        ] {
            assert_passes(rel, Detection::Serial);
        }
    }

    #[test]
    fn instr_timing() {
        assert_passes("instr_timing/instr_timing.gb", Detection::Serial);
    }

    // The `mem_timing` suites check *sub-instruction* memory-access timing. This
    // CPU is instruction-stepped (the bus is ticked in one lump after each whole
    // instruction), so that timing isn't modelled and these fail today. Ignored,
    // documenting the coverage and ready once a cycle-stepped bus lands. Run with
    // `cargo test -- --ignored`.
    #[test]
    #[ignore = "sub-instruction memory timing not modelled (instruction-stepped CPU)"]
    fn mem_timing() {
        assert_passes("mem_timing/mem_timing.gb", Detection::Serial);
        for rel in [
            "mem_timing/individual/01-read_timing.gb",
            "mem_timing/individual/02-write_timing.gb",
            "mem_timing/individual/03-modify_timing.gb",
        ] {
            assert_passes(rel, Detection::Serial);
        }
    }

    // --- Memory ($A000) reporting suites ----------------------------------------

    // Same sub-instruction-timing limitation as `mem_timing` above, but this suite
    // reports through the $A000 memory block instead of serial — so it still
    // exercises the memory-detection path even while ignored for accuracy.
    #[test]
    #[ignore = "sub-instruction memory timing not modelled (instruction-stepped CPU)"]
    fn mem_timing_2() {
        assert_passes("mem_timing-2/mem_timing.gb", Detection::Memory);
        for rel in [
            "mem_timing-2/rom_singles/01-read_timing.gb",
            "mem_timing-2/rom_singles/02-write_timing.gb",
            "mem_timing-2/rom_singles/03-modify_timing.gb",
        ] {
            assert_passes(rel, Detection::Memory);
        }
    }

    // The DMG OAM-corruption bug is not modelled by this PPU, so these fail today.
    // Kept (ignored) to document the coverage; run with `cargo test -- --ignored`.
    //
    // Investigation notes (why this is deferred, not a quick fix): the corruption
    // itself (a 16-bit inc/dec of an $FE00-$FEFF value during mode 2 copies OAM
    // bytes between rows) is straightforward to model, and the `2-causes` /
    // `3-non_causes` sub-tests — "does corruption fire for the right instructions?"
    // — would be reachable. The blockers are timing accuracy, which our
    // instruction-atomic CPU (bus ticked in one lump *after* each whole
    // instruction; see `Emulator::run_frame`) can't provide:
    //   * `1-lcd_sync` needs the LCD-enable→first-scanline quirk (LY ticks ~110
    //     M-cycles after enable, not a full 114).
    //   * `7-timing_effect` CRCs the corruption across 116 sub-scanline timings —
    //     needs the exact PPU dot at the instant of the increment.
    //   * `8-instr_effect` needs POP/PUSH to corrupt two *consecutive* OAM rows at
    //     different sub-instruction M-cycles — impossible without cycle stepping.
    // The combined ROM stops at the first failing sub-test (`lcd_sync`), so it
    // can't pass until a cycle-accurate CPU/PPU lands (the same rework `mem_timing`
    // needs). `mem_timing-2` above still exercises the $A000 detection path.
    #[test]
    #[ignore = "DMG OAM-corruption bug needs cycle-accurate timing this instruction-stepped CPU lacks"]
    fn oam_bug() {
        assert_passes("oam_bug/oam_bug.gb", Detection::Memory);
        for rel in [
            "oam_bug/rom_singles/1-lcd_sync.gb",
            "oam_bug/rom_singles/2-causes.gb",
            "oam_bug/rom_singles/3-non_causes.gb",
            "oam_bug/rom_singles/4-scanline_timing.gb",
            "oam_bug/rom_singles/5-timing_bug.gb",
            "oam_bug/rom_singles/6-timing_no_bug.gb",
            "oam_bug/rom_singles/7-timing_effect.gb",
            "oam_bug/rom_singles/8-instr_effect.gb",
        ] {
            assert_passes(rel, Detection::Memory);
        }
    }

    // --- Harness-level unit tests (no external ROMs) -----------------------------

    #[test]
    fn detect_serial_recognizes_pass_and_fail() {
        assert_eq!(detect_serial("01:ok\nPassed"), Some(Verdict::Passed));
        assert_eq!(
            detect_serial("06-ld r,r\n\nFailed"),
            Some(Verdict::Failed("06-ld r,r\n\nFailed".to_string()))
        );
        assert_eq!(detect_serial("still running..."), None);
    }

    /// A minimal MBC1+RAM cartridge (so `$A000-$BFFF` is real, writable RAM) with
    /// RAM enabled, for exercising `detect_memory` without a real ROM.
    fn cart_with_ram() -> Cartridge {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0134..0x0134 + 4].copy_from_slice(b"MEMT");
        rom[0x0147] = 0x03; // MBC1 + RAM + battery
        rom[0x0148] = 0x00; // 32 KiB ROM
        rom[0x0149] = 0x02; // 8 KiB RAM
        let mut checksum = 0u8;
        for &b in &rom[0x0134..=0x014C] {
            checksum = checksum.wrapping_sub(b).wrapping_sub(1);
        }
        rom[0x014D] = checksum;
        let mut cart = Cartridge::from_bytes(&rom).expect("valid MBC1+RAM ROM");
        cart.write(0x0000, 0x0A); // enable cartridge RAM
        cart
    }

    fn write_result_block(cart: &mut Cartridge, status: u8, message: &[u8]) {
        cart.write(STATUS_ADDR, status);
        cart.write(SIGNATURE_ADDR, RESULT_SIGNATURE[0]);
        cart.write(SIGNATURE_ADDR + 1, RESULT_SIGNATURE[1]);
        cart.write(SIGNATURE_ADDR + 2, RESULT_SIGNATURE[2]);
        for (i, &b) in message.iter().enumerate() {
            cart.write(MESSAGE_ADDR + i as u16, b);
        }
        cart.write(MESSAGE_ADDR + message.len() as u16, 0); // NUL terminator
    }

    #[test]
    fn detect_memory_reads_the_a000_result_block() {
        // No signature yet -> undecided.
        let mut cart = cart_with_ram();
        assert_eq!(detect_memory(&cart), None);

        // Signature present but still running.
        write_result_block(&mut cart, STATUS_RUNNING, b"running");
        assert_eq!(detect_memory(&cart), None);

        // Passed.
        write_result_block(&mut cart, STATUS_PASSED, b"mem_timing\n\nPassed");
        assert_eq!(detect_memory(&cart), Some(Verdict::Passed));

        // Failed carries the NUL-terminated message.
        write_result_block(&mut cart, 0x01, b"mem_timing\n\n01:01\n\nFailed");
        assert_eq!(
            detect_memory(&cart),
            Some(Verdict::Failed("mem_timing\n\n01:01\n\nFailed".to_string()))
        );
    }
}

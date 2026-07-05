//! Game Boy (SM83) emulator core.
//!
//! This crate will eventually own the CPU, PPU, APU, timers, and memory map for
//! the original Game Boy, exposing them behind system-agnostic traits (video
//! output, input, persistence) that the UI shell in the `emulator` crate can
//! drive. Today it provides the cartridge loader ([`cartridge`]), the SM83 CPU
//! core ([`cpu`]), the system memory bus ([`memory`]) that wires the CPU to the
//! cartridge and internal RAM, and the timer/divider ([`timer`]).

pub mod apu;
pub mod cartridge;
pub mod cpu;
pub mod dma;
pub mod input;
pub mod joypad;
pub mod memory;
pub mod ppu;
pub mod serial;
pub mod testrom;
pub mod timer;

pub use apu::{Apu, APU_SAMPLE_RATE};
pub use cartridge::{Cartridge, CartridgeError, CartridgeHeader, CartridgeType, Mapper};
pub use cpu::{Bus, Cpu, CpuFault, Interrupt};
pub use dma::OamDma;
pub use input::GameboyButton;
pub use joypad::Joypad;
pub use memory::SystemBus;
pub use ppu::{Mode, Ppu, PpuStep};
pub use serial::Serial;
pub use timer::Timer;

/// Native resolution of the original Game Boy LCD, in pixels.
pub const SCREEN_WIDTH: u32 = 160;
/// Native resolution of the original Game Boy LCD, in pixels.
pub const SCREEN_HEIGHT: u32 = 144;

/// The DMG master clock in T-cycles per second (`2^22`).
pub const CLOCK_HZ: u32 = 4_194_304;
/// T-cycles in one full PPU frame: 154 scanlines of 456 dots. At [`CLOCK_HZ`]
/// this yields the DMG's ~59.7275 Hz refresh.
pub const T_CYCLES_PER_FRAME: u32 = 70_224;

use std::collections::HashSet;

/// The outcome of driving the machine for one frame.
#[derive(Debug, Clone, Default)]
pub struct FrameResult {
    /// Whether a full PPU frame completed (`false` if the CPU faulted or the
    /// safety cap was hit before a frame boundary).
    pub completed: bool,
    /// Set when the CPU locked on an illegal opcode during this frame.
    pub fault: Option<CpuFault>,
}

/// The top-level Game Boy: the SM83 CPU wired to the system memory bus.
///
/// This advances **emulated time only** — [`run_frame`](Emulator::run_frame)
/// runs the machine forward until the PPU completes a frame. Wall-clock pacing
/// (matching or scaling real hardware speed) is the host shell's concern, not
/// this type's.
#[derive(Debug)]
pub struct Emulator {
    cpu: Cpu,
    bus: SystemBus,
}

impl Emulator {
    /// Build a machine around a loaded cartridge, with the CPU in its post-boot
    /// DMG state (as if the boot ROM had already run).
    pub fn new(cartridge: Cartridge) -> Emulator {
        Emulator {
            cpu: Cpu::new(),
            bus: SystemBus::new(cartridge),
        }
    }

    /// Advance the machine by up to one frame's worth of emulated time
    /// ([`T_CYCLES_PER_FRAME`] T-cycles), returning early if the PPU completes a
    /// frame or the CPU faults.
    ///
    /// With the LCD on, exactly one V-Blank falls in any one-frame window, so
    /// this returns as soon as it lands (`completed: true`). With the LCD **off**
    /// no frame boundary ever comes — a legitimate state games use during setup —
    /// so the budget is spent in full and `completed` is `false`; this is normal,
    /// not an error. Either way emulated time advances by ~one frame, keeping the
    /// caller's pacing steady regardless of LCD state.
    ///
    /// `cpu.fault()` is checked every step so a locked CPU (illegal opcode)
    /// returns immediately rather than spinning the whole budget every call.
    pub fn run_frame(&mut self) -> FrameResult {
        let mut elapsed = 0u32;
        while elapsed < T_CYCLES_PER_FRAME {
            let t = self.cpu.step(&mut self.bus);
            self.bus.tick(t);
            elapsed += t;
            if let Some(fault) = self.cpu.fault() {
                return FrameResult {
                    completed: false,
                    fault: Some(fault),
                };
            }
            if self.bus.take_frame_ready() {
                return FrameResult {
                    completed: true,
                    fault: None,
                };
            }
        }
        FrameResult {
            completed: false,
            fault: None,
        }
    }

    /// Feed the currently-held buttons to the joypad for the coming frame.
    pub fn set_buttons(&mut self, held: &HashSet<GameboyButton>) {
        self.bus.set_buttons(held);
    }

    /// The most recently completed frame: `SCREEN_WIDTH * SCREEN_HEIGHT` shade
    /// indices (`0-3`, row-major).
    pub fn framebuffer(&self) -> &[u8] {
        self.bus.framebuffer()
    }

    /// Drain the APU's accumulated interleaved-stereo samples (`[L, R, ...]`).
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        self.bus.take_audio_samples()
    }

    /// Drain the bytes the game has transmitted over the serial port since the
    /// last call, in order. The [`testrom`] harness reads Blargg's serial output
    /// through this seam.
    pub fn take_serial_output(&mut self) -> Vec<u8> {
        self.bus.take_serial_output()
    }

    /// The loaded cartridge (for battery-save persistence).
    pub fn cartridge(&self) -> &Cartridge {
        self.bus.cartridge()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal 32 KiB ROM-only image with a valid header checksum, with `program`
    /// spliced in at the entry point (`0x0100`).
    fn build_rom(program: &[u8]) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0134..0x0134 + 4].copy_from_slice(b"ROMX");
        rom[0x0147] = 0x00; // ROM only
        let mut x = 0u8;
        for &b in &rom[0x0134..=0x014C] {
            x = x.wrapping_sub(b).wrapping_sub(1);
        }
        rom[0x014D] = x;
        rom[0x0100..0x0100 + program.len()].copy_from_slice(program);
        rom
    }

    #[test]
    fn run_frame_completes_after_enabling_the_lcd() {
        // LD A,0x80 / LDH (0x40),A (LCDC = on) / JR -2 (spin).
        let program = [0x3E, 0x80, 0xE0, 0x40, 0x18, 0xFE];
        let cart = Cartridge::from_bytes(&build_rom(&program)).unwrap();
        let mut emu = Emulator::new(cart);

        let result = emu.run_frame();
        assert!(result.completed, "a frame should complete once the LCD is on");
        assert!(result.fault.is_none());
        assert_eq!(emu.framebuffer().len(), (SCREEN_WIDTH * SCREEN_HEIGHT) as usize);
    }

    #[test]
    fn run_frame_reports_a_fault_on_an_illegal_opcode() {
        // 0xD3 is an undefined opcode; it locks the CPU.
        let cart = Cartridge::from_bytes(&build_rom(&[0xD3])).unwrap();
        let mut emu = Emulator::new(cart);

        let result = emu.run_frame();
        assert!(!result.completed);
        assert!(result.fault.is_some(), "an illegal opcode should surface a fault");
    }
}

//! The Sharp SM83 CPU core.
//!
//! This module owns everything about executing instructions: the [`Registers`]
//! and [`Flags`], the pure ALU primitives ([`alu`]), the base and `$CB` opcode
//! dispatch ([`execute`], [`cb`]), and the interrupt/`EI`/`HALT`/`STOP` state
//! machine driven by [`Cpu::step`]. Memory is reached only through the [`Bus`]
//! trait, so the CPU is fully exercised in unit tests against the flat-memory
//! double without any real hardware wired up yet.
//!
//! **Stepping model.** [`Cpu::step`] runs one whole instruction (or one
//! interrupt dispatch, or one idle cycle while halted) and returns the number of
//! **T-states** it consumed. This "instruction-stepped" model is enough for the
//! CPU test ROMs; a later "cycle-stepped" refinement would tick other subsystems
//! *during* each memory access, which is why [`Bus::read8`] already takes
//! `&mut self`.
//!
//! **What lives here vs. later.** `IME` and the whole dispatch sequence live on
//! the CPU because `HALT`, the halt bug, the `EI` delay, and `RETI` are defined
//! in terms of them and are meaningless without dispatch. The per-source enable
//! (`IE` at `0xFFFF`) and request (`IF` at `0xFF0F`) bits stay memory-mapped and
//! are reached through the [`Bus`]; wiring the PPU/timer/joypad to *set* those
//! bits is the later "Interrupts" item.

mod alu;
mod bus;
mod cb;
mod execute;
mod interrupts;
mod flags;
mod ops;
mod registers;

pub use bus::{Bus, FlatMemory};
pub use flags::Flags;
pub use interrupts::Interrupt;
pub use registers::Registers;

/// A hard fault that halts emulation. Modeled as data (surfaced via
/// [`Cpu::fault`]) rather than a `Result` on the hot path so [`Cpu::step`] can
/// keep returning a plain cycle count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CpuFault {
    #[error("illegal opcode {opcode:#04x} at pc {pc:#06x}; cpu locked up")]
    IllegalOpcode { opcode: u8, pc: u16 },
}

/// The CPU's run status. Undefined opcodes lock the real hardware, so we model
/// that explicitly rather than panicking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunState {
    Running,
    Halted,
    Stopped,
    Locked,
}

/// The SM83 CPU: register file plus the interrupt/halt state machine.
///
/// Generic-free by design (only its methods are generic over `B: Bus`), so it
/// stays trivially `Clone`/`Debug` for future save states and can drive any bus.
#[derive(Debug, Clone)]
pub struct Cpu {
    regs: Registers,
    /// Interrupt Master Enable — a CPU flag, not memory-mapped.
    ime: bool,
    /// Countdown implementing `EI`'s one-instruction delay (0 = idle).
    ei_delay: u8,
    state: RunState,
    /// Set when the `HALT` bug fires: the next opcode fetch must not advance `PC`.
    halt_bug: bool,
    fault: Option<CpuFault>,
}

impl Cpu {
    /// A CPU seeded with the DMG post-boot state (as if the boot ROM had run and
    /// handed control to `0x0100`).
    pub fn new() -> Cpu {
        Cpu {
            regs: Registers::post_boot_dmg(),
            ime: false,
            ei_delay: 0,
            state: RunState::Running,
            halt_bug: false,
            fault: None,
        }
    }

    /// Read-only access to the register file (mainly for tests and debuggers).
    pub fn registers(&self) -> &Registers {
        &self.regs
    }

    /// Whether the CPU has locked up, and why.
    pub fn fault(&self) -> Option<CpuFault> {
        self.fault
    }

    /// Whether interrupts are currently master-enabled.
    pub fn ime(&self) -> bool {
        self.ime
    }

    /// Execute one step and return the T-states it consumed. A step is one of:
    /// an idle cycle (halted/stopped/locked), an interrupt dispatch, or one
    /// fetch-decode-execute of an instruction.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        if matches!(self.state, RunState::Locked | RunState::Stopped) {
            return 4;
        }

        let pending = self.pending_interrupts(bus);

        if self.state == RunState::Halted {
            if pending != 0 {
                self.state = RunState::Running;
                // Fall through: this step services the interrupt (if IME) or
                // runs the next instruction (if not).
            } else {
                return 4;
            }
        }

        if self.ime && pending != 0 {
            return self.service_interrupt(bus, pending);
        }

        let cycles = self.execute_next(bus);
        self.advance_ei_delay();
        cycles
    }

    /// Fetch, decode, and execute one instruction; returns its T-states. Honors
    /// the pending `HALT` bug (re-reading the byte after `HALT`).
    fn execute_next<B: Bus>(&mut self, bus: &mut B) -> u32 {
        let opcode = self.fetch_opcode(bus);
        if opcode == 0xCB {
            let sub = self.fetch8(bus);
            self.execute_cb(bus, sub)
        } else {
            self.execute_base(bus, opcode)
        }
    }

    /// The next opcode byte. Normally advances `PC`; when the halt bug is armed
    /// it reads without advancing (so the byte is consumed twice) and disarms.
    fn fetch_opcode<B: Bus>(&mut self, bus: &mut B) -> u8 {
        if self.halt_bug {
            self.halt_bug = false;
            bus.read8(self.regs.pc)
        } else {
            self.fetch8(bus)
        }
    }

    /// Advance the `EI` delay counter, enabling `IME` when it reaches zero. Runs
    /// once per executed instruction (not on halt-idle or interrupt-dispatch
    /// steps), which is what produces the one-instruction delay.
    fn advance_ei_delay(&mut self) {
        if self.ei_delay > 0 {
            self.ei_delay -= 1;
            if self.ei_delay == 0 {
                self.ime = true;
            }
        }
    }

    /// Read the byte at `PC` and advance `PC`.
    fn fetch8<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let value = bus.read8(self.regs.pc);
        self.regs.pc = self.regs.pc.wrapping_add(1);
        value
    }

    /// Read a little-endian word at `PC` and advance `PC` by two.
    fn fetch16<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.fetch8(bus);
        let hi = self.fetch8(bus);
        u16::from_le_bytes([lo, hi])
    }

    /// Read a signed relative offset at `PC` (for `JR`/`ADD SP,r8`/`LD HL,SP+r8`).
    fn fetch_signed<B: Bus>(&mut self, bus: &mut B) -> i8 {
        self.fetch8(bus) as i8
    }

    /// Push a 16-bit value: `SP` pre-decrements by two, high byte at the higher
    /// address (little-endian on the stack).
    fn push16<B: Bus>(&mut self, bus: &mut B, value: u16) {
        self.regs.sp = self.regs.sp.wrapping_sub(2);
        bus.write16(self.regs.sp, value);
    }

    /// Pop a 16-bit value: read at `SP`, then `SP` post-increments by two.
    fn pop16<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let value = bus.read16(self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(2);
        value
    }

    /// Record an illegal-opcode lockup and return the (idle) cycle cost.
    fn lock_up(&mut self, opcode: u8) -> u32 {
        let pc = self.regs.pc.wrapping_sub(1);
        tracing::error!(opcode = format_args!("{opcode:#04x}"), pc = format_args!("{pc:#06x}"), "illegal opcode; cpu locked up");
        self.fault = Some(CpuFault::IllegalOpcode { opcode, pc });
        self.state = RunState::Locked;
        4
    }
}

impl Default for Cpu {
    fn default() -> Cpu {
        Cpu::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cpu_starts_in_dmg_post_boot_state() {
        let cpu = Cpu::new();
        assert_eq!(cpu.registers().af(), 0x01B0);
        assert_eq!(cpu.registers().pc, 0x0100);
        assert!(!cpu.ime());
        assert!(cpu.fault().is_none());
    }
}

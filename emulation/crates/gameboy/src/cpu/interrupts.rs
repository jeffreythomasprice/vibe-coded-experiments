//! Interrupt sources and the dispatch sequence.
//!
//! Five interrupts share three pieces of state: the CPU's `IME` flag (see
//! [`super::Cpu`]), the memory-mapped `IE` (`0xFFFF`) enable bits, and the `IF`
//! (`0xFF0F`) request bits. Between instructions, if `IME` is set and any
//! `IE & IF` bit is pending, the highest-priority one is serviced: `IME` is
//! cleared, its `IF` bit is cleared, `PC` is pushed, and execution jumps to the
//! interrupt's vector (~20 T-states).
//!
//! The five bits also wake the CPU from `HALT` even when `IME=0` — that logic
//! lives in [`super::Cpu::step`]; here we only expose the source enum plus the
//! CPU-side dispatch and request helpers.

use super::{Bus, Cpu};

/// Address of the `IF` (interrupt request) register.
pub(super) const IF_ADDR: u16 = 0xFF0F;
/// Address of the `IE` (interrupt enable) register.
pub(super) const IE_ADDR: u16 = 0xFFFF;
/// Only the low five bits of `IE`/`IF` correspond to interrupts.
pub(super) const INTERRUPT_MASK: u8 = 0x1F;

/// A Game Boy interrupt source, ordered by priority (highest first) — the same
/// order as its bit index in `IE`/`IF`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    VBlank,
    Lcd,
    Timer,
    Serial,
    Joypad,
}

impl Interrupt {
    /// The five interrupts in priority order (highest first).
    pub const ALL: [Interrupt; 5] = [
        Interrupt::VBlank,
        Interrupt::Lcd,
        Interrupt::Timer,
        Interrupt::Serial,
        Interrupt::Joypad,
    ];

    /// The bit index this interrupt occupies in `IE`/`IF`.
    pub fn bit(self) -> u8 {
        match self {
            Interrupt::VBlank => 0,
            Interrupt::Lcd => 1,
            Interrupt::Timer => 2,
            Interrupt::Serial => 3,
            Interrupt::Joypad => 4,
        }
    }

    /// The address the CPU jumps to when this interrupt is serviced.
    pub fn vector(self) -> u16 {
        match self {
            Interrupt::VBlank => 0x0040,
            Interrupt::Lcd => 0x0048,
            Interrupt::Timer => 0x0050,
            Interrupt::Serial => 0x0058,
            Interrupt::Joypad => 0x0060,
        }
    }
}

impl Cpu {
    /// The set of pending-and-enabled interrupts (`IE & IF`, masked to five bits).
    pub(super) fn pending_interrupts<B: Bus>(&self, bus: &mut B) -> u8 {
        bus.read8(IE_ADDR) & bus.read8(IF_ADDR) & INTERRUPT_MASK
    }

    /// Dispatch the highest-priority pending interrupt: clear `IME`, clear its
    /// `IF` bit, push `PC`, and jump to the vector. Returns 20 T-states.
    ///
    /// `pending` is the already-computed `IE & IF & 0x1F`; the caller has
    /// verified it is non-zero and that `IME` is set.
    pub(super) fn service_interrupt<B: Bus>(&mut self, bus: &mut B, pending: u8) -> u32 {
        let interrupt = Interrupt::ALL
            .into_iter()
            .find(|i| pending & (1 << i.bit()) != 0)
            .expect("service_interrupt called with no pending interrupt");

        self.ime = false;
        let cleared = bus.read8(IF_ADDR) & !(1 << interrupt.bit());
        bus.write8(IF_ADDR, cleared);
        let pc = self.regs.pc;
        self.push16(bus, pc);
        self.regs.pc = interrupt.vector();
        tracing::trace!(?interrupt, vector = format_args!("{:#06x}", interrupt.vector()), "serviced interrupt");
        20
    }

    /// Request an interrupt by setting its `IF` bit. This is the seam future
    /// subsystems (PPU, timer, joypad, serial) use to raise interrupts.
    pub fn request_interrupt<B: Bus>(bus: &mut B, interrupt: Interrupt) {
        let updated = bus.read8(IF_ADDR) | (1 << interrupt.bit());
        bus.write8(IF_ADDR, updated);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::FlatMemory;

    #[test]
    fn bits_and_vectors_match_hardware() {
        assert_eq!(Interrupt::VBlank.bit(), 0);
        assert_eq!(Interrupt::VBlank.vector(), 0x0040);
        assert_eq!(Interrupt::Joypad.bit(), 4);
        assert_eq!(Interrupt::Joypad.vector(), 0x0060);
    }

    #[test]
    fn all_is_in_priority_order() {
        let bits: Vec<u8> = Interrupt::ALL.iter().map(|i| i.bit()).collect();
        assert_eq!(bits, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn request_interrupt_sets_the_if_bit() {
        let mut mem = FlatMemory::new();
        Cpu::request_interrupt(&mut mem, Interrupt::Timer);
        assert_eq!(mem.read8(IF_ADDR) & (1 << 2), 1 << 2);
    }

    /// A CPU at PC 0x0100 running `program`, with a fresh flat memory.
    fn cpu_running(program: &[u8]) -> (Cpu, FlatMemory) {
        let mut cpu = Cpu::new();
        cpu.regs.pc = 0x0100;
        cpu.regs.sp = 0xFFFE;
        let mut mem = FlatMemory::new();
        mem.load(0x0100, program);
        (cpu, mem)
    }

    /// Enable an interrupt in both `IE` and `IF` so it is pending.
    fn arm(mem: &mut FlatMemory, interrupt: Interrupt) {
        mem.write8(IE_ADDR, 1 << interrupt.bit());
        mem.write8(IF_ADDR, 1 << interrupt.bit());
    }

    #[test]
    fn ei_enables_interrupts_after_the_following_instruction() {
        // EI ; NOP ; NOP  with a VBlank pending the whole time.
        let (mut cpu, mut mem) = cpu_running(&[0xFB, 0x00, 0x00]);
        arm(&mut mem, Interrupt::VBlank);

        cpu.step(&mut mem); // EI — IME still off, delay armed
        assert!(!cpu.ime());
        cpu.step(&mut mem); // NOP after EI — IME turns on at the end of this step
        assert!(cpu.ime());
        // Only now does the pending interrupt get serviced.
        let cycles = cpu.step(&mut mem);
        assert_eq!(cycles, 20);
        assert_eq!(cpu.regs.pc, Interrupt::VBlank.vector());
        assert!(!cpu.ime()); // cleared by dispatch
        assert_eq!(mem.read8(IF_ADDR) & 0x01, 0); // IF bit cleared
    }

    #[test]
    fn di_cancels_a_pending_ei_enable() {
        // EI ; DI ; NOP — the EI delay must be cancelled by DI.
        let (mut cpu, mut mem) = cpu_running(&[0xFB, 0xF3, 0x00]);
        arm(&mut mem, Interrupt::VBlank);
        cpu.step(&mut mem); // EI
        cpu.step(&mut mem); // DI cancels the pending enable
        cpu.step(&mut mem); // NOP
        assert!(!cpu.ime());
        assert_ne!(cpu.regs.pc, Interrupt::VBlank.vector());
    }

    #[test]
    fn vblank_has_priority_over_other_pending_interrupts() {
        let (mut cpu, mut mem) = cpu_running(&[0x00]);
        cpu.ime = true;
        // Arm both VBlank (bit 0) and Timer (bit 2).
        mem.write8(IE_ADDR, 0b0000_0101);
        mem.write8(IF_ADDR, 0b0000_0101);
        let cycles = cpu.step(&mut mem);
        assert_eq!(cycles, 20);
        assert_eq!(cpu.regs.pc, Interrupt::VBlank.vector());
        // Only VBlank's request bit is cleared; Timer stays pending.
        assert_eq!(mem.read8(IF_ADDR), 0b0000_0100);
    }

    #[test]
    fn reti_enables_ime_immediately_and_returns() {
        let (mut cpu, mut mem) = cpu_running(&[0xD9]); // RETI
        cpu.regs.sp = 0xC000;
        mem.write16(0xC000, 0x4000);
        let cycles = cpu.step(&mut mem);
        assert_eq!(cycles, 16);
        assert_eq!(cpu.regs.pc, 0x4000);
        assert!(cpu.ime()); // immediate, no one-instruction delay
    }

    #[test]
    fn halt_with_ime_wakes_and_services_pending_interrupt() {
        let (mut cpu, mut mem) = cpu_running(&[0x76, 0x00]); // HALT ; NOP
        cpu.ime = true;
        cpu.step(&mut mem); // HALT with no pending interrupt -> sleeps
        assert_eq!(cpu.regs.pc, 0x0101);
        arm(&mut mem, Interrupt::VBlank);
        let cycles = cpu.step(&mut mem); // wakes and services
        assert_eq!(cycles, 20);
        assert_eq!(cpu.regs.pc, Interrupt::VBlank.vector());
    }

    #[test]
    fn halt_without_ime_wakes_but_does_not_service() {
        let (mut cpu, mut mem) = cpu_running(&[0x76, 0x3C]); // HALT ; INC A
        cpu.ime = false;
        cpu.regs.a = 0;
        cpu.step(&mut mem); // HALT (no pending yet) -> sleeps
        arm(&mut mem, Interrupt::VBlank);
        cpu.step(&mut mem); // wakes, but IME=0 so it runs INC A instead
        assert_eq!(cpu.regs.a, 0x01);
        assert_ne!(cpu.regs.pc, Interrupt::VBlank.vector());
    }

    #[test]
    fn halt_with_no_pending_stays_asleep_and_consumes_idle_cycles() {
        let (mut cpu, mut mem) = cpu_running(&[0x76, 0x3C]); // HALT ; INC A
        cpu.ime = true;
        cpu.regs.a = 0;
        cpu.step(&mut mem); // HALT
        let cycles = cpu.step(&mut mem); // still asleep, nothing pending
        assert_eq!(cycles, 4);
        assert_eq!(cpu.regs.a, 0x00); // INC A did not run
    }

    #[test]
    fn halt_bug_reexecutes_the_byte_after_halt() {
        // IME=0 with an interrupt already pending when HALT runs triggers the
        // bug: the byte after HALT is read twice. INC A therefore runs twice.
        let (mut cpu, mut mem) = cpu_running(&[0x76, 0x3C, 0x00]); // HALT ; INC A ; NOP
        cpu.ime = false;
        cpu.regs.a = 0;
        arm(&mut mem, Interrupt::VBlank);
        cpu.step(&mut mem); // HALT — does not sleep, arms the bug (PC=0x0101)
        assert_eq!(cpu.regs.pc, 0x0101);
        cpu.step(&mut mem); // INC A, but PC does NOT advance (halt bug) -> still 0x0101
        assert_eq!(cpu.regs.a, 0x01);
        assert_eq!(cpu.regs.pc, 0x0101);
        cpu.step(&mut mem); // INC A again (byte read twice), now PC advances
        assert_eq!(cpu.regs.a, 0x02);
        assert_eq!(cpu.regs.pc, 0x0102);
    }

    #[test]
    fn stop_consumes_two_bytes_and_enters_standby() {
        let (mut cpu, mut mem) = cpu_running(&[0x10, 0x00, 0x3C]); // STOP ; INC A
        cpu.regs.a = 0;
        cpu.step(&mut mem); // STOP consumes both bytes
        assert_eq!(cpu.regs.pc, 0x0102);
        let cycles = cpu.step(&mut mem); // idle in standby; INC A does not run
        assert_eq!(cycles, 4);
        assert_eq!(cpu.regs.a, 0x00);
    }
}

//! The Game Boy (DMG) system memory bus.
//!
//! [`SystemBus`] is the real memory-map implementation of the CPU's [`Bus`]
//! trait — the second `impl Bus` in the crate, alongside the flat-memory test
//! double [`FlatMemory`](crate::cpu::FlatMemory). Where `FlatMemory` treats
//! every address as plain RAM, `SystemBus` decodes the DMG address space and
//! routes each region to the right place: the ROM (`0x0000-0x7FFF`) and external
//! RAM (`0xA000-0xBFFF`) regions go to the [`Cartridge`] (and its MBC), and the
//! rest is backed by RAM owned here.
//!
//! **This is where the interrupt registers live.** The CPU's interrupt dispatch
//! (see [`crate::cpu`]) only knows the *addresses* `IE` (`0xFFFF`) and `IF`
//! (`0xFF0F`) and reaches them through the [`Bus`]; the actual storage — and the
//! hardware quirk that `IF`'s top three bits always read as `1` — lives here in
//! [`InterruptRegisters`], not in the CPU.
//!
//! The timer/divider (`DIV`/`TIMA`/`TMA`/`TAC`, `0xFF04-0xFF07`) is real: it is
//! a [`Timer`] owned here and advanced by [`SystemBus::tick`], which also raises
//! the timer interrupt on `IF` when `TIMA` overflows.
//!
//! The PPU is real too: [`Ppu`] owns VRAM (`0x8000-0x9FFF`), OAM
//! (`0xFE00-0xFE9F`), and the LCD registers (`0xFF40-0xFF45`, `0xFF47-0xFF4B`),
//! all routed to it here. It is advanced by [`SystemBus::tick`], which raises the
//! V-Blank and STAT interrupts on `IF` and latches completed frames for
//! [`SystemBus::take_frame_ready`]. VRAM/OAM CPU access is gated by PPU mode (the
//! `Ppu` returns `0xFF` / drops writes while it owns the bus).
//!
//! OAM DMA (`0xFF46`) is real: an [`OamDma`] controller owned here performs a
//! timed 160-byte copy into OAM, advanced by [`SystemBus::tick`]. While it runs,
//! the CPU-facing [`Bus`] read/write methods gate all non-HRAM access (returning
//! `0xFF` / dropping writes) over the raw [`SystemBus::read_raw`] decode.
//!
//! The joypad (`P1/JOYP`, `0xFF00`) is real: a [`Joypad`] owned here decodes the
//! button matrix, and [`SystemBus::set_buttons`] pushes the frontend's held-button
//! set into it, raising the joypad interrupt on a press edge.
//!
//! The APU (`0xFF10-0xFF3F`) is real too: an [`Apu`] owned here services the
//! sound registers and Wave RAM, is advanced by [`SystemBus::tick`], and
//! produces audio samples latched for [`SystemBus::take_audio_samples`]. It
//! raises no interrupt.
//!
//! Every I/O register the machine defines now routes to a real subsystem; the
//! remaining flat read/write storage in the I/O array is only the handful of
//! unimplemented/unused registers (e.g. `0xFF03`, `0xFF08-0xFF0E`).

use std::collections::HashSet;

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::cpu::{Bus, Interrupt};
use crate::dma::OamDma;
use crate::input::GameboyButton;
use crate::joypad::Joypad;
use crate::ppu::Ppu;
use crate::serial::Serial;
use crate::timer::Timer;

/// The three high bits of `IF` (`0xFF0F`) that always read back as `1` on
/// hardware; only the low five bits are real interrupt-request flags. This is a
/// *storage* quirk local to the bus, distinct from the CPU's dispatch mask.
const IF_UNUSED_MASK: u8 = 0xE0;

/// The memory-mapped interrupt registers: `IE` (`0xFFFF`) and `IF` (`0xFF0F`).
///
/// `IE` stores all eight bits verbatim. `IF` has only five significant bits (one
/// per interrupt source); its top three read back as `1` and are dropped on
/// write. The CPU masks with `0x1F` when it dispatches, but a program that reads
/// `0xFF0F` directly still observes the forced `1`s, so the quirk is modeled here
/// rather than leaked into the CPU.
#[derive(Debug, Clone, Copy, Default)]
struct InterruptRegisters {
    enable: u8,
    request: u8,
}

impl InterruptRegisters {
    fn read_enable(&self) -> u8 {
        self.enable
    }

    fn write_enable(&mut self, value: u8) {
        self.enable = value;
    }

    fn read_request(&self) -> u8 {
        self.request | IF_UNUSED_MASK
    }

    fn write_request(&mut self, value: u8) {
        self.request = value & !IF_UNUSED_MASK;
    }

    /// Raise an interrupt by setting its `IF` bit. The seam an on-bus subsystem
    /// (here, the timer) uses instead of [`Cpu::request_interrupt`], which needs
    /// a whole-`Bus` borrow.
    ///
    /// [`Cpu::request_interrupt`]: crate::Cpu::request_interrupt
    fn request(&mut self, interrupt: Interrupt) {
        self.request |= 1 << interrupt.bit();
    }
}

const WRAM_SIZE: usize = 0x2000;
const IO_SIZE: usize = 0x0080;
const HRAM_SIZE: usize = 0x007F;

/// The DMG system bus: a [`Cartridge`] plus the internal RAM regions, decoded
/// into a single 16-bit address space.
///
/// RAM regions are boxed fixed-size arrays (mirroring [`FlatMemory`]'s
/// `Box<[u8; N]>`): cheap to move, and the compile-time-fixed widths make an
/// out-of-bounds index impossible for those regions.
#[derive(Debug)]
pub struct SystemBus {
    cartridge: Cartridge,
    wram: Box<[u8; WRAM_SIZE]>,
    io: Box<[u8; IO_SIZE]>,
    hram: Box<[u8; HRAM_SIZE]>,
    interrupts: InterruptRegisters,
    timer: Timer,
    ppu: Ppu,
    dma: OamDma,
    joypad: Joypad,
    serial: Serial,
    apu: Apu,
    /// Latches when the PPU completes a frame, so a UI loop can decide when to
    /// present. Consumed via [`SystemBus::take_frame_ready`].
    frame_ready: bool,
}

impl SystemBus {
    /// Build the system bus around a loaded cartridge. All internal RAM powers on
    /// zeroed — we skip the boot ROM, so nothing seeds it (hardware comes up with
    /// indeterminate contents, which emulators conventionally treat as zero).
    pub fn new(cartridge: Cartridge) -> SystemBus {
        tracing::debug!("system bus initialized");
        SystemBus {
            cartridge,
            wram: Box::new([0; WRAM_SIZE]),
            io: Box::new([0; IO_SIZE]),
            hram: Box::new([0; HRAM_SIZE]),
            interrupts: InterruptRegisters::default(),
            timer: Timer::new(),
            ppu: Ppu::new(),
            dma: OamDma::new(),
            joypad: Joypad::new(),
            serial: Serial::new(),
            apu: Apu::new(),
            frame_ready: false,
        }
    }

    /// Advance the time-dependent subsystems by `t_states` T-cycles — the count
    /// a just-executed instruction (or interrupt dispatch) consumed. Advances
    /// the timer, the PPU, and any in-flight OAM DMA transfer, and raises
    /// whatever interrupts the timer/PPU request on `IF` (OAM DMA raises none).
    /// A completed PPU frame is latched for [`SystemBus::take_frame_ready`].
    ///
    /// There is no top-level run loop yet that both steps the CPU and calls
    /// this; for now it is driven directly (e.g. by tests).
    pub fn tick(&mut self, t_states: u32) {
        if self.timer.tick(t_states) {
            self.interrupts.request(Interrupt::Timer);
        }
        if self.serial.tick(t_states) {
            self.interrupts.request(Interrupt::Serial);
        }
        let step = self.ppu.step(t_states);
        for (interrupt, requested) in step.interrupts() {
            if requested {
                self.interrupts.request(interrupt);
            }
        }
        if step.frame_complete {
            self.frame_ready = true;
        }
        self.apu.tick(t_states);
        self.tick_dma(t_states);
    }

    /// Advance an in-flight OAM DMA transfer by `t_states` T-cycles, copying the
    /// bytes that came due. The source read goes through the ungated [`read_raw`]
    /// so the transfer is not blocked by its own bus hold, and the OAM write
    /// bypasses the PPU mode lock via [`Ppu::write_oam_dma`].
    ///
    /// [`read_raw`]: SystemBus::read_raw
    fn tick_dma(&mut self, t_states: u32) {
        if !self.dma.is_active() {
            return;
        }
        let base = (self.dma.source_page() as u16) << 8;
        let (start, end) = self.dma.advance(t_states);
        for i in start..end {
            let byte = self.read_raw(base + i as u16);
            self.ppu.write_oam_dma(i, byte);
        }
    }

    /// The current PPU framebuffer: `160 * 144` shade indices (`0-3`), row-major.
    pub fn framebuffer(&self) -> &[u8] {
        self.ppu.framebuffer()
    }

    /// Take and clear the "a frame is ready to present" latch.
    pub fn take_frame_ready(&mut self) -> bool {
        std::mem::take(&mut self.frame_ready)
    }

    /// Drain the interleaved stereo audio samples (`L, R, …`, in `[-1.0, 1.0]`)
    /// the APU has produced since the last call. The seam a future run loop
    /// feeds to the `audio` crate each frame; like [`SystemBus::set_buttons`] it
    /// is exercised directly by tests for now, there being no run loop yet.
    pub fn take_audio_samples(&mut self) -> Vec<f32> {
        self.apu.take_samples()
    }

    /// Drain the bytes the game has transmitted over the serial port since the
    /// last call, in order. Used by the headless test-ROM harness to read the
    /// ASCII stream Blargg's suites print to the link port; see
    /// [`Serial::take_output`](crate::serial::Serial::take_output).
    pub fn take_serial_output(&mut self) -> Vec<u8> {
        self.serial.take_output()
    }

    /// Push the frontend's currently-held Game Boy buttons into the joypad
    /// register, raising the joypad interrupt on a press edge. This is the seam
    /// the future top-level run loop calls each frame with the input router's
    /// pressed set; there is no such loop yet, so (like the timer/PPU/DMA) it is
    /// exercised directly by tests for now.
    pub fn set_buttons(&mut self, held: &HashSet<GameboyButton>) {
        if self.joypad.set_pressed(held) {
            self.interrupts.request(Interrupt::Joypad);
        }
    }

    /// The loaded cartridge (for header inspection and, later, save-RAM readout).
    pub fn cartridge(&self) -> &Cartridge {
        &self.cartridge
    }

    /// Mutable access to the cartridge (for save-RAM restore plumbing).
    pub fn cartridge_mut(&mut self) -> &mut Cartridge {
        &mut self.cartridge
    }
}

/// Whether `addr` is in HRAM (`0xFF80-0xFFFE`) — the only region the CPU can
/// still reach while an OAM DMA transfer holds the bus.
fn is_hram(addr: u16) -> bool {
    matches!(addr, 0xFF80..=0xFFFE)
}

impl SystemBus {
    /// The raw address decode, with no OAM-DMA bus-blocking gate. This is both
    /// the body the [`Bus`] methods wrap and the path the DMA transfer itself
    /// uses to read its source (so a transfer is never blocked by its own hold).
    fn read_raw(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.cartridge.read(addr),
            0x8000..=0x9FFF => self.ppu.read_vram(addr),
            0xA000..=0xBFFF => self.cartridge.read(addr),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize],
            // Echo RAM: `addr - 0xE000` indexes WRAM `0x0000..=0x1DFF`, i.e. the
            // mirror of `0xC000-0xDDFF`. The top 0x200 of WRAM is not echoed, and
            // the subtraction produces exactly that partial mirror.
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize],
            0xFE00..=0xFE9F => self.ppu.read_oam(addr),
            0xFEA0..=0xFEFF => 0xFF,
            0xFF04..=0xFF07 => self.timer.read(addr),
            0xFF0F => self.interrupts.read_request(),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.read_register(addr),
            0xFF46 => self.dma.source_page(),
            0xFF00 => self.joypad.read(),
            0xFF01..=0xFF02 => self.serial.read(addr),
            0xFF10..=0xFF3F => self.apu.read(addr),
            0xFF03..=0xFF7F => self.io[(addr - 0xFF00) as usize],
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize],
            0xFFFF => self.interrupts.read_enable(),
        }
    }

    /// The raw address decode for writes, with no OAM-DMA bus-blocking gate. A
    /// write to `0xFF46` triggers (or restarts) an OAM DMA transfer.
    fn write_raw(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.cartridge.write(addr, value),
            0x8000..=0x9FFF => self.ppu.write_vram(addr, value),
            0xA000..=0xBFFF => self.cartridge.write(addr, value),
            0xC000..=0xDFFF => self.wram[(addr - 0xC000) as usize] = value,
            0xE000..=0xFDFF => self.wram[(addr - 0xE000) as usize] = value,
            0xFE00..=0xFE9F => self.ppu.write_oam(addr, value),
            0xFEA0..=0xFEFF => {}
            0xFF04..=0xFF07 => self.timer.write(addr, value),
            0xFF0F => self.interrupts.write_request(value),
            0xFF40..=0xFF45 | 0xFF47..=0xFF4B => self.ppu.write_register(addr, value),
            0xFF46 => self.dma.trigger(value),
            0xFF00 => {
                if self.joypad.write(value) {
                    self.interrupts.request(Interrupt::Joypad);
                }
            }
            0xFF01..=0xFF02 => self.serial.write(addr, value),
            0xFF10..=0xFF3F => self.apu.write(addr, value),
            0xFF03..=0xFF7F => self.io[(addr - 0xFF00) as usize] = value,
            0xFF80..=0xFFFE => self.hram[(addr - 0xFF80) as usize] = value,
            0xFFFF => self.interrupts.write_enable(value),
        }
    }
}

impl Bus for SystemBus {
    /// While an OAM DMA transfer is active the CPU can only reach HRAM; any other
    /// read observes the bus conflict as `0xFF`. The `0xFF46` register itself
    /// stays readable so a transfer can be re-triggered mid-flight.
    fn read8(&mut self, addr: u16) -> u8 {
        if self.dma.is_active() && !is_hram(addr) && addr != 0xFF46 {
            return 0xFF;
        }
        self.read_raw(addr)
    }

    /// The write-side counterpart of the OAM DMA bus block: non-HRAM writes are
    /// dropped while a transfer is active, except the `0xFF46` trigger itself.
    fn write8(&mut self, addr: u16, value: u8) {
        if self.dma.is_active() && !is_hram(addr) && addr != 0xFF46 {
            return;
        }
        self.write_raw(addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::{Cpu, Interrupt};

    /// Build a minimal 32 KiB ROM-only cartridge image with a valid header
    /// checksum. Optionally splice a program in at the entry point (`0x0100`).
    fn build_rom(program_at_0100: &[u8]) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0134..0x0134 + 4].copy_from_slice(b"ROMX");
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KiB
        rom[0x0149] = 0x00; // no RAM
        let mut x = 0u8;
        for &b in &rom[0x0134..=0x014C] {
            x = x.wrapping_sub(b).wrapping_sub(1);
        }
        rom[0x014D] = x;
        rom[0x0100..0x0100 + program_at_0100.len()].copy_from_slice(program_at_0100);
        rom
    }

    fn test_bus() -> SystemBus {
        let cart = Cartridge::from_bytes(&build_rom(&[])).expect("test rom parses");
        SystemBus::new(cart)
    }

    #[test]
    fn wram_read_write_roundtrips() {
        let mut bus = test_bus();
        for &addr in &[0xC000u16, 0xCFFF, 0xDFFF] {
            bus.write8(addr, 0xAB);
            assert_eq!(bus.read8(addr), 0xAB);
        }
    }

    #[test]
    fn vram_read_write_roundtrips() {
        let mut bus = test_bus();
        for &addr in &[0x8000u16, 0x9FFF] {
            bus.write8(addr, 0x5A);
            assert_eq!(bus.read8(addr), 0x5A);
        }
    }

    #[test]
    fn oam_read_write_roundtrips() {
        let mut bus = test_bus();
        for &addr in &[0xFE00u16, 0xFE9F] {
            bus.write8(addr, 0x3C);
            assert_eq!(bus.read8(addr), 0x3C);
        }
    }

    #[test]
    fn hram_read_write_roundtrips() {
        let mut bus = test_bus();
        for &addr in &[0xFF80u16, 0xFFFE] {
            bus.write8(addr, 0x77);
            assert_eq!(bus.read8(addr), 0x77);
        }
        // 0xFFFF is IE, not the top of HRAM: writing HRAM's last byte must not
        // touch it, and vice versa.
        bus.write8(0xFFFE, 0x11);
        bus.write8(0xFFFF, 0x22);
        assert_eq!(bus.read8(0xFFFE), 0x11);
        assert_eq!(bus.read8(0xFFFF), 0x22);
    }

    #[test]
    fn io_stub_read_write_roundtrips() {
        // The handful of genuinely-unimplemented I/O registers are still flat
        // storage (documented stub behavior); 0xFF08 is one of them.
        let mut bus = test_bus();
        bus.write8(0xFF08, 0x91);
        assert_eq!(bus.read8(0xFF08), 0x91);
    }

    #[test]
    fn apu_registers_route_to_the_apu_not_the_io_stub() {
        // NR10 (0xFF10) forces bit 7 to 1 on read — proof the APU, not the flat
        // I/O array, services this address (the array would return the stored 0).
        let mut bus = test_bus();
        bus.write8(0xFF10, 0x00);
        assert_eq!(bus.read8(0xFF10), 0x80);
        // NR52 (0xFF26) reads its post-boot 0x70 (forced bits, power off).
        assert_eq!(bus.read8(0xFF26), 0x70);
    }

    #[test]
    fn apu_produces_audio_samples_when_ticked() {
        // Power the APU on and tick a span; samples accumulate and drain.
        let mut bus = test_bus();
        bus.write8(0xFF26, 0x80); // NR52: power on
        bus.tick(100_000);
        let samples = bus.take_audio_samples();
        assert!(!samples.is_empty(), "ticking a powered APU yields samples");
        assert!(bus.take_audio_samples().is_empty(), "draining empties the buffer");
    }

    #[test]
    fn oam_dma_readback_returns_source_page() {
        let mut bus = test_bus();
        bus.write8(0xFF46, 0xC0);
        // 0xFF46 stays readable even mid-transfer, returning the source page.
        assert_eq!(bus.read8(0xFF46), 0xC0);
    }

    #[test]
    fn oam_dma_copies_page_to_oam() {
        // A game's V-Blank sprite update, driven through the real bus: seed a
        // source page in WRAM, kick the DMA, tick a full transfer, read OAM back.
        let mut bus = test_bus();
        for i in 0..0xA0u16 {
            bus.write8(0xC000 + i, (i as u8).wrapping_mul(3).wrapping_add(1));
        }
        bus.write8(0xFF46, 0xC0); // source page 0xC0 → 0xC000..=0xC09F
        bus.tick(640); // 160 M-cycles: the whole transfer

        // LCD is off (power-on), so OAM is freely readable.
        for i in 0..0xA0u16 {
            assert_eq!(
                bus.read8(0xFE00 + i),
                (i as u8).wrapping_mul(3).wrapping_add(1),
                "OAM byte {i:#04x}"
            );
        }
        // Normal (non-HRAM) access has resumed after completion.
        bus.write8(0xC000, 0x42);
        assert_eq!(bus.read8(0xC000), 0x42);
    }

    #[test]
    fn oam_dma_is_timed_not_instant() {
        let mut bus = test_bus();
        bus.write8(0xD000, 0x11); // a byte in WRAM outside the source page
        bus.write8(0xC000, 0x55); // source page byte 0
        bus.write8(0xFF46, 0xC0);

        // One M-cycle in: transfer is still active, so the bus block is in
        // effect — proof it did not complete instantly.
        bus.tick(4);
        assert_eq!(bus.read8(0xD000), 0xFF); // blocked, not 0x11

        // Finish the remaining 159 M-cycles; access resumes.
        bus.tick(636);
        assert_eq!(bus.read8(0xD000), 0x11);
        assert_eq!(bus.read8(0xFE00), 0x55); // byte 0 landed in OAM
    }

    #[test]
    fn oam_dma_blocks_non_hram_access() {
        let mut bus = test_bus();
        bus.write8(0xFF80, 0xAA); // HRAM
        bus.write8(0xC000, 0xBB); // WRAM (non-HRAM)
        bus.write8(0xFF46, 0xD0); // kick a transfer (source page 0xD0)

        // Mid-transfer (before the 640-cycle completion): HRAM still works,
        // non-HRAM reads see the conflict as 0xFF, non-HRAM writes are dropped.
        bus.tick(4);
        assert_eq!(bus.read8(0xFF80), 0xAA);
        assert_eq!(bus.read8(0xC000), 0xFF);
        bus.write8(0xC000, 0x99); // dropped
        bus.tick(636); // complete the transfer
        assert_eq!(bus.read8(0xC000), 0xBB); // the dropped write never landed
    }

    #[test]
    fn ppu_registers_route_to_the_ppu_not_the_io_stub() {
        // STAT (0xFF41) forces bit 7 to 1 and masks the low bits — proof the
        // PPU, not the flat I/O array, services this address.
        let mut bus = test_bus();
        bus.write8(0xFF41, 0x00);
        assert_eq!(bus.read8(0xFF41) & 0x80, 0x80);
        // LY (0xFF44) is read-only through the bus.
        bus.write8(0xFF44, 0x50);
        assert_eq!(bus.read8(0xFF44), 0x00);
    }

    #[test]
    fn ppu_renders_a_bg_tile_through_the_system_bus() {
        // End-to-end: set up tile data + BG map + palette + LCDC entirely through
        // the bus, tick one scanline, and read the rendered framebuffer back out.
        let mut bus = test_bus();
        // Tile 1 (at 0x8010) solid color 3.
        for row in 0..8u16 {
            bus.write8(0x8010 + row * 2, 0xFF);
            bus.write8(0x8010 + row * 2 + 1, 0xFF);
        }
        bus.write8(0x9800, 0x01); // BG map entry (0,0) → tile 1
        bus.write8(0xFF47, 0b11_10_01_00); // BGP identity
        // LCDC: enable | 0x8000 tile-data | BG enable.
        bus.write8(0xFF40, 0x80 | 0x10 | 0x01);

        bus.tick(456); // one full scanline: line 0 is drawn during it

        assert_eq!(bus.framebuffer()[0], 3); // top-left pixel shows shade 3
    }

    #[test]
    fn ppu_vblank_services_interrupt_through_system_bus() {
        // ROM: EI ; NOP ; NOP at the entry point.
        let cart = Cartridge::from_bytes(&build_rom(&[0xFB, 0x00, 0x00])).unwrap();
        let mut bus = SystemBus::new(cart);
        let mut cpu = Cpu::new();

        bus.write8(0xFFFF, 1 << Interrupt::VBlank.bit()); // enable VBlank in IE
        bus.write8(0xFF40, 0x80); // LCDC: enable the LCD so the PPU runs

        // A full frame is 154 * 456 dots; VBlank is requested on entering line
        // 144. Tick a whole frame so LY reaches 144.
        bus.tick(144 * 456);
        assert_eq!(bus.read8(0xFF44), 144); // LY
        assert_eq!(bus.read8(0xFF0F) & 1, 1); // VBlank requested in IF
        assert!(bus.take_frame_ready());

        cpu.step(&mut bus); // EI  — IME armed
        cpu.step(&mut bus); // NOP — IME on
        let cycles = cpu.step(&mut bus); // dispatch

        assert_eq!(cycles, 20);
        assert_eq!(cpu.registers().pc, Interrupt::VBlank.vector());
        assert_eq!(bus.read8(0xFF0F) & 1, 0); // VBlank bit cleared by dispatch
    }

    #[test]
    fn echo_ram_aliases_wram() {
        let mut bus = test_bus();
        bus.write8(0xC000, 0xAB);
        assert_eq!(bus.read8(0xE000), 0xAB);
        bus.write8(0xE005, 0xCD);
        assert_eq!(bus.read8(0xC005), 0xCD);
        // Near the top of the echo window.
        bus.write8(0xC1FF, 0xEF);
        assert_eq!(bus.read8(0xE1FF), 0xEF);
    }

    #[test]
    fn unusable_region_reads_ff_and_ignores_writes() {
        let mut bus = test_bus();
        assert_eq!(bus.read8(0xFEA0), 0xFF);
        assert_eq!(bus.read8(0xFEFF), 0xFF);
        bus.write8(0xFEA0, 0x00);
        assert_eq!(bus.read8(0xFEA0), 0xFF);
    }

    #[test]
    fn ie_stores_all_eight_bits() {
        let mut bus = test_bus();
        bus.write8(0xFFFF, 0xFF);
        assert_eq!(bus.read8(0xFFFF), 0xFF);
        bus.write8(0xFFFF, 0x00);
        assert_eq!(bus.read8(0xFFFF), 0x00);
    }

    #[test]
    fn if_top_three_bits_read_as_one() {
        let mut bus = test_bus();
        bus.write8(0xFF0F, 0x00);
        assert_eq!(bus.read8(0xFF0F), 0xE0);
        bus.write8(0xFF0F, 0x05);
        assert_eq!(bus.read8(0xFF0F), 0xE5);
        bus.write8(0xFF0F, 0xFF);
        assert_eq!(bus.read8(0xFF0F), 0xFF);
    }

    #[test]
    fn if_write_drops_top_three_bits() {
        let mut bus = test_bus();
        bus.write8(0xFF0F, 0xE0); // only the forced-1 bits
        assert_eq!(bus.read8(0xFF0F), 0xE0); // no low bits leaked in
    }

    #[test]
    fn rom_readable_through_bus() {
        let mut bus = test_bus();
        // The cartridge-type byte at 0x0147 is 0x00 (ROM only) in our test image.
        assert_eq!(bus.read8(0x0147), 0x00);
    }

    #[test]
    fn rom_write_is_mbc_control_not_data() {
        let mut bus = test_bus();
        let before = bus.read8(0x2000);
        bus.write8(0x2000, 0xAB); // ROM-region write: an MBC command, not data
        assert_eq!(bus.read8(0x2000), before); // RomOnly ignores it
    }

    #[test]
    fn cpu_services_interrupt_through_system_bus() {
        // ROM: EI ; NOP ; NOP at the 0x0100 entry point.
        let cart = Cartridge::from_bytes(&build_rom(&[0xFB, 0x00, 0x00])).unwrap();
        let mut bus = SystemBus::new(cart);
        let mut cpu = Cpu::new(); // post-boot: PC=0x0100, SP=0xFFFE

        bus.write8(0xFFFF, 0x01); // enable VBlank in IE
        Cpu::request_interrupt(&mut bus, Interrupt::VBlank); // set IF bit 0

        cpu.step(&mut bus); // EI  — IME still off, delay armed
        cpu.step(&mut bus); // NOP — IME turns on at end of this step
        let cycles = cpu.step(&mut bus); // dispatch

        assert_eq!(cycles, 20);
        assert_eq!(cpu.registers().pc, Interrupt::VBlank.vector());
        // VBlank's IF bit is cleared by dispatch; the forced top bits remain.
        assert_eq!(bus.read8(0xFF0F), 0xE0);
    }

    #[test]
    fn timer_registers_route_to_the_timer_not_the_io_stub() {
        // 0xFF07 (TAC) forces its unused upper bits to 1 — proof the timer, not
        // the flat I/O array, is servicing this address.
        let mut bus = test_bus();
        bus.write8(0xFF07, 0x00);
        assert_eq!(bus.read8(0xFF07), 0xF8);
        // A DIV write resets the counter rather than storing the byte.
        bus.write8(0xFF04, 0xAB);
        assert_eq!(bus.read8(0xFF04), 0x00);
    }

    #[test]
    fn timer_overflow_services_interrupt_through_system_bus() {
        // ROM: EI ; NOP ; NOP at the entry point.
        let cart = Cartridge::from_bytes(&build_rom(&[0xFB, 0x00, 0x00])).unwrap();
        let mut bus = SystemBus::new(cart);
        let mut cpu = Cpu::new();

        bus.write8(0xFFFF, 1 << Interrupt::Timer.bit()); // enable Timer in IE
        bus.write8(0xFF06, 0x00); // TMA
        bus.write8(0xFF05, 0xFF); // TIMA one short of overflow
        bus.write8(0xFF07, 0x05); // TAC: enable + ÷16 clock select

        // 16 T-cycles overflow TIMA; 4 more complete the delayed reload and set
        // the timer bit in IF.
        bus.tick(20);
        assert_eq!(bus.read8(0xFF0F), 0xE4); // Timer (bit 2) requested

        cpu.step(&mut bus); // EI  — IME armed
        cpu.step(&mut bus); // NOP — IME on
        let cycles = cpu.step(&mut bus); // dispatch

        assert_eq!(cycles, 20);
        assert_eq!(cpu.registers().pc, Interrupt::Timer.vector());
        assert_eq!(bus.read8(0xFF0F), 0xE0); // Timer bit cleared by dispatch
    }

    #[test]
    fn request_interrupt_then_read_if_through_bus() {
        let mut bus = test_bus();
        Cpu::request_interrupt(&mut bus, Interrupt::Timer); // bit 2
        assert_eq!(bus.read8(0xFF0F), 0xE4); // bit 2 set + forced top three
    }

    #[test]
    fn joypad_registers_route_to_the_joypad_not_the_io_stub() {
        // Writing 0x00 to FF00 and reading back 0xCF (forced unused bits + both
        // rows selected, nothing held) proves the joypad, not the flat I/O array,
        // is servicing this address — the array would return the stored 0x00.
        let mut bus = test_bus();
        bus.write8(0xFF00, 0x00);
        assert_eq!(bus.read8(0xFF00), 0xCF);
    }

    #[test]
    fn joypad_press_requests_interrupt_through_system_bus() {
        // ROM: EI ; NOP ; NOP at the entry point.
        let cart = Cartridge::from_bytes(&build_rom(&[0xFB, 0x00, 0x00])).unwrap();
        let mut bus = SystemBus::new(cart);
        let mut cpu = Cpu::new();

        bus.write8(0xFFFF, 1 << Interrupt::Joypad.bit()); // enable Joypad in IE
        bus.write8(0xFF00, 0b0001_0000); // select the buttons row (P14=1, P15=0)

        let held: HashSet<GameboyButton> = [GameboyButton::A].into_iter().collect();
        bus.set_buttons(&held);
        assert_eq!(bus.read8(0xFF0F), 0xF0); // Joypad (bit 4) requested

        cpu.step(&mut bus); // EI  — IME armed
        cpu.step(&mut bus); // NOP — IME on
        let cycles = cpu.step(&mut bus); // dispatch

        assert_eq!(cycles, 20);
        assert_eq!(cpu.registers().pc, Interrupt::Joypad.vector());
        assert_eq!(bus.read8(0xFF0F), 0xE0); // Joypad bit cleared by dispatch
    }

    #[test]
    fn serial_registers_route_to_the_serial_not_the_io_stub() {
        // Writing 0x00 to SC (0xFF02) and reading back 0x7E (forced unused bits)
        // proves the serial port, not the flat I/O array, is servicing this
        // address — the array would return the stored 0x00.
        let mut bus = test_bus();
        bus.write8(0xFF02, 0x00);
        assert_eq!(bus.read8(0xFF02), 0x7E);
    }

    #[test]
    fn serial_transfer_services_interrupt_through_system_bus() {
        // ROM: EI ; NOP ; NOP at the entry point.
        let cart = Cartridge::from_bytes(&build_rom(&[0xFB, 0x00, 0x00])).unwrap();
        let mut bus = SystemBus::new(cart);
        let mut cpu = Cpu::new();

        bus.write8(0xFFFF, 1 << Interrupt::Serial.bit()); // enable Serial in IE
        bus.write8(0xFF02, 0x81); // SC: start transfer + internal clock

        // A full internal-clock byte takes 4096 T-cycles; then the transfer
        // completes and sets the serial bit in IF.
        bus.tick(4096);
        assert_eq!(bus.read8(0xFF0F), 0xE8); // Serial (bit 3) requested
        assert_eq!(bus.read8(0xFF01), 0xFF); // SB clocked in 0xFF (no partner)

        cpu.step(&mut bus); // EI  — IME armed
        cpu.step(&mut bus); // NOP — IME on
        let cycles = cpu.step(&mut bus); // dispatch

        assert_eq!(cycles, 20);
        assert_eq!(cpu.registers().pc, Interrupt::Serial.vector());
        assert_eq!(bus.read8(0xFF0F), 0xE0); // Serial bit cleared by dispatch
    }
}

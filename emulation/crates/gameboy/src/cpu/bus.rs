//! The CPU's view of memory.
//!
//! The SM83 core reaches the rest of the machine through exactly one seam: the
//! [`Bus`] trait's `read8`/`write8`. This mirrors the abstraction-boundary
//! pattern used by `common::storage` — the trait lives here, and the real
//! system memory map (cartridge + WRAM + VRAM + OAM + I/O + HRAM) becomes just
//! another `impl Bus` once those subsystems exist. Keeping the CPU generic over
//! `B: Bus` gives static dispatch (monomorphization, no vtable) on the hottest
//! path in the emulator, and lets the CPU be exercised in isolation against the
//! flat-memory [`FlatMemory`] double below.
//!
//! `read8` takes `&mut self` deliberately: a future cycle-stepped model ticks
//! the PPU/timer/DMA *during* each access, and some reads are side-effectful
//! (serial, certain I/O). Requiring `&mut` now keeps the CPU's call sites stable
//! when that refinement lands, and costs nothing — the CPU already needs `&mut B`
//! to write.

/// The memory interface the CPU drives. 16-bit little-endian helpers are
/// provided so byte-ordering lives in exactly one place.
pub trait Bus {
    /// Read one byte from `addr`.
    fn read8(&mut self, addr: u16) -> u8;

    /// Write one byte to `addr`.
    fn write8(&mut self, addr: u16, value: u8);

    /// Read a little-endian 16-bit word (`addr` low byte, `addr+1` high byte).
    fn read16(&mut self, addr: u16) -> u16 {
        let lo = self.read8(addr);
        let hi = self.read8(addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    /// Write a little-endian 16-bit word (`addr` low byte, `addr+1` high byte).
    fn write16(&mut self, addr: u16, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        self.write8(addr, lo);
        self.write8(addr.wrapping_add(1), hi);
    }
}

/// A flat 64 KiB address space with no region decoding: every address is plain
/// RAM. Used only to unit-test the CPU in isolation — real hardware behavior
/// (open bus, ROM read-only, I/O side effects) is out of scope here.
#[derive(Clone)]
pub struct FlatMemory {
    bytes: Box<[u8; 0x1_0000]>,
}

impl FlatMemory {
    /// A zeroed 64 KiB space.
    pub fn new() -> FlatMemory {
        FlatMemory {
            bytes: Box::new([0; 0x1_0000]),
        }
    }

    /// Copy `program` into memory starting at `addr` (wrapping if it would run
    /// off the end), typically to place a small test program at the PC.
    pub fn load(&mut self, addr: u16, program: &[u8]) {
        for (i, &byte) in program.iter().enumerate() {
            self.bytes[addr.wrapping_add(i as u16) as usize] = byte;
        }
    }
}

impl Default for FlatMemory {
    fn default() -> FlatMemory {
        FlatMemory::new()
    }
}

impl Bus for FlatMemory {
    fn read8(&mut self, addr: u16) -> u8 {
        self.bytes[addr as usize]
    }

    fn write8(&mut self, addr: u16, value: u8) {
        self.bytes[addr as usize] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_memory_read_write_roundtrips() {
        let mut mem = FlatMemory::new();
        mem.write8(0x1234, 0xAB);
        assert_eq!(mem.read8(0x1234), 0xAB);
    }

    #[test]
    fn load_places_program_at_address() {
        let mut mem = FlatMemory::new();
        mem.load(0x0100, &[0x01, 0x02, 0x03]);
        assert_eq!(mem.read8(0x0100), 0x01);
        assert_eq!(mem.read8(0x0101), 0x02);
        assert_eq!(mem.read8(0x0102), 0x03);
        assert_eq!(mem.read8(0x0103), 0x00);
    }

    #[test]
    fn read16_and_write16_are_little_endian() {
        let mut mem = FlatMemory::new();
        mem.write16(0x2000, 0xBEEF);
        assert_eq!(mem.read8(0x2000), 0xEF);
        assert_eq!(mem.read8(0x2001), 0xBE);
        assert_eq!(mem.read16(0x2000), 0xBEEF);
    }

    #[test]
    fn read16_wraps_across_the_top_of_memory() {
        let mut mem = FlatMemory::new();
        mem.write8(0xFFFF, 0x34);
        mem.write8(0x0000, 0x12);
        assert_eq!(mem.read16(0xFFFF), 0x1234);
    }
}

//! MBC2: types `0x05`/`0x06`. Up to 256 KiB ROM and a built-in 512×4-bit RAM
//! (no external RAM). Two quirks: a single control register whose meaning is
//! selected by address bit 8, and RAM cells that store only the low nibble.

use crate::cartridge::header::ROM_BANK_SIZE;
use super::{is_ram_enable_value, MbcImpl};

/// The built-in RAM is 512 half-bytes; it echoes across the whole
/// `0xA000-0xBFFF` window, so the useful address bits are the low 9.
const RAM_MASK: usize = 0x01FF;

/// MBC2 register state.
#[derive(Debug)]
pub(in crate::cartridge) struct Mbc2 {
    ram_enabled: bool,
    /// 4-bit ROM bank register; a stored `0` reads back as `1`.
    rom_bank: u8,
}

impl Mbc2 {
    pub(super) fn new() -> Mbc2 {
        Mbc2 {
            ram_enabled: false,
            rom_bank: 1,
        }
    }
}

impl MbcImpl for Mbc2 {
    fn rom_offset(&self, addr: u16) -> usize {
        match addr {
            0x0000..=0x3FFF => addr as usize,
            _ => self.rom_bank as usize * ROM_BANK_SIZE + (addr as usize - 0x4000),
        }
    }

    fn write_control(&mut self, addr: u16, value: u8) {
        // Only the 0x0000-0x3FFF range holds registers; address bit 8 selects
        // which: clear = RAM enable, set = ROM bank.
        if addr < 0x4000 {
            if addr & 0x0100 == 0 {
                self.ram_enabled = is_ram_enable_value(value);
            } else {
                let bank = value & 0x0F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            tracing::trace!(
                addr = format_args!("{addr:#06x}"),
                value = format_args!("{value:#04x}"),
                rom_bank = self.rom_bank,
                ram_enabled = self.ram_enabled,
                "mbc2 control write"
            );
        }
    }

    fn ram_read(&self, ram: &[u8], addr: u16) -> u8 {
        if !self.ram_enabled || ram.is_empty() {
            return 0xFF;
        }
        let off = (addr as usize - 0xA000) & RAM_MASK;
        // Only the low nibble is real; the high nibble reads as 1s.
        0xF0 | (ram[off] & 0x0F)
    }

    fn ram_write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        if !self.ram_enabled || ram.is_empty() {
            return;
        }
        let off = (addr as usize - 0xA000) & RAM_MASK;
        ram[off] = value & 0x0F;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_bit_8_discriminates_registers() {
        let mut m = Mbc2::new();
        // Bit 8 clear -> RAM enable register.
        m.write_control(0x0000, 0x0A);
        assert!(m.ram_enabled);
        // Bit 8 set -> ROM bank register (RAM enable unchanged).
        m.write_control(0x0100, 0x05);
        assert_eq!(m.rom_bank, 5);
        assert!(m.ram_enabled);
    }

    #[test]
    fn rom_bank_zero_remaps_to_one() {
        let mut m = Mbc2::new();
        m.write_control(0x0100, 0x00);
        assert_eq!(m.rom_offset(0x4000) / ROM_BANK_SIZE, 1);
    }

    #[test]
    fn ram_stores_low_nibble_and_echoes() {
        let mut m = Mbc2::new();
        let mut ram = vec![0u8; 512];
        m.write_control(0x0000, 0x0A); // enable
        m.ram_write(&mut ram, 0xA000, 0xAB);
        // High nibble discarded; reads back with high nibble set.
        assert_eq!(m.ram_read(&ram, 0xA000), 0xFB);
        // Echo: 0xA200 maps to the same cell as 0xA000 (offset & 0x1FF).
        assert_eq!(m.ram_read(&ram, 0xA000 + 0x0200), 0xFB);
    }

    #[test]
    fn ram_disabled_reads_open_bus() {
        let m = Mbc2::new();
        let ram = vec![0u8; 512];
        assert_eq!(m.ram_read(&ram, 0xA000), 0xFF);
    }
}

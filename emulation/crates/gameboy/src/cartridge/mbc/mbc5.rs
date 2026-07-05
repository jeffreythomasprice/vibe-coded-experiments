//! MBC5: types `0x19`-`0x1E`. Up to 8 MiB ROM and 128 KiB RAM; the mapper many
//! late DMG and CGB titles require. Unlike MBC1/3 there is no `0 -> 1` bank
//! remap: ROM bank 0 is genuinely selectable at `0x4000-0x7FFF`.

use crate::cartridge::header::ROM_BANK_SIZE;
use super::{is_ram_enable_value, simple_ram_read, simple_ram_write, MbcImpl};

/// MBC5 register state. The ROM bank is a full 9 bits, split across a low-8-bit
/// register (`0x2000-0x2FFF`) and a single 9th bit (`0x3000-0x3FFF`).
#[derive(Debug)]
pub(in crate::cartridge) struct Mbc5 {
    ram_enabled: bool,
    rom_bank: u16,
    ram_bank: u8,
    rumble: bool,
}

impl Mbc5 {
    pub(super) fn new() -> Mbc5 {
        Mbc5 {
            ram_enabled: false,
            rom_bank: 1,
            ram_bank: 0,
            rumble: false,
        }
    }
}

impl MbcImpl for Mbc5 {
    fn rom_offset(&self, addr: u16) -> usize {
        match addr {
            0x0000..=0x3FFF => addr as usize,
            _ => self.rom_bank as usize * ROM_BANK_SIZE + (addr as usize - 0x4000),
        }
    }

    fn write_control(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = is_ram_enable_value(value),
            0x2000..=0x2FFF => self.rom_bank = (self.rom_bank & 0x0100) | value as u16,
            0x3000..=0x3FFF => {
                self.rom_bank = (self.rom_bank & 0x00FF) | ((value as u16 & 0x01) << 8);
            }
            0x4000..=0x5FFF => {
                self.ram_bank = value & 0x0F;
                self.rumble = value & 0x08 != 0;
            }
            _ => {}
        }
        tracing::trace!(
            addr = format_args!("{addr:#06x}"),
            value = format_args!("{value:#04x}"),
            rom_bank = self.rom_bank,
            ram_bank = self.ram_bank,
            ram_enabled = self.ram_enabled,
            rumble = self.rumble,
            "mbc5 control write"
        );
    }

    fn ram_read(&self, ram: &[u8], addr: u16) -> u8 {
        simple_ram_read(ram, self.ram_enabled, self.ram_bank as usize, addr)
    }

    fn ram_write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        simple_ram_write(ram, self.ram_enabled, self.ram_bank as usize, addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn high_bank(m: &Mbc5) -> usize {
        m.rom_offset(0x4000) / ROM_BANK_SIZE
    }

    #[test]
    fn nine_bit_rom_bank() {
        let mut m = Mbc5::new();
        m.write_control(0x2000, 0xFF); // low 8 bits
        m.write_control(0x3000, 0x01); // bit 9
        assert_eq!(high_bank(&m), 0x1FF);
    }

    #[test]
    fn bank_zero_is_selectable() {
        let mut m = Mbc5::new();
        m.write_control(0x2000, 0x00);
        m.write_control(0x3000, 0x00);
        assert_eq!(high_bank(&m), 0);
    }

    #[test]
    fn ram_bank_switching() {
        let mut m = Mbc5::new();
        let mut ram = vec![0u8; 128 * 1024];
        m.write_control(0x0000, 0x0A); // enable
        m.write_control(0x4000, 0x00);
        m.ram_write(&mut ram, 0xA000, 0xAA);
        m.write_control(0x4000, 0x0F);
        m.ram_write(&mut ram, 0xA000, 0xBB);
        assert_eq!(ram[0], 0xAA);
        assert_eq!(ram[0x0F * 0x2000], 0xBB);
    }

    #[test]
    fn rumble_bit_is_exposed() {
        let mut m = Mbc5::new();
        m.write_control(0x4000, 0x08);
        assert!(m.rumble);
        m.write_control(0x4000, 0x00);
        assert!(!m.rumble);
    }
}

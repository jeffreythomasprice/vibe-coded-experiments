//! MBC6: type `0x20`. Used by exactly one game (Net de Get). It splits the
//! `0x4000-0x7FFF` window into two independently banked halves and adds
//! addressable flash memory.
//!
//! Experimental / best-effort: we model a single linear ROM bank in the
//! switchable window plus banked SRAM. The dual-half split and flash interface
//! are not emulated.

use crate::cartridge::header::ROM_BANK_SIZE;
use super::{is_ram_enable_value, simple_ram_read, simple_ram_write, MbcImpl};

#[derive(Debug)]
pub(in crate::cartridge) struct Mbc6 {
    ram_enabled: bool,
    rom_bank: u16,
    ram_bank: u8,
}

impl Mbc6 {
    pub(super) fn new() -> Mbc6 {
        Mbc6 {
            ram_enabled: false,
            rom_bank: 1,
            ram_bank: 0,
        }
    }
}

impl MbcImpl for Mbc6 {
    fn rom_offset(&self, addr: u16) -> usize {
        match addr {
            0x0000..=0x3FFF => addr as usize,
            _ => self.rom_bank as usize * ROM_BANK_SIZE + (addr as usize - 0x4000),
        }
    }

    fn write_control(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x03FF => self.ram_enabled = is_ram_enable_value(value),
            0x2000..=0x3FFF => self.rom_bank = value as u16,
            0x0400..=0x07FF => self.ram_bank = value & 0x07,
            _ => {}
        }
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

    #[test]
    fn basic_rom_banking() {
        let mut m = Mbc6::new();
        m.write_control(0x2000, 0x10);
        assert_eq!(m.rom_offset(0x4000) / ROM_BANK_SIZE, 0x10);
    }
}

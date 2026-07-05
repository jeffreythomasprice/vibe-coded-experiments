//! MMM01: types `0x0B`-`0x0D`. A multi-game meta-mapper that boots a menu and
//! then reconfigures itself to present one embedded game as an MBC1 image.
//!
//! Experimental / best-effort: we model the post-configuration steady state as
//! plain MBC1-style banking, which is enough to read ROM and RAM linearly. The
//! menu/reconfiguration handshake is not emulated.

use crate::cartridge::header::ROM_BANK_SIZE;
use super::{is_ram_enable_value, simple_ram_read, simple_ram_write, MbcImpl};

#[derive(Debug)]
pub(in crate::cartridge) struct Mmm01 {
    ram_enabled: bool,
    rom_bank: u8,
    ram_bank: u8,
}

impl Mmm01 {
    pub(super) fn new() -> Mmm01 {
        Mmm01 {
            ram_enabled: false,
            rom_bank: 1,
            ram_bank: 0,
        }
    }
}

impl MbcImpl for Mmm01 {
    fn rom_offset(&self, addr: u16) -> usize {
        match addr {
            0x0000..=0x3FFF => addr as usize,
            _ => self.rom_bank as usize * ROM_BANK_SIZE + (addr as usize - 0x4000),
        }
    }

    fn write_control(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = is_ram_enable_value(value),
            0x2000..=0x3FFF => {
                let bank = value & 0x1F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => self.ram_bank = value & 0x03,
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
    fn basic_rom_and_ram_banking() {
        let mut m = Mmm01::new();
        m.write_control(0x2000, 0x03);
        assert_eq!(m.rom_offset(0x4000) / ROM_BANK_SIZE, 3);

        let mut ram = vec![0u8; 32 * 1024];
        m.write_control(0x0000, 0x0A);
        m.write_control(0x4000, 0x01);
        m.ram_write(&mut ram, 0xA000, 0x7E);
        assert_eq!(ram[0x2000], 0x7E);
    }
}

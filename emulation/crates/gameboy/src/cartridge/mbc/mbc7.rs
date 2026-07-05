//! MBC7: type `0x22`. Adds a two-axis tilt accelerometer and a serial EEPROM,
//! both accessed through registers in the `0xA000-0xBFFF` window rather than
//! plain RAM (used by Kirby Tilt 'n' Tumble, Command Master).
//!
//! Experimental / best-effort: ROM banking is modeled as MBC5-style. The
//! accelerometer is stubbed to its centered rest value (`0x81D0` on hardware,
//! i.e. `0x8000` bias + no tilt) and the EEPROM is not emulated, so tilt games
//! will see a level, motionless controller.

use crate::cartridge::header::ROM_BANK_SIZE;
use super::{is_ram_enable_value, MbcImpl};

/// The accelerometer rest reading (centered, no tilt) on real hardware.
const ACCEL_CENTER: u16 = 0x81D0;

#[derive(Debug)]
pub(in crate::cartridge) struct Mbc7 {
    ram_enabled: bool,
    rom_bank: u16,
}

impl Mbc7 {
    pub(super) fn new() -> Mbc7 {
        Mbc7 {
            ram_enabled: false,
            rom_bank: 1,
        }
    }
}

impl MbcImpl for Mbc7 {
    fn rom_offset(&self, addr: u16) -> usize {
        match addr {
            0x0000..=0x3FFF => addr as usize,
            _ => self.rom_bank as usize * ROM_BANK_SIZE + (addr as usize - 0x4000),
        }
    }

    fn write_control(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = is_ram_enable_value(value),
            0x2000..=0x3FFF => self.rom_bank = value as u16,
            _ => {}
        }
    }

    fn ram_read(&self, _ram: &[u8], addr: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }
        // The register file lives at 0xA000-0xAFFF; the accelerometer X/Y latches
        // sit at 0xA020-0xA030 (low/high bytes). Report the centered rest value;
        // everything else reads as open bus.
        match addr {
            0xA020 | 0xA040 => (ACCEL_CENTER & 0x00FF) as u8,
            0xA030 | 0xA050 => (ACCEL_CENTER >> 8) as u8,
            _ => 0xFF,
        }
    }

    fn ram_write(&mut self, _ram: &mut [u8], _addr: u16, _value: u8) {
        // Accelerometer latch / EEPROM writes are not emulated.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rom_banks_like_mbc5() {
        let mut m = Mbc7::new();
        m.write_control(0x2000, 0x0A);
        assert_eq!(m.rom_offset(0x4000) / ROM_BANK_SIZE, 0x0A);
    }

    #[test]
    fn accelerometer_reads_centered() {
        let mut m = Mbc7::new();
        m.write_control(0x0000, 0x0A); // enable register access
        assert_eq!(m.ram_read(&[], 0xA030), (ACCEL_CENTER >> 8) as u8);
        assert_eq!(m.ram_read(&[], 0xA020), (ACCEL_CENTER & 0xFF) as u8);
    }
}

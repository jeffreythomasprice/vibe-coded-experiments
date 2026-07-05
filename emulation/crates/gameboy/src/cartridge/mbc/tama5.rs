//! TAMA5: type `0xFD`. A Bandai mapper used by the Game Boy Tamagotchi titles,
//! driven through a command/argument register protocol with an on-board RTC.
//!
//! Experimental / best-effort: ROM banking is modeled as a simple linear bank
//! register. The command-register protocol, RTC, and nibble-wide RAM interface
//! are not emulated; the RAM window reads as open bus. This will not run the
//! Tamagotchi games correctly, but it loads and maps ROM without panicking.

use crate::cartridge::header::ROM_BANK_SIZE;
use super::MbcImpl;

#[derive(Debug)]
pub(in crate::cartridge) struct Tama5 {
    rom_bank: u8,
}

impl Tama5 {
    pub(super) fn new() -> Tama5 {
        Tama5 { rom_bank: 1 }
    }
}

impl MbcImpl for Tama5 {
    fn rom_offset(&self, addr: u16) -> usize {
        match addr {
            0x0000..=0x3FFF => addr as usize,
            _ => self.rom_bank as usize * ROM_BANK_SIZE + (addr as usize - 0x4000),
        }
    }

    fn write_control(&mut self, addr: u16, value: u8) {
        // The real chip multiplexes everything through 0xA000-0xBFFF command
        // registers; the ROM-space writes we can meaningfully honor are the bank
        // register in the usual 0x2000-0x3FFF range.
        if (0x2000..=0x3FFF).contains(&addr) {
            let bank = value & 0x1F;
            self.rom_bank = if bank == 0 { 1 } else { bank };
        }
    }

    fn ram_read(&self, _ram: &[u8], _addr: u16) -> u8 {
        0xFF
    }

    fn ram_write(&mut self, _ram: &mut [u8], _addr: u16, _value: u8) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rom_banking() {
        let mut m = Tama5::new();
        m.write_control(0x2000, 0x04);
        assert_eq!(m.rom_offset(0x4000) / ROM_BANK_SIZE, 4);
    }
}

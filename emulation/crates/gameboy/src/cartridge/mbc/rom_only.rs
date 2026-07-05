//! No mapper: types `0x00` (ROM only), `0x08`/`0x09` (ROM + optional RAM).

use super::{simple_ram_read, simple_ram_write, MbcImpl};

/// A cartridge with no bank controller. ROM is straight-mapped across
/// `0x0000-0x7FFF`; optional external RAM sits at `0xA000-0xBFFF` with no enable
/// register.
#[derive(Debug, Default)]
pub(in crate::cartridge) struct RomOnly;

impl RomOnly {
    pub(super) fn new() -> RomOnly {
        RomOnly
    }
}

impl MbcImpl for RomOnly {
    fn rom_offset(&self, addr: u16) -> usize {
        addr as usize
    }

    fn write_control(&mut self, _addr: u16, _value: u8) {}

    fn ram_read(&self, ram: &[u8], addr: u16) -> u8 {
        simple_ram_read(ram, true, 0, addr)
    }

    fn ram_write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        simple_ram_write(ram, true, 0, addr, value);
    }
}

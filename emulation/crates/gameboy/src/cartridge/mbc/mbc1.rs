//! MBC1: types `0x01`-`0x03`. Up to 2 MiB ROM and/or 32 KiB RAM.

use crate::cartridge::header::ROM_BANK_SIZE;
use super::{is_ram_enable_value, simple_ram_read, simple_ram_write, MbcImpl};

/// MBC1 register state.
///
/// The banking number is split across two registers: a 5-bit low value written
/// to `0x2000-0x3FFF` and a 2-bit high value written to `0x4000-0x5FFF`. The
/// high bits act as ROM bank bits 5-6 in ROM-banking mode, or as the RAM bank
/// number in RAM-banking mode.
#[derive(Debug)]
pub(in crate::cartridge) struct Mbc1 {
    ram_enabled: bool,
    /// 5-bit low bank register; hardware forces a stored `0` up to `1`.
    bank_lo: u8,
    /// 2-bit high register (ROM bank bits 5-6, or RAM bank).
    bank_hi: u8,
    /// `false` = ROM-banking mode (0), `true` = RAM-banking mode (1).
    ram_banking_mode: bool,
}

impl Mbc1 {
    pub(super) fn new() -> Mbc1 {
        Mbc1 {
            ram_enabled: false,
            bank_lo: 1,
            bank_hi: 0,
            ram_banking_mode: false,
        }
    }

    /// The full ROM bank selected for the switchable `0x4000-0x7FFF` region.
    fn high_bank(&self) -> usize {
        ((self.bank_hi << 5) | self.bank_lo) as usize
    }

    /// The ROM bank mapped at the "fixed" `0x0000-0x3FFF` region. Normally bank
    /// 0, but in RAM-banking mode on a large ROM the high bits shift the low
    /// region too.
    fn low_bank(&self) -> usize {
        if self.ram_banking_mode {
            (self.bank_hi << 5) as usize
        } else {
            0
        }
    }

    fn ram_bank(&self) -> usize {
        if self.ram_banking_mode {
            self.bank_hi as usize
        } else {
            0
        }
    }
}

impl MbcImpl for Mbc1 {
    fn rom_offset(&self, addr: u16) -> usize {
        match addr {
            0x0000..=0x3FFF => self.low_bank() * ROM_BANK_SIZE + addr as usize,
            _ => self.high_bank() * ROM_BANK_SIZE + (addr as usize - 0x4000),
        }
    }

    fn write_control(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_enabled = is_ram_enable_value(value),
            0x2000..=0x3FFF => {
                let lo = value & 0x1F;
                self.bank_lo = if lo == 0 { 1 } else { lo };
            }
            0x4000..=0x5FFF => self.bank_hi = value & 0x03,
            0x6000..=0x7FFF => self.ram_banking_mode = value & 0x01 != 0,
            _ => {}
        }
        tracing::trace!(
            addr = format_args!("{addr:#06x}"),
            value = format_args!("{value:#04x}"),
            rom_bank = self.high_bank(),
            ram_bank = self.ram_bank(),
            ram_enabled = self.ram_enabled,
            mode = self.ram_banking_mode,
            "mbc1 control write"
        );
    }

    fn ram_read(&self, ram: &[u8], addr: u16) -> u8 {
        simple_ram_read(ram, self.ram_enabled, self.ram_bank(), addr)
    }

    fn ram_write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        simple_ram_write(ram, self.ram_enabled, self.ram_bank(), addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bank_at_4000(m: &Mbc1) -> usize {
        m.rom_offset(0x4000) / ROM_BANK_SIZE
    }

    #[test]
    fn low_region_is_bank_zero_by_default() {
        let m = Mbc1::new();
        assert_eq!(m.rom_offset(0x0000), 0);
        assert_eq!(m.rom_offset(0x3FFF), 0x3FFF);
    }

    #[test]
    fn bank_zero_remaps_to_one() {
        let mut m = Mbc1::new();
        m.write_control(0x2000, 0x00);
        assert_eq!(bank_at_4000(&m), 1);
    }

    #[test]
    fn bank_aliasing_20_40_60() {
        // Selecting "bank 0x20" (bank_hi=1, bank_lo=0) lands on 0x21 because the
        // low register can never hold 0.
        let mut m = Mbc1::new();
        m.write_control(0x4000, 0x01); // bank_hi = 1
        m.write_control(0x2000, 0x00); // bank_lo forced 0 -> 1
        assert_eq!(bank_at_4000(&m), 0x21);
    }

    #[test]
    fn high_bits_combine_into_bank() {
        let mut m = Mbc1::new();
        m.write_control(0x2000, 0x05); // bank_lo = 5
        m.write_control(0x4000, 0x02); // bank_hi = 2 -> bits 5-6
        assert_eq!(bank_at_4000(&m), (2 << 5) | 5);
    }

    #[test]
    fn mode_one_remaps_low_region_and_ram_bank() {
        let mut m = Mbc1::new();
        m.write_control(0x4000, 0x01); // bank_hi = 1
        m.write_control(0x6000, 0x01); // RAM-banking mode
        assert_eq!(m.rom_offset(0x0000), 0x20 * ROM_BANK_SIZE);
        assert_eq!(m.ram_bank(), 1);

        m.write_control(0x6000, 0x00); // back to ROM-banking mode
        assert_eq!(m.rom_offset(0x0000), 0);
        assert_eq!(m.ram_bank(), 0);
    }

    #[test]
    fn ram_enable_gates_access() {
        let mut m = Mbc1::new();
        let mut ram = vec![0u8; 8 * 1024];
        m.ram_write(&mut ram, 0xA000, 0x42); // disabled: dropped
        assert_eq!(m.ram_read(&ram, 0xA000), 0xFF);

        m.write_control(0x0000, 0x0A); // enable
        m.ram_write(&mut ram, 0xA000, 0x42);
        assert_eq!(m.ram_read(&ram, 0xA000), 0x42);

        m.write_control(0x0000, 0x00); // disable again
        assert_eq!(m.ram_read(&ram, 0xA000), 0xFF);
    }

    #[test]
    fn ram_bank_switching_in_mode_one() {
        let mut m = Mbc1::new();
        let mut ram = vec![0u8; 32 * 1024];
        m.write_control(0x0000, 0x0A); // enable
        m.write_control(0x6000, 0x01); // RAM-banking mode

        m.write_control(0x4000, 0x00);
        m.ram_write(&mut ram, 0xA000, 0x10);
        m.write_control(0x4000, 0x02);
        m.ram_write(&mut ram, 0xA000, 0x20);

        assert_eq!(ram[0], 0x10);
        assert_eq!(ram[2 * 0x2000], 0x20);
    }
}

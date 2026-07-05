//! MBC3: types `0x0F`-`0x13`. Up to 2 MiB ROM and/or 32 KiB RAM, plus an
//! optional real-time clock. Like MBC1 but with a full 7-bit ROM bank (no
//! forbidden banks) and an RTC mapped over the RAM window.

use crate::cartridge::header::ROM_BANK_SIZE;
use super::rtc::{Rtc, REG_DAY_HIGH, REG_SECONDS};
use super::{is_ram_enable_value, simple_ram_read, simple_ram_write, MbcImpl};

/// MBC3 register state.
#[derive(Debug)]
pub(in crate::cartridge) struct Mbc3 {
    /// Gates both external RAM and the RTC registers.
    ram_rtc_enabled: bool,
    /// 7-bit ROM bank; a stored `0` reads back as `1`.
    rom_bank: u8,
    /// The `0x4000-0x5FFF` selector: `0x00-0x03` picks a RAM bank, `0x08-0x0C`
    /// picks an RTC register.
    map_select: u8,
    has_timer: bool,
    rtc: Rtc,
}

impl Mbc3 {
    pub(super) fn new(has_timer: bool) -> Mbc3 {
        Mbc3 {
            ram_rtc_enabled: false,
            rom_bank: 1,
            map_select: 0,
            has_timer,
            rtc: Rtc::new(),
        }
    }

    /// Whether the current selector maps an RTC register (only when the cart has
    /// a timer).
    fn rtc_selected(&self) -> bool {
        self.has_timer && (REG_SECONDS..=REG_DAY_HIGH).contains(&self.map_select)
    }

    /// The RTC state to append to the save file, or `None` when this cart has no
    /// timer (nothing to persist).
    pub(in crate::cartridge) fn rtc_bytes(&self) -> Option<Vec<u8>> {
        self.has_timer.then(|| self.rtc.to_bytes())
    }

    /// Restore RTC state from a save trailer. Ignored (with a warning) for a
    /// non-timer cart or an unparsable trailer.
    pub(in crate::cartridge) fn load_rtc(&mut self, data: &[u8]) {
        if !self.has_timer {
            return;
        }
        match Rtc::from_bytes(data) {
            Some(rtc) => self.rtc = rtc,
            None => tracing::warn!(got = data.len(), "ignoring malformed rtc save trailer"),
        }
    }
}

impl MbcImpl for Mbc3 {
    fn rom_offset(&self, addr: u16) -> usize {
        match addr {
            0x0000..=0x3FFF => addr as usize,
            _ => self.rom_bank as usize * ROM_BANK_SIZE + (addr as usize - 0x4000),
        }
    }

    fn write_control(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ram_rtc_enabled = is_ram_enable_value(value),
            0x2000..=0x3FFF => {
                let bank = value & 0x7F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => self.map_select = value,
            0x6000..=0x7FFF => self.rtc.write_latch(value),
            _ => {}
        }
        tracing::trace!(
            addr = format_args!("{addr:#06x}"),
            value = format_args!("{value:#04x}"),
            rom_bank = self.rom_bank,
            map_select = format_args!("{:#04x}", self.map_select),
            ram_rtc_enabled = self.ram_rtc_enabled,
            "mbc3 control write"
        );
    }

    fn ram_read(&self, ram: &[u8], addr: u16) -> u8 {
        if !self.ram_rtc_enabled {
            return 0xFF;
        }
        if self.rtc_selected() {
            self.rtc.read(self.map_select)
        } else {
            simple_ram_read(ram, true, self.map_select as usize, addr)
        }
    }

    fn ram_write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        if !self.ram_rtc_enabled {
            return;
        }
        if self.rtc_selected() {
            self.rtc.write(self.map_select, value);
        } else {
            simple_ram_write(ram, true, self.map_select as usize, addr, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_bit_rom_bank_no_forbidden_banks() {
        let mut m = Mbc3::new(false);
        m.write_control(0x2000, 0x20); // 0x20 is reachable (unlike MBC1)
        assert_eq!(m.rom_offset(0x4000) / ROM_BANK_SIZE, 0x20);
        m.write_control(0x2000, 0x00); // 0 -> 1
        assert_eq!(m.rom_offset(0x4000) / ROM_BANK_SIZE, 1);
    }

    #[test]
    fn ram_bank_vs_rtc_select() {
        let mut m = Mbc3::new(true);
        let mut ram = vec![0u8; 32 * 1024];
        m.write_control(0x0000, 0x0A); // enable

        // Select RAM bank 2, write there.
        m.write_control(0x4000, 0x02);
        m.ram_write(&mut ram, 0xA000, 0x55);
        assert_eq!(ram[2 * 0x2000], 0x55);
        assert_eq!(m.ram_read(&ram, 0xA000), 0x55);

        // Select an RTC register: RAM is no longer visible there.
        m.write_control(0x4000, REG_SECONDS);
        assert_ne!(m.ram_read(&ram, 0xA000), 0x55);
    }

    #[test]
    fn rtc_registers_ignored_without_timer() {
        let mut m = Mbc3::new(false);
        let ram = vec![0u8; 32 * 1024];
        m.write_control(0x0000, 0x0A);
        m.write_control(0x4000, REG_SECONDS); // no timer: treated as RAM bank 8
        // Out-of-range RAM bank wraps rather than hitting the RTC.
        let _ = m.ram_read(&ram, 0xA000);
        assert!(!m.rtc_selected());
    }

    #[test]
    fn ram_rtc_gated_by_enable() {
        let mut m = Mbc3::new(true);
        let mut ram = vec![0u8; 32 * 1024];
        m.ram_write(&mut ram, 0xA000, 0x99); // disabled: dropped
        assert_eq!(m.ram_read(&ram, 0xA000), 0xFF);
    }
}

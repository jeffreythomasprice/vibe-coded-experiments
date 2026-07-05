//! HuC1: type `0xFF`. A Hudson mapper very similar to MBC1, with an added
//! infrared LED transceiver mapped over the RAM window.
//!
//! Experimental / best-effort: ROM/RAM banking is modeled as MBC1. The IR
//! register is stubbed — reads return `0xC1` (no IR light detected), which the
//! few HuC1 titles tolerate.

use crate::cartridge::header::ROM_BANK_SIZE;
use super::{simple_ram_read, simple_ram_write, MbcImpl};

/// HuC1 uses `0x0E` (not `0x0A`) in the RAM-enable register to switch the RAM
/// window into IR mode.
const IR_MODE: u8 = 0x0E;
/// Value returned when reading the IR register with no incoming light.
const IR_NO_SIGNAL: u8 = 0xC1;

#[derive(Debug)]
pub(in crate::cartridge) struct HuC1 {
    ir_mode: bool,
    rom_bank: u8,
    ram_bank: u8,
}

impl HuC1 {
    pub(super) fn new() -> HuC1 {
        HuC1 {
            ir_mode: false,
            rom_bank: 1,
            ram_bank: 0,
        }
    }
}

impl MbcImpl for HuC1 {
    fn rom_offset(&self, addr: u16) -> usize {
        match addr {
            0x0000..=0x3FFF => addr as usize,
            _ => self.rom_bank as usize * ROM_BANK_SIZE + (addr as usize - 0x4000),
        }
    }

    fn write_control(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.ir_mode = value & 0x0F == IR_MODE,
            0x2000..=0x3FFF => {
                let bank = value & 0x3F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => self.ram_bank = value & 0x03,
            _ => {}
        }
    }

    fn ram_read(&self, ram: &[u8], addr: u16) -> u8 {
        if self.ir_mode {
            return IR_NO_SIGNAL;
        }
        // When not in IR mode HuC1 RAM behaves as always-enabled banked RAM.
        simple_ram_read(ram, true, self.ram_bank as usize, addr)
    }

    fn ram_write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        if self.ir_mode {
            return;
        }
        simple_ram_write(ram, true, self.ram_bank as usize, addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ir_register_reads_no_signal() {
        let mut m = HuC1::new();
        let ram = vec![0u8; 8 * 1024];
        m.write_control(0x0000, IR_MODE);
        assert_eq!(m.ram_read(&ram, 0xA000), IR_NO_SIGNAL);
    }

    #[test]
    fn ram_banking_when_not_in_ir_mode() {
        let mut m = HuC1::new();
        let mut ram = vec![0u8; 8 * 1024];
        m.ram_write(&mut ram, 0xA000, 0x42);
        assert_eq!(m.ram_read(&ram, 0xA000), 0x42);
    }
}

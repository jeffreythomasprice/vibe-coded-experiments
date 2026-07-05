//! Game Boy / Pocket Camera: type `0xFC`. An MBC3-like mapper plus a M64282FP
//! image sensor whose control and capture registers overlay RAM bank `0x10`.
//!
//! Experimental / best-effort: ROM/RAM banking is modeled linearly. The camera
//! registers are stubbed — the capture-trigger bit always reads back as
//! "capture complete", so software that polls it won't hang, but no image data
//! is produced.

use crate::cartridge::header::ROM_BANK_SIZE;
use super::{is_ram_enable_value, simple_ram_read, simple_ram_write, MbcImpl};

/// RAM bank value that selects the camera register file instead of SRAM.
const CAMERA_REGS_BANK: u8 = 0x10;
/// Offset of the capture-trigger register within the camera register file.
const TRIGGER_REG: u16 = 0xA000;

#[derive(Debug)]
pub(in crate::cartridge) struct Camera {
    ram_enabled: bool,
    rom_bank: u8,
    ram_bank: u8,
}

impl Camera {
    pub(super) fn new() -> Camera {
        Camera {
            ram_enabled: false,
            rom_bank: 1,
            ram_bank: 0,
        }
    }

    fn camera_selected(&self) -> bool {
        self.ram_bank & CAMERA_REGS_BANK != 0
    }
}

impl MbcImpl for Camera {
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
                let bank = value & 0x3F;
                self.rom_bank = if bank == 0 { 1 } else { bank };
            }
            0x4000..=0x5FFF => self.ram_bank = value & 0x1F,
            _ => {}
        }
    }

    fn ram_read(&self, ram: &[u8], addr: u16) -> u8 {
        if !self.ram_enabled {
            return 0xFF;
        }
        if self.camera_selected() {
            // Trigger register: bit 0 clear means "capture finished / idle".
            if addr == TRIGGER_REG {
                return 0x00;
            }
            return 0x00;
        }
        simple_ram_read(ram, true, self.ram_bank as usize, addr)
    }

    fn ram_write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        if !self.ram_enabled {
            return;
        }
        if self.camera_selected() {
            // Camera register writes are accepted but not acted upon.
            return;
        }
        simple_ram_write(ram, true, self.ram_bank as usize, addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_banking_and_camera_register_stub() {
        let mut m = Camera::new();
        let mut ram = vec![0u8; 128 * 1024];
        m.write_control(0x0000, 0x0A); // enable

        // Normal RAM bank.
        m.write_control(0x4000, 0x02);
        m.ram_write(&mut ram, 0xA000, 0x77);
        assert_eq!(m.ram_read(&ram, 0xA000), 0x77);

        // Camera register bank: trigger reads back as idle/complete.
        m.write_control(0x4000, CAMERA_REGS_BANK);
        assert_eq!(m.ram_read(&ram, TRIGGER_REG), 0x00);
    }
}

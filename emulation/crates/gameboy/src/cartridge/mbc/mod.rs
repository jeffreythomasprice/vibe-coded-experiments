//! Memory Bank Controllers.
//!
//! Each MBC is a small state machine holding only its registers (current ROM
//! bank, RAM bank, enable flags, mode, RTC, …). It never owns the ROM/RAM
//! byte buffers — [`Cartridge`](crate::cartridge::Cartridge) does — so an MBC's
//! whole job is to translate a CPU address into a flat ROM offset and to service
//! the `0xA000-0xBFFF` RAM/register region.
//!
//! Dispatch is a plain `enum` + `match` rather than `Box<dyn>`: the mapper set
//! is closed and known at compile time, this is the hottest path in the
//! emulator (every memory access), and an enum stays trivially `Debug`/`Clone`
//! for future save states.

mod camera;
mod huc1;
mod huc3;
mod mbc1;
mod mbc2;
mod mbc3;
mod mbc5;
mod mbc6;
mod mbc7;
mod mmm01;
mod rom_only;
mod rtc;
mod tama5;

use super::header::{CartridgeHeader, Mapper, RAM_BANK_SIZE};

pub(super) use rtc::RTC_SAVE_LEN;

use camera::Camera;
use huc1::HuC1;
use huc3::HuC3;
use mbc1::Mbc1;
use mbc2::Mbc2;
use mbc3::Mbc3;
use mbc5::Mbc5;
use mbc6::Mbc6;
use mbc7::Mbc7;
use mmm01::Mmm01;
use rom_only::RomOnly;
use tama5::Tama5;

/// The behavior every mapper implements. Kept crate-private: only [`Mbc`] and
/// [`Cartridge`](crate::cartridge::Cartridge) interact with mappers.
pub(super) trait MbcImpl: std::fmt::Debug {
    /// Translate a ROM-region address (`0x0000-0x7FFF`) to a flat ROM offset.
    /// Bounds/wrapping against the real ROM length is applied by the caller.
    fn rom_offset(&self, addr: u16) -> usize;

    /// Handle a write into the control region (`0x0000-0x7FFF`): update registers.
    fn write_control(&mut self, addr: u16, value: u8);

    /// Read the RAM/register region (`0xA000-0xBFFF`).
    fn ram_read(&self, ram: &[u8], addr: u16) -> u8;

    /// Write the RAM/register region (`0xA000-0xBFFF`).
    fn ram_write(&mut self, ram: &mut [u8], addr: u16, value: u8);
}

/// A concrete memory bank controller.
#[derive(Debug)]
pub(super) enum Mbc {
    RomOnly(RomOnly),
    Mbc1(Mbc1),
    Mbc2(Mbc2),
    Mbc3(Mbc3),
    Mbc5(Mbc5),
    Mmm01(Mmm01),
    HuC1(HuC1),
    HuC3(HuC3),
    Mbc6(Mbc6),
    Mbc7(Mbc7),
    Tama5(Tama5),
    Camera(Camera),
}

/// Forward a method call to whichever inner mapper the enum holds.
macro_rules! dispatch {
    ($self:expr, $inner:ident => $body:expr) => {
        match $self {
            Mbc::RomOnly($inner) => $body,
            Mbc::Mbc1($inner) => $body,
            Mbc::Mbc2($inner) => $body,
            Mbc::Mbc3($inner) => $body,
            Mbc::Mbc5($inner) => $body,
            Mbc::Mmm01($inner) => $body,
            Mbc::HuC1($inner) => $body,
            Mbc::HuC3($inner) => $body,
            Mbc::Mbc6($inner) => $body,
            Mbc::Mbc7($inner) => $body,
            Mbc::Tama5($inner) => $body,
            Mbc::Camera($inner) => $body,
        }
    };
}

impl Mbc {
    /// Choose and construct the mapper named by the header, logging the choice.
    pub(super) fn from_header(header: &CartridgeHeader) -> Mbc {
        let ct = &header.cartridge_type;
        tracing::info!(
            mapper = ?ct.mapper,
            title = %header.title,
            rom_banks = header.rom_banks,
            ram_banks = header.ram_banks,
            battery = ct.has_battery,
            timer = ct.has_timer,
            rumble = ct.has_rumble,
            "cartridge loaded"
        );

        let experimental = |what: &str| {
            tracing::warn!(
                mapper = ?ct.mapper,
                "{what} mapper support is experimental and unverified against hardware"
            );
        };

        match ct.mapper {
            Mapper::RomOnly => Mbc::RomOnly(RomOnly::new()),
            Mapper::Mbc1 => Mbc::Mbc1(Mbc1::new()),
            Mapper::Mbc2 => Mbc::Mbc2(Mbc2::new()),
            Mapper::Mbc3 => Mbc::Mbc3(Mbc3::new(ct.has_timer)),
            Mapper::Mbc5 => Mbc::Mbc5(Mbc5::new()),
            Mapper::Mmm01 => {
                experimental("MMM01");
                Mbc::Mmm01(Mmm01::new())
            }
            Mapper::HuC1 => {
                experimental("HuC1");
                Mbc::HuC1(HuC1::new())
            }
            Mapper::HuC3 => {
                experimental("HuC3");
                Mbc::HuC3(HuC3::new())
            }
            Mapper::Mbc6 => {
                experimental("MBC6");
                Mbc::Mbc6(Mbc6::new())
            }
            Mapper::Mbc7 => {
                experimental("MBC7");
                Mbc::Mbc7(Mbc7::new())
            }
            Mapper::Tama5 => {
                experimental("TAMA5");
                Mbc::Tama5(Tama5::new())
            }
            Mapper::PocketCamera => {
                experimental("Pocket Camera");
                Mbc::Camera(Camera::new())
            }
            Mapper::Mbc4 => {
                // Apocryphal: no known real cartridge. Treat as MBC3-like.
                experimental("MBC4 (apocryphal)");
                Mbc::Mbc3(Mbc3::new(false))
            }
            Mapper::Unknown(byte) => {
                tracing::warn!(
                    type_byte = format_args!("{byte:#04x}"),
                    "unknown cartridge type; falling back to MBC5 banking"
                );
                Mbc::Mbc5(Mbc5::new())
            }
        }
    }

    pub(super) fn rom_offset(&self, addr: u16) -> usize {
        dispatch!(self, m => m.rom_offset(addr))
    }

    pub(super) fn write_control(&mut self, addr: u16, value: u8) {
        dispatch!(self, m => m.write_control(addr, value))
    }

    pub(super) fn ram_read(&self, ram: &[u8], addr: u16) -> u8 {
        dispatch!(self, m => m.ram_read(ram, addr))
    }

    pub(super) fn ram_write(&mut self, ram: &mut [u8], addr: u16, value: u8) {
        dispatch!(self, m => m.ram_write(ram, addr, value))
    }

    /// The RTC state to persist alongside battery RAM, or `None` for a mapper
    /// without a real-time clock (everything except an MBC3 with a timer).
    pub(super) fn rtc_bytes(&self) -> Option<Vec<u8>> {
        match self {
            Mbc::Mbc3(m) => m.rtc_bytes(),
            _ => None,
        }
    }

    /// Restore RTC state read back from a save trailer. A no-op for mappers
    /// without a clock.
    pub(super) fn load_rtc(&mut self, data: &[u8]) {
        if let Mbc::Mbc3(m) = self {
            m.load_rtc(data);
        }
    }
}

/// The classic RAM-enable magic value: a write with `0x0A` in the low nibble
/// enables cartridge RAM; anything else disables it.
pub(super) fn is_ram_enable_value(value: u8) -> bool {
    value & 0x0F == 0x0A
}

/// Read from straightforwardly-banked external RAM, honoring the enable gate.
/// Returns `0xFF` (open bus) when RAM is disabled or absent.
pub(super) fn simple_ram_read(ram: &[u8], enabled: bool, bank: usize, addr: u16) -> u8 {
    if !enabled || ram.is_empty() {
        return 0xFF;
    }
    let off = bank * RAM_BANK_SIZE + (addr as usize - 0xA000);
    ram[off % ram.len()]
}

/// Write to straightforwardly-banked external RAM, honoring the enable gate.
/// A disabled or absent RAM silently drops the write.
pub(super) fn simple_ram_write(ram: &mut [u8], enabled: bool, bank: usize, addr: u16, value: u8) {
    if !enabled || ram.is_empty() {
        return;
    }
    let len = ram.len();
    let off = (bank * RAM_BANK_SIZE + (addr as usize - 0xA000)) % len;
    ram[off] = value;
}

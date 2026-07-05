//! Cartridge loading and memory-bank-controller (MBC) dispatch.
//!
//! A [`Cartridge`] owns the ROM image and (optional) external RAM as flat byte
//! buffers, plus an [`Mbc`](mbc::Mbc) that holds only the mapper's registers.
//! The MBC never owns the byte buffers; it translates CPU addresses to flat
//! offsets and services the RAM/register region, keeping each mapper a small,
//! independently testable state machine.
//!
//! The two cartridge address regions on the Game Boy bus are ROM at
//! `0x0000-0x7FFF` (writes here are MBC *commands*, not data) and external RAM
//! at `0xA000-0xBFFF`. A future memory bus routes exactly those ranges to
//! [`Cartridge::read`] / [`Cartridge::write`]; nothing else touches the MBC.

pub mod header;
mod mbc;

pub use header::{CartridgeHeader, CartridgeType, CgbFlag, Destination, Mapper};

use mbc::Mbc;

/// The ROM address region serviced by the cartridge (inclusive).
pub const ROM_REGION: std::ops::RangeInclusive<u16> = 0x0000..=0x7FFF;
/// The external-RAM address region serviced by the cartridge (inclusive).
pub const RAM_REGION: std::ops::RangeInclusive<u16> = 0xA000..=0xBFFF;

/// Errors that make a ROM image unusable.
///
/// A bad header checksum or a ROM whose real length disagrees with its declared
/// size are deliberately *not* here: those warn and load anyway (see
/// [`Cartridge::from_bytes`]).
#[derive(Debug, thiserror::Error)]
pub enum CartridgeError {
    #[error("rom too small: need at least {needed} bytes for the header, got {got}")]
    TooShort { needed: usize, got: usize },

    #[error("unknown rom size code {0:#04x}")]
    UnknownRomSize(u8),

    #[error("unknown ram size code {0:#04x}")]
    UnknownRamSize(u8),
}

/// A loaded Game Boy cartridge: ROM + external RAM + its MBC.
#[derive(Debug)]
pub struct Cartridge {
    header: CartridgeHeader,
    rom: Vec<u8>,
    ram: Vec<u8>,
    mbc: Mbc,
}

impl Cartridge {
    /// Parse a ROM image and build the appropriate MBC.
    ///
    /// The header checksum and any ROM-length / declared-size disagreement are
    /// reported at `warn` and do not fail the load — real hardware refuses such
    /// carts, but running homebrew and hand-assembled test ROMs is more useful.
    /// Only a truly unparsable image (too short, or an unrecognized ROM/RAM size
    /// code) is a hard error.
    pub fn from_bytes(rom: &[u8]) -> Result<Cartridge, CartridgeError> {
        let header = CartridgeHeader::parse(rom)?;

        if !header.header_checksum_valid() {
            tracing::warn!(
                stored = format_args!("{:#04x}", header.header_checksum),
                computed = format_args!("{:#04x}", header.computed_header_checksum),
                "header checksum mismatch; loading anyway"
            );
        }
        if rom.len() != header.rom_size_bytes {
            tracing::warn!(
                actual = rom.len(),
                declared = header.rom_size_bytes,
                "rom length disagrees with header-declared size; using actual length"
            );
        }

        let ram_len = ram_alloc_len(&header);
        let ram = vec![0u8; ram_len];
        let mbc = Mbc::from_header(&header);

        Ok(Cartridge {
            header,
            rom: rom.to_vec(),
            ram,
            mbc,
        })
    }

    /// The parsed header.
    pub fn header(&self) -> &CartridgeHeader {
        &self.header
    }

    /// Read a byte from a cartridge address (`0x0000-0x7FFF` or `0xA000-0xBFFF`).
    /// Addresses outside those regions return `0xFF` (open bus).
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => {
                if self.rom.is_empty() {
                    return 0xFF;
                }
                let offset = self.mbc.rom_offset(addr) % self.rom.len();
                self.rom[offset]
            }
            0xA000..=0xBFFF => self.mbc.ram_read(&self.ram, addr),
            _ => 0xFF,
        }
    }

    /// Write a byte to a cartridge address. Writes to the ROM region are MBC
    /// control commands; writes to the RAM region hit external RAM / registers.
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.mbc.write_control(addr, value),
            0xA000..=0xBFFF => self.mbc.ram_write(&mut self.ram, addr, value),
            _ => {}
        }
    }

    /// Whether this cartridge has battery-backed RAM worth persisting to a
    /// `.sav` file.
    pub fn has_battery(&self) -> bool {
        self.header.cartridge_type.has_battery
    }

    /// A snapshot of external RAM, suitable for writing to a save file. Empty
    /// when the cartridge has no RAM.
    pub fn save_ram(&self) -> &[u8] {
        &self.ram
    }

    /// The full battery-save blob: external RAM followed by an RTC trailer when
    /// the cartridge has a real-time clock (MBC3 + timer). The store persists
    /// this opaque byte string as the `.sav`; [`Cartridge::load_bytes`] is its
    /// inverse.
    pub fn save_bytes(&self) -> Vec<u8> {
        let mut out = self.ram.clone();
        if let Some(rtc) = self.mbc.rtc_bytes() {
            out.extend_from_slice(&rtc);
        }
        out
    }

    /// Restore a battery-save blob produced by [`Cartridge::save_bytes`]. When
    /// this cartridge has an RTC, the trailing RTC bytes are split off and fed to
    /// the clock and the remaining prefix is loaded as RAM; otherwise the whole
    /// blob is RAM. A blob too short to carry the RTC trailer (e.g. an older
    /// RAM-only save) still loads its RAM and leaves the clock at its power-on
    /// value.
    pub fn load_bytes(&mut self, data: &[u8]) {
        // Presence of an RTC is a property of the mapper, so this split is
        // deterministic regardless of `data`'s length — the RAM prefix is always
        // exactly `self.ram.len()` bytes (which may be zero for a timer-only cart).
        let has_rtc = self.mbc.rtc_bytes().is_some();
        if has_rtc && data.len() >= self.ram.len() + mbc::RTC_SAVE_LEN {
            let split = self.ram.len();
            self.load_ram(&data[..split]);
            self.mbc.load_rtc(&data[split..]);
        } else {
            self.load_ram(data);
        }
    }

    /// Restore external RAM from a previously saved snapshot. A length mismatch
    /// is tolerated (copying the overlapping prefix) with a warning, so a save
    /// from a slightly different dump still loads.
    pub fn load_ram(&mut self, data: &[u8]) {
        if self.ram.is_empty() {
            if !data.is_empty() {
                tracing::warn!(
                    got = data.len(),
                    "ignoring save data: cartridge has no external ram"
                );
            }
            return;
        }
        if data.len() != self.ram.len() {
            tracing::warn!(
                expected = self.ram.len(),
                got = data.len(),
                "save data size mismatch; loading overlapping prefix"
            );
        }
        let n = data.len().min(self.ram.len());
        self.ram[..n].copy_from_slice(&data[..n]);
    }
}

/// How much external RAM to allocate. MBC2 carries a fixed built-in 512×4-bit
/// RAM and ignores the header's RAM-size byte; everyone else uses the declared
/// size.
fn ram_alloc_len(header: &CartridgeHeader) -> usize {
    match header.cartridge_type.mapper {
        Mapper::Mbc2 => 512,
        _ => header.ram_size_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use header::{MIN_ROM_LEN, ROM_BANK_SIZE};

    /// Build a ROM of `rom_size_byte`'s declared length with a valid header.
    fn build_rom(type_byte: u8, rom_size_byte: u8, ram_size_byte: u8) -> Vec<u8> {
        let (bytes, _) = decode_for_test(rom_size_byte);
        let mut rom = vec![0u8; bytes.max(MIN_ROM_LEN)];
        rom[0x0134..0x0134 + 4].copy_from_slice(b"ROMX");
        rom[0x0147] = type_byte;
        rom[0x0148] = rom_size_byte;
        rom[0x0149] = ram_size_byte;
        // header checksum over 0x0134..=0x014C
        let mut x = 0u8;
        for &b in &rom[0x0134..=0x014C] {
            x = x.wrapping_sub(b).wrapping_sub(1);
        }
        rom[0x014D] = x;
        rom
    }

    fn decode_for_test(rom_size_byte: u8) -> (usize, u16) {
        let banks = 2u16 << rom_size_byte;
        (banks as usize * ROM_BANK_SIZE, banks)
    }

    #[test]
    fn loads_rom_only() {
        let rom = build_rom(0x00, 0x00, 0x00);
        let cart = Cartridge::from_bytes(&rom).unwrap();
        assert_eq!(cart.header().cartridge_type.mapper, Mapper::RomOnly);
        // ROM straight-mapped: the type byte is readable at its address.
        assert_eq!(cart.read(0x0147), 0x00);
    }

    #[test]
    fn out_of_range_read_is_open_bus() {
        let rom = build_rom(0x00, 0x00, 0x00);
        let cart = Cartridge::from_bytes(&rom).unwrap();
        assert_eq!(cart.read(0xC000), 0xFF); // WRAM, not a cartridge address
        assert_eq!(cart.read(0xA000), 0xFF); // no external RAM present
    }

    #[test]
    fn battery_ram_round_trips() {
        // MBC1 + RAM + BATTERY, 8 KiB RAM.
        let rom = build_rom(0x03, 0x01, 0x02);
        let mut cart = Cartridge::from_bytes(&rom).unwrap();
        assert!(cart.has_battery());
        assert_eq!(cart.save_ram().len(), 8 * 1024);

        let mut snapshot = vec![0u8; 8 * 1024];
        snapshot[0] = 0xAB;
        snapshot[100] = 0xCD;
        cart.load_ram(&snapshot);
        assert_eq!(cart.save_ram()[0], 0xAB);
        assert_eq!(cart.save_ram()[100], 0xCD);
    }

    #[test]
    fn load_ram_tolerates_size_mismatch() {
        let rom = build_rom(0x03, 0x01, 0x02);
        let mut cart = Cartridge::from_bytes(&rom).unwrap();
        cart.load_ram(&[0x11, 0x22, 0x33]); // shorter than 8 KiB
        assert_eq!(cart.save_ram()[0], 0x11);
        assert_eq!(cart.save_ram()[2], 0x33);
    }

    #[test]
    fn mbc3_save_blob_carries_ram_and_rtc() {
        // MBC3 + TIMER + RAM + BATTERY, 8 KiB RAM.
        let rom = build_rom(0x10, 0x01, 0x02);
        let mut a = Cartridge::from_bytes(&rom).unwrap();
        a.write(0x0000, 0x0A); // enable RAM/RTC
        a.write(0x4000, 0x00); // select RAM bank 0
        a.write(0xA000, 0x42);
        a.write(0xA100, 0x99);

        let blob = a.save_bytes();
        assert_eq!(blob.len(), 8 * 1024 + mbc::RTC_SAVE_LEN);

        let mut b = Cartridge::from_bytes(&rom).unwrap();
        b.load_bytes(&blob);
        b.write(0x0000, 0x0A);
        b.write(0x4000, 0x00);
        assert_eq!(b.read(0xA000), 0x42);
        assert_eq!(b.read(0xA100), 0x99);
        // The RTC trailer round-trips verbatim (no time-based mutation between).
        assert_eq!(b.save_bytes(), blob);
    }

    #[test]
    fn non_rtc_save_blob_is_ram_only() {
        // MBC1 + RAM + BATTERY, 8 KiB: no RTC trailer.
        let rom = build_rom(0x03, 0x01, 0x02);
        let mut cart = Cartridge::from_bytes(&rom).unwrap();
        assert_eq!(cart.save_bytes().len(), 8 * 1024);

        let mut blob = vec![0u8; 8 * 1024];
        blob[5] = 0x7E;
        cart.load_bytes(&blob);
        assert_eq!(cart.save_ram()[5], 0x7E);
    }

    #[test]
    fn short_blob_loads_ram_and_skips_rtc() {
        // An RTC cart handed a RAM-only blob (no trailer) must load RAM and leave
        // the clock at its power-on value rather than panic.
        let rom = build_rom(0x10, 0x01, 0x02);
        let mut cart = Cartridge::from_bytes(&rom).unwrap();
        let blob = vec![0xAAu8; 8 * 1024];
        cart.load_bytes(&blob);
        assert_eq!(cart.save_ram()[0], 0xAA);
    }
}

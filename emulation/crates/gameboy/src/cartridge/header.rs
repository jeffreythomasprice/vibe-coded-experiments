//! Cartridge header parsing (`0x0100-0x014F`).
//!
//! Every Game Boy ROM begins with a fixed-layout header describing the title,
//! which Memory Bank Controller (MBC) the cartridge uses, and how much ROM and
//! RAM it carries. This module decodes that header into strongly-typed fields;
//! it does not itself perform any banking. The authoritative field layout and
//! the cartridge-type / size tables come from
//! `references/gameboy/Specifications.html` (the "Cartridge Header" section) and
//! are summarized in `GAMEBOY_IMPLEMENTATION_NOTES.md`.

use crate::cartridge::CartridgeError;

/// First byte of the header region.
pub const HEADER_START: usize = 0x0100;
/// Last byte of the header region (inclusive).
pub const HEADER_END: usize = 0x014F;
/// Smallest ROM we can meaningfully parse: everything through the header.
pub const MIN_ROM_LEN: usize = HEADER_END + 1;

const TITLE_START: usize = 0x0134;
const TITLE_END: usize = 0x0143; // inclusive; also the CGB flag on CGB carts
const CGB_FLAG: usize = 0x0143;
const SGB_FLAG: usize = 0x0146;
const CARTRIDGE_TYPE: usize = 0x0147;
const ROM_SIZE: usize = 0x0148;
const RAM_SIZE: usize = 0x0149;
const DESTINATION: usize = 0x014A;
const OLD_LICENSEE: usize = 0x014B;
const MASK_ROM_VERSION: usize = 0x014C;
const HEADER_CHECKSUM: usize = 0x014D;
const GLOBAL_CHECKSUM: usize = 0x014E; // 0x014E-0x014F, big-endian

/// One 16 KiB ROM bank.
pub const ROM_BANK_SIZE: usize = 0x4000;
/// One 8 KiB external-RAM bank.
pub const RAM_BANK_SIZE: usize = 0x2000;

/// The MBC chip family a cartridge uses, decoded from the `0x0147` type byte.
///
/// This names the *chip*; RAM/battery/timer/rumble/sensor presence is tracked
/// separately on [`CartridgeType`] because those cut across mapper families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mapper {
    RomOnly,
    Mbc1,
    Mbc2,
    Mbc3,
    Mbc5,
    /// Apocryphal: listed in old docs (`0x15-0x17`) but no known real cartridge.
    Mbc4,
    Mbc6,
    Mbc7,
    Mmm01,
    HuC1,
    HuC3,
    Tama5,
    PocketCamera,
    /// A `0x0147` value not present in any published table.
    Unknown(u8),
}

/// Fully decoded `0x0147` cartridge-type byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CartridgeType {
    pub mapper: Mapper,
    pub has_ram: bool,
    pub has_battery: bool,
    /// MBC3 real-time clock.
    pub has_timer: bool,
    /// MBC5 rumble motor.
    pub has_rumble: bool,
    /// MBC7 accelerometer / Pocket Camera image sensor.
    pub has_sensor: bool,
    /// The raw byte, preserved for logging and round-tripping.
    pub raw: u8,
}

impl CartridgeType {
    /// Decode the `0x0147` byte. Every value in the published table is mapped;
    /// anything else becomes [`Mapper::Unknown`] with all feature flags off.
    pub fn from_byte(byte: u8) -> CartridgeType {
        use Mapper::*;
        let (mapper, has_ram, has_battery, has_timer, has_rumble, has_sensor) = match byte {
            0x00 => (RomOnly, false, false, false, false, false),
            0x01 => (Mbc1, false, false, false, false, false),
            0x02 => (Mbc1, true, false, false, false, false),
            0x03 => (Mbc1, true, true, false, false, false),
            0x05 => (Mbc2, false, false, false, false, false),
            0x06 => (Mbc2, false, true, false, false, false),
            0x08 => (RomOnly, true, false, false, false, false),
            0x09 => (RomOnly, true, true, false, false, false),
            0x0B => (Mmm01, false, false, false, false, false),
            0x0C => (Mmm01, true, false, false, false, false),
            0x0D => (Mmm01, true, true, false, false, false),
            0x0F => (Mbc3, false, true, true, false, false),
            0x10 => (Mbc3, true, true, true, false, false),
            0x11 => (Mbc3, false, false, false, false, false),
            0x12 => (Mbc3, true, false, false, false, false),
            0x13 => (Mbc3, true, true, false, false, false),
            0x15 => (Mbc4, false, false, false, false, false),
            0x16 => (Mbc4, true, false, false, false, false),
            0x17 => (Mbc4, true, true, false, false, false),
            0x19 => (Mbc5, false, false, false, false, false),
            0x1A => (Mbc5, true, false, false, false, false),
            0x1B => (Mbc5, true, true, false, false, false),
            0x1C => (Mbc5, false, false, false, true, false),
            0x1D => (Mbc5, true, false, false, true, false),
            0x1E => (Mbc5, true, true, false, true, false),
            0x20 => (Mbc6, true, true, false, false, false),
            0x22 => (Mbc7, true, true, false, true, true),
            0xFC => (PocketCamera, true, true, false, false, true),
            0xFD => (Tama5, true, true, true, false, false),
            0xFE => (HuC3, true, true, true, false, false),
            0xFF => (HuC1, true, true, false, false, false),
            other => (Unknown(other), false, false, false, false, false),
        };
        CartridgeType {
            mapper,
            has_ram,
            has_battery,
            has_timer,
            has_rumble,
            has_sensor,
            raw: byte,
        }
    }
}

/// Game Boy Color compatibility, from the `0x0143` flag byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CgbFlag {
    /// Not a CGB-aware cartridge (`0x0143` is part of the title).
    None,
    /// CGB features supported, DMG-compatible (`0x80`).
    Enhanced,
    /// CGB required (`0xC0`).
    CgbOnly,
}

impl CgbFlag {
    fn from_byte(byte: u8) -> CgbFlag {
        match byte {
            0x80 => CgbFlag::Enhanced,
            0xC0 => CgbFlag::CgbOnly,
            _ => CgbFlag::None,
        }
    }

    /// Whether this cartridge treats `0x0143` as a CGB flag rather than a title
    /// character (true for both CGB-enhanced and CGB-only carts).
    fn occupies_title_byte(self) -> bool {
        !matches!(self, CgbFlag::None)
    }
}

/// Intended sales region, from the `0x014A` byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destination {
    Japanese,
    NonJapanese,
    Unknown(u8),
}

impl Destination {
    fn from_byte(byte: u8) -> Destination {
        match byte {
            0x00 => Destination::Japanese,
            0x01 => Destination::NonJapanese,
            other => Destination::Unknown(other),
        }
    }
}

/// Fully parsed cartridge header.
#[derive(Debug, Clone)]
pub struct CartridgeHeader {
    pub title: String,
    pub cartridge_type: CartridgeType,
    pub rom_size_bytes: usize,
    pub rom_banks: u16,
    pub ram_size_bytes: usize,
    pub ram_banks: u16,
    pub cgb_flag: CgbFlag,
    pub sgb: bool,
    pub destination: Destination,
    pub old_licensee: u8,
    pub mask_rom_version: u8,
    /// Header checksum as stored at `0x014D`.
    pub header_checksum: u8,
    /// Header checksum recomputed from the header bytes.
    pub computed_header_checksum: u8,
    /// 16-bit global checksum at `0x014E-0x014F` (the real GB never verifies it).
    pub global_checksum: u16,
}

impl CartridgeHeader {
    /// Parse the header out of a full ROM image.
    ///
    /// Fails only when the image is too short to contain a header. A mismatched
    /// header checksum is *not* an error — real hardware refuses to boot, but an
    /// emulator is more useful if it loads homebrew and hand-assembled ROMs
    /// anyway; callers can consult [`header_checksum_valid`](Self::header_checksum_valid).
    pub fn parse(rom: &[u8]) -> Result<CartridgeHeader, CartridgeError> {
        if rom.len() < MIN_ROM_LEN {
            return Err(CartridgeError::TooShort {
                needed: MIN_ROM_LEN,
                got: rom.len(),
            });
        }

        let cgb_flag = CgbFlag::from_byte(rom[CGB_FLAG]);
        let title = parse_title(rom, cgb_flag);
        let cartridge_type = CartridgeType::from_byte(rom[CARTRIDGE_TYPE]);
        let (rom_size_bytes, rom_banks) = decode_rom_size(rom[ROM_SIZE])?;
        let (ram_size_bytes, ram_banks) = decode_ram_size(rom[RAM_SIZE])?;
        let computed_header_checksum = compute_header_checksum(rom);
        let global_checksum =
            u16::from_be_bytes([rom[GLOBAL_CHECKSUM], rom[GLOBAL_CHECKSUM + 1]]);

        Ok(CartridgeHeader {
            title,
            cartridge_type,
            rom_size_bytes,
            rom_banks,
            ram_size_bytes,
            ram_banks,
            cgb_flag,
            sgb: rom[SGB_FLAG] == 0x03,
            destination: Destination::from_byte(rom[DESTINATION]),
            old_licensee: rom[OLD_LICENSEE],
            mask_rom_version: rom[MASK_ROM_VERSION],
            header_checksum: rom[HEADER_CHECKSUM],
            computed_header_checksum,
            global_checksum,
        })
    }

    /// Whether the stored header checksum matches the recomputed value.
    pub fn header_checksum_valid(&self) -> bool {
        self.header_checksum == self.computed_header_checksum
    }
}

fn parse_title(rom: &[u8], cgb_flag: CgbFlag) -> String {
    let end = if cgb_flag.occupies_title_byte() {
        TITLE_END // exclusive of 0x0143 (the CGB flag byte)
    } else {
        TITLE_END + 1
    };
    let bytes = &rom[TITLE_START..end];
    // Titles are 0x00-padded ASCII; stop at the first NUL and drop any
    // non-printable bytes rather than emit control characters.
    bytes
        .iter()
        .take_while(|&&b| b != 0x00)
        .filter(|&&b| (0x20..=0x7E).contains(&b))
        .map(|&b| b as char)
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Decode the `0x0148` ROM-size byte into (bytes, bank count).
fn decode_rom_size(byte: u8) -> Result<(usize, u16), CartridgeError> {
    let banks: u16 = match byte {
        0x00..=0x08 => 2u16 << byte,
        0x52 => 72,
        0x53 => 80,
        0x54 => 96,
        other => return Err(CartridgeError::UnknownRomSize(other)),
    };
    Ok((banks as usize * ROM_BANK_SIZE, banks))
}

/// Decode the `0x0149` RAM-size byte into (bytes, bank count).
///
/// Note the header's `02`/`05`/`04`/`03` ordering is *not* monotonic; the byte
/// values are a lookup table, not an exponent.
fn decode_ram_size(byte: u8) -> Result<(usize, u16), CartridgeError> {
    let bytes: usize = match byte {
        0x00 => 0,
        0x01 => 2 * 1024,
        0x02 => 8 * 1024,
        0x03 => 32 * 1024,
        0x04 => 128 * 1024,
        0x05 => 64 * 1024,
        other => return Err(CartridgeError::UnknownRamSize(other)),
    };
    // A 2 KiB cart still occupies one (partial) 8 KiB bank slot.
    let banks = if bytes == 0 {
        0
    } else {
        (bytes.div_ceil(RAM_BANK_SIZE)) as u16
    };
    Ok((bytes, banks))
}

/// The `0x014D` header checksum: `x = 0; for i in 0x0134..=0x014C { x = x - MEM[i] - 1 }`.
fn compute_header_checksum(rom: &[u8]) -> u8 {
    let mut x: u8 = 0;
    for &byte in &rom[TITLE_START..=MASK_ROM_VERSION] {
        x = x.wrapping_sub(byte).wrapping_sub(1);
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal ROM image with a valid header for the given type/size
    /// bytes and a correctly computed header checksum.
    fn synthetic_rom(type_byte: u8, rom_size_byte: u8, ram_size_byte: u8) -> Vec<u8> {
        let mut rom = vec![0u8; MIN_ROM_LEN];
        // Title "TEST" then NUL padding.
        rom[TITLE_START..TITLE_START + 4].copy_from_slice(b"TEST");
        rom[CARTRIDGE_TYPE] = type_byte;
        rom[ROM_SIZE] = rom_size_byte;
        rom[RAM_SIZE] = ram_size_byte;
        rom[HEADER_CHECKSUM] = compute_header_checksum(&rom);
        rom
    }

    #[test]
    fn rejects_short_rom() {
        let err = CartridgeHeader::parse(&[0u8; 0x100]).unwrap_err();
        assert!(matches!(err, CartridgeError::TooShort { .. }));
    }

    #[test]
    fn parses_type_and_sizes() {
        let rom = synthetic_rom(0x03, 0x01, 0x02);
        let h = CartridgeHeader::parse(&rom).unwrap();
        assert_eq!(h.title, "TEST");
        assert_eq!(h.cartridge_type.mapper, Mapper::Mbc1);
        assert!(h.cartridge_type.has_ram);
        assert!(h.cartridge_type.has_battery);
        assert_eq!(h.rom_banks, 4);
        assert_eq!(h.rom_size_bytes, 4 * ROM_BANK_SIZE);
        assert_eq!(h.ram_size_bytes, 8 * 1024);
        assert_eq!(h.ram_banks, 1);
        assert!(h.header_checksum_valid());
    }

    #[test]
    fn rom_size_special_values() {
        assert_eq!(decode_rom_size(0x00).unwrap(), (32 * 1024, 2));
        assert_eq!(decode_rom_size(0x08).unwrap(), (8 * 1024 * 1024, 512));
        assert_eq!(decode_rom_size(0x52).unwrap().1, 72);
        assert_eq!(decode_rom_size(0x53).unwrap().1, 80);
        assert_eq!(decode_rom_size(0x54).unwrap().1, 96);
        assert!(matches!(
            decode_rom_size(0x99),
            Err(CartridgeError::UnknownRomSize(0x99))
        ));
    }

    #[test]
    fn ram_size_table() {
        assert_eq!(decode_ram_size(0x00).unwrap(), (0, 0));
        assert_eq!(decode_ram_size(0x01).unwrap(), (2 * 1024, 1));
        assert_eq!(decode_ram_size(0x02).unwrap(), (8 * 1024, 1));
        assert_eq!(decode_ram_size(0x03).unwrap(), (32 * 1024, 4));
        assert_eq!(decode_ram_size(0x04).unwrap(), (128 * 1024, 16));
        assert_eq!(decode_ram_size(0x05).unwrap(), (64 * 1024, 8));
        assert!(matches!(
            decode_ram_size(0x42),
            Err(CartridgeError::UnknownRamSize(0x42))
        ));
    }

    #[test]
    fn tampered_checksum_still_parses_but_flags_invalid() {
        let mut rom = synthetic_rom(0x00, 0x00, 0x00);
        rom[TITLE_START] ^= 0xFF; // corrupt a header byte, leave stored checksum
        let h = CartridgeHeader::parse(&rom).unwrap();
        assert!(!h.header_checksum_valid());
    }

    #[test]
    fn cgb_flag_excluded_from_title() {
        let mut rom = synthetic_rom(0x00, 0x00, 0x00);
        // Fill the full 16-byte title region, then set the CGB flag byte.
        rom[TITLE_START..=TITLE_END].fill(b'A');
        rom[CGB_FLAG] = 0x80;
        rom[HEADER_CHECKSUM] = compute_header_checksum(&rom);
        let h = CartridgeHeader::parse(&rom).unwrap();
        assert_eq!(h.cgb_flag, CgbFlag::Enhanced);
        assert_eq!(h.title.len(), 15); // 0x134..=0x142, excluding the flag byte
    }

    #[test]
    fn full_type_table_maps_every_documented_byte() {
        let expected = [
            (0x00u8, Mapper::RomOnly),
            (0x01, Mapper::Mbc1),
            (0x02, Mapper::Mbc1),
            (0x03, Mapper::Mbc1),
            (0x05, Mapper::Mbc2),
            (0x06, Mapper::Mbc2),
            (0x08, Mapper::RomOnly),
            (0x09, Mapper::RomOnly),
            (0x0B, Mapper::Mmm01),
            (0x0C, Mapper::Mmm01),
            (0x0D, Mapper::Mmm01),
            (0x0F, Mapper::Mbc3),
            (0x10, Mapper::Mbc3),
            (0x11, Mapper::Mbc3),
            (0x12, Mapper::Mbc3),
            (0x13, Mapper::Mbc3),
            (0x15, Mapper::Mbc4),
            (0x16, Mapper::Mbc4),
            (0x17, Mapper::Mbc4),
            (0x19, Mapper::Mbc5),
            (0x1A, Mapper::Mbc5),
            (0x1B, Mapper::Mbc5),
            (0x1C, Mapper::Mbc5),
            (0x1D, Mapper::Mbc5),
            (0x1E, Mapper::Mbc5),
            (0x20, Mapper::Mbc6),
            (0x22, Mapper::Mbc7),
            (0xFC, Mapper::PocketCamera),
            (0xFD, Mapper::Tama5),
            (0xFE, Mapper::HuC3),
            (0xFF, Mapper::HuC1),
        ];
        for (byte, mapper) in expected {
            assert_eq!(
                CartridgeType::from_byte(byte).mapper,
                mapper,
                "type byte {byte:#04x}"
            );
        }
        assert_eq!(
            CartridgeType::from_byte(0x42).mapper,
            Mapper::Unknown(0x42)
        );
    }

    #[test]
    fn feature_flags_decode() {
        let rumble = CartridgeType::from_byte(0x1E);
        assert_eq!(rumble.mapper, Mapper::Mbc5);
        assert!(rumble.has_ram && rumble.has_battery && rumble.has_rumble);

        let timer = CartridgeType::from_byte(0x10);
        assert!(timer.has_timer && timer.has_ram && timer.has_battery);

        let sensor = CartridgeType::from_byte(0x22);
        assert!(sensor.has_sensor && sensor.has_rumble);
    }
}

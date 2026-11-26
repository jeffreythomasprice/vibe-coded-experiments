//! Save states: a snapshot of the whole machine as an opaque byte string.
//!
//! The layout is [`MAGIC`], a little-endian [`FORMAT_VERSION`], then two
//! `postcard` values: a [`StateHeader`] and the [`Emulator`] itself. The ROM is
//! not stored, so a state only loads into a machine already running the same
//! cartridge. Transient host-facing buffers (audio samples, serial capture, the
//! frame-ready latch) are not stored either.
//!
//! `postcard` is not self-describing: **any** change to a serialized struct
//! changes the format, and must bump [`FORMAT_VERSION`].

use serde::{Deserialize, Serialize};

use crate::Emulator;

const MAGIC: &[u8; 4] = b"GBST";
const FORMAT_VERSION: u16 = 1;

/// Why a save state could not be produced or loaded.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("not a save state")]
    BadMagic,
    #[error("unsupported save state version {found} (expected {FORMAT_VERSION})")]
    UnsupportedVersion { found: u16 },
    #[error("save state belongs to a different cartridge ({found:?})")]
    WrongCartridge { found: String },
    #[error("save state is corrupt")]
    Corrupt,
    #[error("could not encode save state: {0}")]
    Encode(#[source] postcard::Error),
    #[error("could not decode save state: {0}")]
    Decode(#[source] postcard::Error),
}

#[derive(Debug, Serialize, Deserialize)]
struct StateHeader {
    title: String,
    global_checksum: u16,
    /// Guards the body: a damaged one could otherwise decode into register
    /// values that index out of bounds.
    body_hash: u64,
}

impl Emulator {
    /// Snapshot the machine.
    pub fn save_state(&self) -> Result<Vec<u8>, StateError> {
        let body = postcard::to_stdvec(self).map_err(StateError::Encode)?;
        let cartridge = self.cartridge().header();
        let header = StateHeader {
            title: cartridge.title.clone(),
            global_checksum: cartridge.global_checksum,
            body_hash: fnv1a(&body),
        };
        let header = postcard::to_stdvec(&header).map_err(StateError::Encode)?;

        let mut out = Vec::with_capacity(MAGIC.len() + 2 + header.len() + body.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(&body);
        tracing::debug!(bytes = out.len(), "captured save state");
        Ok(out)
    }

    /// Replace the machine's state with a snapshot from [`Emulator::save_state`].
    /// On error the running machine is left untouched.
    pub fn load_state(&mut self, data: &[u8]) -> Result<(), StateError> {
        let rest = data.strip_prefix(MAGIC).ok_or(StateError::BadMagic)?;
        let (version, rest) = rest.split_first_chunk().ok_or(StateError::Corrupt)?;
        let version = u16::from_le_bytes(*version);
        if version != FORMAT_VERSION {
            return Err(StateError::UnsupportedVersion { found: version });
        }

        let (header, body): (StateHeader, _) =
            postcard::take_from_bytes(rest).map_err(StateError::Decode)?;
        let cartridge = self.cartridge().header();
        if header.title != cartridge.title || header.global_checksum != cartridge.global_checksum {
            return Err(StateError::WrongCartridge {
                found: header.title,
            });
        }
        if header.body_hash != fnv1a(body) {
            return Err(StateError::Corrupt);
        }

        let mut restored: Emulator = postcard::from_bytes(body).map_err(StateError::Decode)?;
        if !self.cartridge().accepts_state(restored.cartridge()) {
            return Err(StateError::Corrupt);
        }
        restored
            .bus
            .cartridge_mut()
            .adopt_rom(self.bus.cartridge_mut());
        *self = restored;
        tracing::debug!(bytes = data.len(), "restored save state");
        Ok(())
    }
}

/// 64-bit FNV-1a.
fn fnv1a(data: &[u8]) -> u64 {
    data.iter().fold(0xCBF2_9CE4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3)
    })
}

/// `serde(with)` support for boxed fixed-size byte arrays, which serde does not
/// cover past 32 elements.
pub(crate) mod boxed_bytes {
    use serde::de::{Deserialize, Deserializer, Error};
    use serde::ser::Serializer;

    pub fn serialize<S: Serializer, const N: usize>(
        bytes: &[u8; N],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(
        deserializer: D,
    ) -> Result<Box<[u8; N]>, D::Error> {
        // Borrows from the input, which `postcard` always supports.
        let bytes = <&[u8]>::deserialize(deserializer)?;
        let array: [u8; N] = bytes
            .try_into()
            .map_err(|_| D::Error::invalid_length(bytes.len(), &"a fixed-size byte array"))?;
        Ok(Box::new(array))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bus, Cartridge};

    /// Turns the LCD on, then forever increments a counter in WRAM and mirrors it
    /// into VRAM so successive frames differ.
    const PROGRAM: &[u8] = &[
        0x3E, 0x91, // LD A,0x91
        0xE0, 0x40, // LDH (LCDC),A
        0x21, 0x00, 0xC0, // LD HL,0xC000
        0x34, // INC (HL)
        0x7E, // LD A,(HL)
        0xEA, 0x00, 0x80, // LD (0x8000),A
        0x18, 0xF9, // JR to INC (HL)
    ];

    fn rom(title: &[u8], cartridge_type: u8, ram_code: u8) -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0134..0x0134 + title.len()].copy_from_slice(title);
        rom[0x0147] = cartridge_type;
        rom[0x0149] = ram_code;
        rom[0x014D] = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |x, &b| x.wrapping_sub(b).wrapping_sub(1));
        rom[0x0100..0x0100 + PROGRAM.len()].copy_from_slice(PROGRAM);
        rom
    }

    fn machine(title: &[u8], cartridge_type: u8, ram_code: u8) -> Emulator {
        let rom = rom(title, cartridge_type, ram_code);
        Emulator::new(Cartridge::from_bytes(&rom).unwrap())
    }

    fn run(emu: &mut Emulator, frames: usize) {
        for _ in 0..frames {
            assert!(emu.run_frame().fault.is_none());
        }
    }

    #[test]
    fn a_restored_machine_replays_identically() {
        let mut emu = machine(b"STATE", 0x00, 0);
        run(&mut emu, 3);
        let saved = emu.save_state().unwrap();
        run(&mut emu, 5);
        let later = emu.save_state().unwrap();
        let later_frame = emu.framebuffer().to_vec();
        assert_ne!(saved, later);

        emu.load_state(&saved).unwrap();
        assert_eq!(emu.save_state().unwrap(), saved);
        // The ROM came across, so the program keeps running the same way.
        run(&mut emu, 5);
        assert_eq!(emu.save_state().unwrap(), later);
        assert_eq!(emu.framebuffer(), later_frame);
    }

    #[test]
    fn a_state_loads_into_a_freshly_built_machine() {
        let mut emu = machine(b"STATE", 0x00, 0);
        run(&mut emu, 3);
        let saved = emu.save_state().unwrap();

        let mut fresh = machine(b"STATE", 0x00, 0);
        fresh.load_state(&saved).unwrap();
        assert_eq!(fresh.save_state().unwrap(), saved);
        assert_eq!(fresh.framebuffer(), emu.framebuffer());
    }

    #[test]
    fn banked_ram_and_clock_round_trip() {
        // MBC1+RAM+BATTERY and MBC3+TIMER+RAM+BATTERY.
        for cartridge_type in [0x03, 0x10] {
            let mut emu = machine(b"BANKED", cartridge_type, 0x03);
            let cart = emu.bus.cartridge_mut();
            cart.write(0x0000, 0x0A);
            cart.write(0xA000, 0x42);
            let saved = emu.save_state().unwrap();
            let battery = emu.cartridge().save_bytes();
            emu.bus.cartridge_mut().write(0xA000, 0x99);

            emu.load_state(&saved).unwrap();
            assert_eq!(emu.bus.read8(0xA000), 0x42);
            assert_eq!(emu.cartridge().save_bytes(), battery);
        }
    }

    #[test]
    fn rejected_states_leave_the_machine_untouched() {
        let mut emu = machine(b"STATE", 0x00, 0);
        run(&mut emu, 2);
        let good = emu.save_state().unwrap();
        run(&mut emu, 2);
        let before = emu.save_state().unwrap();

        let other = machine(b"OTHER", 0x00, 0).save_state().unwrap();
        assert!(matches!(
            emu.load_state(&other),
            Err(StateError::WrongCartridge { .. })
        ));
        assert!(matches!(
            emu.load_state(b"garbage"),
            Err(StateError::BadMagic)
        ));
        let mut future = good.clone();
        future[4] = 0xFF;
        assert!(matches!(
            emu.load_state(&future),
            Err(StateError::UnsupportedVersion { .. })
        ));
        for len in [0, 4, 5, 8, 20, good.len() - 1] {
            assert!(emu.load_state(&good[..len]).is_err(), "truncated to {len}");
        }
        let mut flipped = good.clone();
        *flipped.last_mut().unwrap() ^= 0xFF;
        assert!(matches!(
            emu.load_state(&flipped),
            Err(StateError::Corrupt)
        ));

        assert_eq!(emu.save_state().unwrap(), before);
        run(&mut emu, 1);
    }

    #[test]
    fn a_state_from_a_differently_shaped_cartridge_is_rejected() {
        let saved = machine(b"STATE", 0x00, 0).save_state().unwrap();
        let mut banked = machine(b"STATE", 0x00, 0);
        // Same identity, different mapper: build it, then force the header match.
        let mut rom = rom(b"STATE", 0x03, 0x03);
        rom[0x014E..0x0150].copy_from_slice(&[0, 0]);
        banked.bus = crate::SystemBus::new(Cartridge::from_bytes(&rom).unwrap());
        assert!(matches!(
            banked.load_state(&saved),
            Err(StateError::Corrupt)
        ));
    }
}

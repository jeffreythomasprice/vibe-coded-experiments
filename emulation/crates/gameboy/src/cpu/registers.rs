//! The SM83 register file.
//!
//! Eight 8-bit registers (`A F B C D E H L`) that also pair into four 16-bit
//! registers (`AF BC DE HL`), plus the 16-bit `SP` and `PC`. `F` is a typed
//! [`Flags`] rather than a raw byte so its always-zero low nibble is enforced
//! structurally (see [`crate::cpu::flags`]); `set_af` funnels through
//! `Flags::from_u8` so even `POP AF` cannot smuggle bits into it.

use super::flags::Flags;

/// The CPU register file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Registers {
    pub a: u8,
    pub f: Flags,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}

impl Registers {
    /// The register state left by the DMG boot ROM, used when we skip it and
    /// jump straight to the cartridge entry point at `0x0100`.
    pub fn post_boot_dmg() -> Registers {
        Registers {
            a: 0x01,
            f: Flags::from_u8(0xB0),
            b: 0x00,
            c: 0x13,
            d: 0x00,
            e: 0xD8,
            h: 0x01,
            l: 0x4D,
            sp: 0xFFFE,
            pc: 0x0100,
        }
    }

    /// The `AF` pair (`A` high, `F` low).
    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f.to_u8()])
    }

    /// The `BC` pair.
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }

    /// The `DE` pair.
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }

    /// The `HL` pair.
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }

    /// Set the `AF` pair. The low nibble of `F` is discarded (hardware invariant).
    pub fn set_af(&mut self, value: u16) {
        let [a, f] = value.to_be_bytes();
        self.a = a;
        self.f = Flags::from_u8(f);
    }

    /// Set the `BC` pair.
    pub fn set_bc(&mut self, value: u16) {
        let [b, c] = value.to_be_bytes();
        self.b = b;
        self.c = c;
    }

    /// Set the `DE` pair.
    pub fn set_de(&mut self, value: u16) {
        let [d, e] = value.to_be_bytes();
        self.d = d;
        self.e = e;
    }

    /// Set the `HL` pair.
    pub fn set_hl(&mut self, value: u16) {
        let [h, l] = value.to_be_bytes();
        self.h = h;
        self.l = l;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_compose_high_and_low_bytes() {
        let r = Registers {
            a: 0x12,
            f: Flags::from_u8(0x30),
            b: 0x34,
            c: 0x56,
            ..Default::default()
        };
        assert_eq!(r.af(), 0x1230);
        assert_eq!(r.bc(), 0x3456);
    }

    #[test]
    fn set_pair_splits_into_high_and_low_bytes() {
        let mut r = Registers::default();
        r.set_de(0x9ABC);
        assert_eq!(r.d, 0x9A);
        assert_eq!(r.e, 0xBC);
        r.set_hl(0xDEAD);
        assert_eq!(r.hl(), 0xDEAD);
    }

    #[test]
    fn setting_af_masks_flag_low_nibble() {
        let mut r = Registers::default();
        r.set_af(0x12FF);
        assert_eq!(r.a, 0x12);
        assert_eq!(r.f.to_u8(), 0xF0);
        assert_eq!(r.af(), 0x12F0);
    }

    #[test]
    fn post_boot_seed_matches_dmg() {
        let r = Registers::post_boot_dmg();
        assert_eq!(r.af(), 0x01B0);
        assert_eq!(r.bc(), 0x0013);
        assert_eq!(r.de(), 0x00D8);
        assert_eq!(r.hl(), 0x014D);
        assert_eq!(r.sp, 0xFFFE);
        assert_eq!(r.pc, 0x0100);
    }
}

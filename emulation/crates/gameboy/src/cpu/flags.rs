//! The `F` flag register.
//!
//! `F` is the low byte of `AF` and holds four condition flags in its upper
//! nibble: `Z` (zero), `N` (add/sub), `H` (half-carry), `C` (carry). The low
//! nibble is **always zero on real hardware** — it cannot be set even by
//! `POP AF`. Modeling `F` as a newtype that masks that nibble on every write
//! makes the invariant impossible to violate, instead of trusting every call
//! site to remember it.
//!
//! A hand-rolled newtype (rather than the `bitflags` crate) keeps the `gameboy`
//! crate dependency-free, matching the workspace's preference.

const Z: u8 = 0b1000_0000;
const N: u8 = 0b0100_0000;
const H: u8 = 0b0010_0000;
const C: u8 = 0b0001_0000;

/// The `F` register: the `Z`/`N`/`H`/`C` condition flags. Only bits 7-4 are
/// stored; bits 3-0 are always zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags(u8);

impl Flags {
    /// Flags with every condition clear.
    pub fn empty() -> Flags {
        Flags(0)
    }

    /// Build flags from explicit `Z`/`N`/`H`/`C` values.
    pub fn new(z: bool, n: bool, h: bool, c: bool) -> Flags {
        let mut f = Flags(0);
        f.set_z(z);
        f.set_n(n);
        f.set_h(h);
        f.set_c(c);
        f
    }

    /// Build flags from a raw byte, discarding the always-zero low nibble.
    pub fn from_u8(value: u8) -> Flags {
        Flags(value & 0xF0)
    }

    /// The raw byte value, with the low nibble guaranteed zero.
    pub fn to_u8(self) -> u8 {
        self.0
    }

    /// Zero flag — the result of the last operation was `0`.
    pub fn z(self) -> bool {
        self.0 & Z != 0
    }

    /// Subtract flag — the last ALU op was a subtraction (consumed by `DAA`).
    pub fn n(self) -> bool {
        self.0 & N != 0
    }

    /// Half-carry flag — carry out of bit 3 (consumed by `DAA`).
    pub fn h(self) -> bool {
        self.0 & H != 0
    }

    /// Carry flag — carry out of the high bit, or the shifted-out bit.
    pub fn c(self) -> bool {
        self.0 & C != 0
    }

    /// Set or clear the zero flag.
    pub fn set_z(&mut self, value: bool) {
        self.set(Z, value);
    }

    /// Set or clear the subtract flag.
    pub fn set_n(&mut self, value: bool) {
        self.set(N, value);
    }

    /// Set or clear the half-carry flag.
    pub fn set_h(&mut self, value: bool) {
        self.set(H, value);
    }

    /// Set or clear the carry flag.
    pub fn set_c(&mut self, value: bool) {
        self.set(C, value);
    }

    fn set(&mut self, mask: u8, value: bool) {
        if value {
            self.0 |= mask;
        } else {
            self.0 &= !mask;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_u8_masks_low_nibble_to_zero() {
        assert_eq!(Flags::from_u8(0xFF).to_u8(), 0xF0);
        assert_eq!(Flags::from_u8(0b1010_1111).to_u8(), 0b1010_0000);
    }

    #[test]
    fn each_flag_bit_roundtrips_through_u8() {
        for (mask, name) in [(Z, "z"), (N, "n"), (H, "h"), (C, "c")] {
            let f = Flags::from_u8(mask);
            assert_eq!(f.to_u8(), mask, "flag {name} did not roundtrip");
        }
    }

    #[test]
    fn setters_toggle_individual_flags_without_disturbing_others() {
        let mut f = Flags::empty();
        f.set_z(true);
        f.set_c(true);
        assert!(f.z() && f.c() && !f.n() && !f.h());
        f.set_z(false);
        assert!(!f.z() && f.c());
    }

    #[test]
    fn each_getter_reads_its_own_bit() {
        assert!(Flags::from_u8(Z).z());
        assert!(Flags::from_u8(N).n());
        assert!(Flags::from_u8(H).h());
        assert!(Flags::from_u8(C).c());
        assert!(!Flags::from_u8(Z).c());
    }
}

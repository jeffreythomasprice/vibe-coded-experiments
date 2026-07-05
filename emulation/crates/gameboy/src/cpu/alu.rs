//! Pure arithmetic/logic primitives, isolated from opcode dispatch.
//!
//! Every function here is a total, side-effect-free mapping from operands to
//! `(result, flags)`. This is where the SM83's fiddly flag rules live — half-
//! carry boundaries, `DAA`'s BCD adjustment, the low-byte carry of `ADD SP,r8` —
//! so they can be exhaustively unit-tested without constructing a CPU or a bus.
//! The opcode handlers in [`super::execute`] and [`super::cb`] stay thin: they
//! shuttle bytes between registers/memory and call into here.
//!
//! Two intentional conventions:
//! - The carry-in variants (`add8`/`sub8` with `carry_in`) serve both the plain
//!   and the `adc`/`sbc`/`cp` forms; pass `false` for the plain op.
//! - The rotate/shift helpers always compute `Z` from the result (the `$CB`
//!   semantics). The base-page `RLCA/RLA/RRCA/RRA` force `Z=0`; that override is
//!   applied at their opcode arms, not here.

use super::flags::Flags;

/// `ADD`/`ADC`: `a + b + carry_in`. `H` = carry out of bit 3, `C` = carry out
/// of bit 7, `N` = 0.
pub(super) fn add8(a: u8, b: u8, carry_in: bool) -> (u8, Flags) {
    let cin = carry_in as u16;
    let sum = a as u16 + b as u16 + cin;
    let result = sum as u8;
    let h = (a & 0x0F) + (b & 0x0F) + cin as u8 > 0x0F;
    (result, Flags::new(result == 0, false, h, sum > 0xFF))
}

/// `SUB`/`SBC`/`CP`: `a - b - carry_in`. `H` = borrow from bit 4, `C` = borrow
/// from bit 8, `N` = 1. (`CP` uses this result only for its flags.)
pub(super) fn sub8(a: u8, b: u8, carry_in: bool) -> (u8, Flags) {
    let cin = carry_in as u16;
    let result = a.wrapping_sub(b).wrapping_sub(cin as u8);
    let h = (a & 0x0F) < (b & 0x0F) + cin as u8;
    let c = (a as u16) < b as u16 + cin;
    (result, Flags::new(result == 0, true, h, c))
}

/// `AND`: `H` set, `C` clear.
pub(super) fn and8(a: u8, b: u8) -> (u8, Flags) {
    let result = a & b;
    (result, Flags::new(result == 0, false, true, false))
}

/// `OR`: `H` and `C` clear.
pub(super) fn or8(a: u8, b: u8) -> (u8, Flags) {
    let result = a | b;
    (result, Flags::new(result == 0, false, false, false))
}

/// `XOR`: `H` and `C` clear.
pub(super) fn xor8(a: u8, b: u8) -> (u8, Flags) {
    let result = a ^ b;
    (result, Flags::new(result == 0, false, false, false))
}

/// `INC r`: 8-bit increment. `C` is **preserved** (`INC` does not touch it),
/// `N` = 0, `H` set when the low nibble was `0xF`.
pub(super) fn inc8(v: u8, prev_c: bool) -> (u8, Flags) {
    let result = v.wrapping_add(1);
    (result, Flags::new(result == 0, false, v & 0x0F == 0x0F, prev_c))
}

/// `DEC r`: 8-bit decrement. `C` is **preserved**, `N` = 1, `H` set when the low
/// nibble was `0x0` (borrow).
pub(super) fn dec8(v: u8, prev_c: bool) -> (u8, Flags) {
    let result = v.wrapping_sub(1);
    (result, Flags::new(result == 0, true, v & 0x0F == 0x00, prev_c))
}

/// `DAA`: adjust `A` into packed BCD after an add or subtract, using the `N`,
/// `H`, and `C` flags left by that operation. `H` is cleared; `C` reflects the
/// decimal carry.
pub(super) fn daa(a: u8, f: Flags) -> (u8, Flags) {
    let mut result = a;
    let mut carry = false;
    if !f.n() {
        if f.c() || a > 0x99 {
            result = result.wrapping_add(0x60);
            carry = true;
        }
        if f.h() || a & 0x0F > 0x09 {
            result = result.wrapping_add(0x06);
        }
    } else {
        if f.c() {
            result = result.wrapping_sub(0x60);
            carry = true;
        }
        if f.h() {
            result = result.wrapping_sub(0x06);
        }
    }
    (result, Flags::new(result == 0, f.n(), false, carry))
}

/// `CPL`: complement `A`. Sets `N` and `H`; preserves `Z` and `C`.
pub(super) fn cpl(a: u8, f: Flags) -> (u8, Flags) {
    (!a, Flags::new(f.z(), true, true, f.c()))
}

/// `ADD HL,rr`: 16-bit add. `Z` is **preserved**, `N` = 0, `H` = carry out of
/// bit 11, `C` = carry out of bit 15.
pub(super) fn add16(hl: u16, rr: u16, f: Flags) -> (u16, Flags) {
    let result = hl.wrapping_add(rr);
    let h = (hl & 0x0FFF) + (rr & 0x0FFF) > 0x0FFF;
    let c = hl as u32 + rr as u32 > 0xFFFF;
    (result, Flags::new(f.z(), false, h, c))
}

/// `ADD SP,r8` and `LD HL,SP+r8`: add a signed 8-bit offset to `SP`. `Z` and `N`
/// are cleared; `H`/`C` are computed from the **low byte** as an unsigned 8-bit
/// add (a well-known SM83 quirk).
pub(super) fn add_sp_r8(sp: u16, r8: i8) -> (u16, Flags) {
    let offset = r8 as u8; // raw byte, for the low-byte carry computation
    let result = sp.wrapping_add(r8 as i16 as u16);
    let h = (sp & 0x0F) + (offset as u16 & 0x0F) > 0x0F;
    let c = (sp & 0xFF) + (offset as u16 & 0xFF) > 0xFF;
    (result, Flags::new(false, false, h, c))
}

/// `RLC`: rotate left, bit 7 into both carry and bit 0.
pub(super) fn rlc(v: u8) -> (u8, Flags) {
    let carry = v & 0x80 != 0;
    let result = v.rotate_left(1);
    (result, Flags::new(result == 0, false, false, carry))
}

/// `RRC`: rotate right, bit 0 into both carry and bit 7.
pub(super) fn rrc(v: u8) -> (u8, Flags) {
    let carry = v & 0x01 != 0;
    let result = v.rotate_right(1);
    (result, Flags::new(result == 0, false, false, carry))
}

/// `RL`: rotate left through carry (carry into bit 0, bit 7 into carry).
pub(super) fn rl(v: u8, carry_in: bool) -> (u8, Flags) {
    let carry = v & 0x80 != 0;
    let result = (v << 1) | carry_in as u8;
    (result, Flags::new(result == 0, false, false, carry))
}

/// `RR`: rotate right through carry (carry into bit 7, bit 0 into carry).
pub(super) fn rr(v: u8, carry_in: bool) -> (u8, Flags) {
    let carry = v & 0x01 != 0;
    let result = (v >> 1) | ((carry_in as u8) << 7);
    (result, Flags::new(result == 0, false, false, carry))
}

/// `SLA`: arithmetic shift left; bit 7 into carry, 0 into bit 0.
pub(super) fn sla(v: u8) -> (u8, Flags) {
    let carry = v & 0x80 != 0;
    let result = v << 1;
    (result, Flags::new(result == 0, false, false, carry))
}

/// `SRA`: arithmetic shift right; bit 0 into carry, bit 7 preserved (sign).
pub(super) fn sra(v: u8) -> (u8, Flags) {
    let carry = v & 0x01 != 0;
    let result = (v >> 1) | (v & 0x80);
    (result, Flags::new(result == 0, false, false, carry))
}

/// `SRL`: logical shift right; bit 0 into carry, 0 into bit 7.
pub(super) fn srl(v: u8) -> (u8, Flags) {
    let carry = v & 0x01 != 0;
    let result = v >> 1;
    (result, Flags::new(result == 0, false, false, carry))
}

/// `SWAP`: exchange the high and low nibbles. `C` clear.
pub(super) fn swap(v: u8) -> (u8, Flags) {
    let result = v.rotate_left(4);
    (result, Flags::new(result == 0, false, false, false))
}

/// `BIT n,v`: test bit `n`. `Z` = complement of the bit, `N` = 0, `H` = 1, `C`
/// preserved. Returns only flags (the operand is unchanged).
pub(super) fn bit(v: u8, n: u8, f: Flags) -> Flags {
    Flags::new(v & (1 << n) == 0, false, true, f.c())
}

/// `SET n,v`: set bit `n`.
pub(super) fn set(v: u8, n: u8) -> u8 {
    v | (1 << n)
}

/// `RES n,v`: clear bit `n`.
pub(super) fn res(v: u8, n: u8) -> u8 {
    v & !(1 << n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add8_sets_half_carry_at_nibble_boundary() {
        let (result, f) = add8(0x0F, 0x01, false);
        assert_eq!(result, 0x10);
        assert!(f.h() && !f.c() && !f.z() && !f.n());
    }

    #[test]
    fn add8_sets_carry_and_zero_on_wraparound() {
        let (result, f) = add8(0xFF, 0x01, false);
        assert_eq!(result, 0x00);
        assert!(f.c() && f.h() && f.z() && !f.n());
    }

    #[test]
    fn adc_propagates_carry_in_into_half_and_full() {
        let (result, f) = add8(0x0F, 0x00, true);
        assert_eq!(result, 0x10);
        assert!(f.h());
        let (result, f) = add8(0xFF, 0x00, true);
        assert_eq!(result, 0x00);
        assert!(f.c() && f.z());
    }

    #[test]
    fn sub8_sets_half_borrow() {
        let (result, f) = sub8(0x10, 0x01, false);
        assert_eq!(result, 0x0F);
        assert!(f.h() && f.n() && !f.c() && !f.z());
    }

    #[test]
    fn sub8_sets_carry_on_underflow() {
        let (result, f) = sub8(0x00, 0x01, false);
        assert_eq!(result, 0xFF);
        assert!(f.c() && f.h() && f.n());
    }

    #[test]
    fn sbc_borrows_carry_in() {
        let (result, f) = sub8(0x01, 0x00, true);
        assert_eq!(result, 0x00);
        assert!(f.z() && f.n() && !f.c());
        let (result, f) = sub8(0x00, 0x00, true);
        assert_eq!(result, 0xFF);
        assert!(f.c() && f.h());
    }

    #[test]
    fn cp_sets_zero_when_equal() {
        let (_, f) = sub8(0x42, 0x42, false);
        assert!(f.z() && f.n() && !f.c() && !f.h());
    }

    #[test]
    fn and_sets_half_but_or_and_xor_clear_it() {
        assert!(and8(0xF0, 0x0F).1.h());
        assert!(!or8(0xF0, 0x0F).1.h());
        assert!(!xor8(0xF0, 0x0F).1.h());
        assert!(and8(0x00, 0xFF).1.z());
        assert!(xor8(0xAA, 0xAA).1.z());
    }

    #[test]
    fn inc8_preserves_carry_and_sets_half_at_0f() {
        let (result, f) = inc8(0x0F, true);
        assert_eq!(result, 0x10);
        assert!(f.h() && f.c() && !f.n());
        let (result, f) = inc8(0xFF, false);
        assert_eq!(result, 0x00);
        assert!(f.z() && f.h() && !f.c());
    }

    #[test]
    fn dec8_preserves_carry_and_sets_n() {
        let (result, f) = dec8(0x10, true);
        assert_eq!(result, 0x0F);
        assert!(f.h() && f.n() && f.c());
        let (result, f) = dec8(0x01, false);
        assert_eq!(result, 0x00);
        assert!(f.z() && f.n() && !f.h());
    }

    #[test]
    fn daa_adjusts_after_addition() {
        // 0x09 + 0x08 = 0x11 (raw), H set -> DAA yields BCD 17.
        let (sum, f) = add8(0x09, 0x08, false);
        let (bcd, f) = daa(sum, f);
        assert_eq!(bcd, 0x17);
        assert!(!f.c() && !f.h());

        // 0x90 + 0x80 = 0x10 (raw) with carry -> BCD 70, carry set.
        let (sum, f) = add8(0x90, 0x80, false);
        let (bcd, f) = daa(sum, f);
        assert_eq!(bcd, 0x70);
        assert!(f.c());
    }

    #[test]
    fn daa_adjusts_after_subtraction() {
        // BCD 0x42 - 0x13: raw sub then DAA -> 0x29.
        let (diff, f) = sub8(0x42, 0x13, false);
        let (bcd, f) = daa(diff, f);
        assert_eq!(bcd, 0x29);
        assert!(f.n() && !f.c());
    }

    #[test]
    fn cpl_sets_n_and_h_preserving_z_and_c() {
        let (result, f) = cpl(0x35, Flags::new(true, false, false, true));
        assert_eq!(result, 0xCA);
        assert!(f.n() && f.h() && f.z() && f.c());
    }

    #[test]
    fn add16_half_from_bit11_carry_from_bit15_zero_preserved() {
        let (result, f) = add16(0x0FFF, 0x0001, Flags::new(true, true, true, true));
        assert_eq!(result, 0x1000);
        assert!(f.h() && !f.c() && f.z() && !f.n());

        let (result, f) = add16(0xFFFF, 0x0001, Flags::empty());
        assert_eq!(result, 0x0000);
        assert!(f.c() && f.h() && !f.z());
    }

    #[test]
    fn add_sp_r8_computes_half_and_carry_from_low_byte() {
        let (result, f) = add_sp_r8(0x000F, 0x01);
        assert_eq!(result, 0x0010);
        assert!(f.h() && !f.c() && !f.z() && !f.n());

        let (result, f) = add_sp_r8(0x00FF, 0x01);
        assert_eq!(result, 0x0100);
        assert!(f.h() && f.c());
    }

    #[test]
    fn add_sp_r8_handles_negative_offset() {
        // 0x0000 + (-1): low-byte add 0x00 + 0xFF = 0xFF — no bit-3 or bit-7 carry.
        let (result, f) = add_sp_r8(0x0000, -1);
        assert_eq!(result, 0xFFFF);
        assert!(!f.h() && !f.c());

        // 0x00FF + (-1): low nibble 0xF+0xF carries (H); full byte 0xFF+0xFF carries (C).
        let (result, f) = add_sp_r8(0x00FF, -1);
        assert_eq!(result, 0x00FE);
        assert!(f.h() && f.c());
    }

    #[test]
    fn rlc_and_rrc_wrap_through_carry() {
        let (result, f) = rlc(0x80);
        assert_eq!(result, 0x01);
        assert!(f.c() && !f.z());
        let (result, f) = rrc(0x01);
        assert_eq!(result, 0x80);
        assert!(f.c());
        assert!(rlc(0x00).1.z());
    }

    #[test]
    fn rl_and_rr_use_carry_in() {
        let (result, f) = rl(0x80, true);
        assert_eq!(result, 0x01);
        assert!(f.c());
        let (result, f) = rr(0x01, true);
        assert_eq!(result, 0x80);
        assert!(f.c());
    }

    #[test]
    fn sla_sra_srl_behave_distinctly() {
        let (result, f) = sla(0x80);
        assert_eq!(result, 0x00);
        assert!(f.c() && f.z());

        let (result, f) = sra(0x81);
        assert_eq!(result, 0xC0); // sign bit preserved
        assert!(f.c());

        let (result, f) = srl(0x81);
        assert_eq!(result, 0x40); // zero shifted into bit 7
        assert!(f.c());
    }

    #[test]
    fn swap_exchanges_nibbles_and_sets_zero() {
        let (result, f) = swap(0xAB);
        assert_eq!(result, 0xBA);
        assert!(!f.z() && !f.c());
        assert!(swap(0x00).1.z());
    }

    #[test]
    fn bit_sets_zero_from_complement_and_preserves_carry() {
        let f = bit(0b0000_1000, 3, Flags::new(false, false, false, true));
        assert!(!f.z() && !f.n() && f.h() && f.c());
        let f = bit(0b0000_1000, 2, Flags::empty());
        assert!(f.z() && f.h());
    }

    #[test]
    fn set_and_res_modify_the_targeted_bit() {
        assert_eq!(set(0x00, 7), 0x80);
        assert_eq!(res(0xFF, 0), 0xFE);
        assert_eq!(set(0x0F, 4), 0x1F);
        assert_eq!(res(0x10, 4), 0x00);
    }
}

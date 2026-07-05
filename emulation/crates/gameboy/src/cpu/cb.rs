//! `$CB`-prefixed opcode dispatch (rotates/shifts and single-bit ops).
//!
//! The whole `$CB` page is perfectly regular, so we decode it structurally
//! rather than listing 256 arms:
//! - bits 7-6 pick the group — `00` rotate/shift, `01` `BIT`, `10` `RES`,
//!   `11` `SET`;
//! - bits 5-3 pick the sub-operation (rotate/shift kind) or the bit index;
//! - bits 2-0 pick the operand register (`B C D E H L (HL) A`), shared with the
//!   base table via [`Cpu::read_r`]/[`Cpu::write_r`].
//!
//! Timing: register operands are 8 T-states. The `(HL)` operand is 16 (it is a
//! read-modify-write) — except `BIT n,(HL)`, which only reads and so costs 12.

use super::{alu, Bus, Cpu};

impl Cpu {
    /// Decode and execute one `$CB`-prefixed opcode (the byte *after* the `0xCB`
    /// prefix); returns its T-states.
    pub(super) fn execute_cb<B: Bus>(&mut self, bus: &mut B, opcode: u8) -> u32 {
        let code = opcode & 0x07;
        let is_hl = Cpu::is_hl_operand(code);
        let value = self.read_r(bus, code);

        match opcode >> 6 {
            0b00 => {
                let (result, flags) = match (opcode >> 3) & 0x07 {
                    0 => alu::rlc(value),
                    1 => alu::rrc(value),
                    2 => alu::rl(value, self.regs.f.c()),
                    3 => alu::rr(value, self.regs.f.c()),
                    4 => alu::sla(value),
                    5 => alu::sra(value),
                    6 => alu::swap(value),
                    7 => alu::srl(value),
                    _ => unreachable!(),
                };
                self.write_r(bus, code, result);
                self.regs.f = flags;
                if is_hl {
                    16
                } else {
                    8
                }
            }
            0b01 => {
                let n = (opcode >> 3) & 0x07;
                self.regs.f = alu::bit(value, n, self.regs.f);
                if is_hl {
                    12
                } else {
                    8
                }
            }
            0b10 => {
                let n = (opcode >> 3) & 0x07;
                self.write_r(bus, code, alu::res(value, n));
                if is_hl {
                    16
                } else {
                    8
                }
            }
            0b11 => {
                let n = (opcode >> 3) & 0x07;
                self.write_r(bus, code, alu::set(value, n));
                if is_hl {
                    16
                } else {
                    8
                }
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::FlatMemory;

    fn cpu_running(program: &[u8]) -> (Cpu, FlatMemory) {
        let mut cpu = Cpu::new();
        cpu.regs.pc = 0x0100;
        let mut mem = FlatMemory::new();
        mem.load(0x0100, program);
        (cpu, mem)
    }

    #[test]
    fn cb_bit_7_h_computes_zero_from_complement() {
        let (mut cpu, mut mem) = cpu_running(&[0xCB, 0x7C]); // BIT 7, H
        cpu.regs.h = 0x00;
        assert_eq!(cpu.step(&mut mem), 8);
        assert!(cpu.regs.f.z() && cpu.regs.f.h() && !cpu.regs.f.n());

        let (mut cpu, mut mem) = cpu_running(&[0xCB, 0x7C]);
        cpu.regs.h = 0x80;
        cpu.step(&mut mem);
        assert!(!cpu.regs.f.z());
    }

    #[test]
    fn cb_swap_a_exchanges_nibbles() {
        let (mut cpu, mut mem) = cpu_running(&[0xCB, 0x37]); // SWAP A
        cpu.regs.a = 0xAB;
        assert_eq!(cpu.step(&mut mem), 8);
        assert_eq!(cpu.regs.a, 0xBA);
    }

    #[test]
    fn cb_rl_c_rotates_through_carry() {
        let (mut cpu, mut mem) = cpu_running(&[0xCB, 0x11]); // RL C
        cpu.regs.c = 0x80;
        cpu.regs.f.set_c(false);
        cpu.step(&mut mem);
        assert_eq!(cpu.regs.c, 0x00);
        assert!(cpu.regs.f.c() && cpu.regs.f.z());
    }

    #[test]
    fn cb_res_and_set_modify_bits_in_place() {
        let (mut cpu, mut mem) = cpu_running(&[0xCB, 0x87, 0xCB, 0xC7]); // RES 0,A ; SET 0,A
        cpu.regs.a = 0xFF;
        cpu.step(&mut mem);
        assert_eq!(cpu.regs.a, 0xFE);
        cpu.step(&mut mem);
        assert_eq!(cpu.regs.a, 0xFF);
    }

    #[test]
    fn cb_ops_on_hl_cost_more_than_register_ops() {
        // RLC (HL) = 16, BIT 0,(HL) = 12, SET 7,(HL) = 16.
        let (mut cpu, mut mem) = cpu_running(&[0xCB, 0x06, 0xCB, 0x46, 0xCB, 0xFE]);
        cpu.regs.set_hl(0xC000);
        mem.write8(0xC000, 0x80);
        assert_eq!(cpu.step(&mut mem), 16); // RLC (HL)
        assert_eq!(mem.read8(0xC000), 0x01);
        assert_eq!(cpu.step(&mut mem), 12); // BIT 0,(HL)
        assert_eq!(cpu.step(&mut mem), 16); // SET 7,(HL)
        assert_eq!(mem.read8(0xC000), 0x81);
    }
}

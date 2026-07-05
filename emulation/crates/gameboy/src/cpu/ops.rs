//! Operand and control-flow helpers shared by the base and `$CB` dispatch.
//!
//! Both opcode tables address an 8-bit operand by the same 3-bit code
//! (`B C D E H L (HL) A`), and the base table's jumps/calls/returns share one
//! condition encoding. Factoring those here keeps [`super::execute`] and
//! [`super::cb`] as thin tables that read cleanly against the opcode matrix.

use super::{alu, Bus, Cpu};

impl Cpu {
    /// Read the 8-bit operand named by a 3-bit register code
    /// (`0=B 1=C 2=D 3=E 4=H 5=L 6=(HL) 7=A`). Code `6` reads memory at `HL`.
    pub(super) fn read_r<B: Bus>(&mut self, bus: &mut B, code: u8) -> u8 {
        match code {
            0 => self.regs.b,
            1 => self.regs.c,
            2 => self.regs.d,
            3 => self.regs.e,
            4 => self.regs.h,
            5 => self.regs.l,
            6 => bus.read8(self.regs.hl()),
            7 => self.regs.a,
            _ => unreachable!("register code out of range: {code}"),
        }
    }

    /// Write the 8-bit operand named by a 3-bit register code. Code `6` writes
    /// memory at `HL`.
    pub(super) fn write_r<B: Bus>(&mut self, bus: &mut B, code: u8, value: u8) {
        match code {
            0 => self.regs.b = value,
            1 => self.regs.c = value,
            2 => self.regs.d = value,
            3 => self.regs.e = value,
            4 => self.regs.h = value,
            5 => self.regs.l = value,
            6 => bus.write8(self.regs.hl(), value),
            7 => self.regs.a = value,
            _ => unreachable!("register code out of range: {code}"),
        }
    }

    /// Whether a register code names the `(HL)` memory operand (the one that adds
    /// a memory-access cycle).
    pub(super) fn is_hl_operand(code: u8) -> bool {
        code == 6
    }

    /// Apply an 8-bit ALU operation to `A`, selected by the 3-bit code
    /// (`0=ADD 1=ADC 2=SUB 3=SBC 4=AND 5=XOR 6=OR 7=CP`). `CP` updates flags only.
    pub(super) fn alu_a(&mut self, op: u8, operand: u8) {
        let a = self.regs.a;
        let carry = self.regs.f.c();
        let (result, flags) = match op {
            0 => alu::add8(a, operand, false),
            1 => alu::add8(a, operand, carry),
            2 => alu::sub8(a, operand, false),
            3 => alu::sub8(a, operand, carry),
            4 => alu::and8(a, operand),
            5 => alu::xor8(a, operand),
            6 => alu::or8(a, operand),
            7 => {
                let (_, flags) = alu::sub8(a, operand, false);
                self.regs.f = flags;
                return;
            }
            _ => unreachable!("alu op out of range: {op}"),
        };
        self.regs.a = result;
        self.regs.f = flags;
    }

    /// Evaluate a 2-bit branch condition (`0=NZ 1=Z 2=NC 3=C`).
    pub(super) fn condition(&self, code: u8) -> bool {
        match code {
            0 => !self.regs.f.z(),
            1 => self.regs.f.z(),
            2 => !self.regs.f.c(),
            3 => self.regs.f.c(),
            _ => unreachable!("condition code out of range: {code}"),
        }
    }

    /// `JR`: fetch a signed offset (always, to advance `PC`) and branch if
    /// `take`. 12 T-states taken, 8 not taken.
    pub(super) fn jr<B: Bus>(&mut self, bus: &mut B, take: bool) -> u32 {
        let offset = self.fetch_signed(bus);
        if take {
            self.regs.pc = self.regs.pc.wrapping_add(offset as u16);
            12
        } else {
            8
        }
    }

    /// `JP`: fetch a 16-bit target and branch if `take`. 16 taken, 12 not taken.
    pub(super) fn jp<B: Bus>(&mut self, bus: &mut B, take: bool) -> u32 {
        let addr = self.fetch16(bus);
        if take {
            self.regs.pc = addr;
            16
        } else {
            12
        }
    }

    /// `CALL`: fetch a target and, if `take`, push `PC` and jump. 24 taken, 12 not.
    pub(super) fn call<B: Bus>(&mut self, bus: &mut B, take: bool) -> u32 {
        let addr = self.fetch16(bus);
        if take {
            let ret = self.regs.pc;
            self.push16(bus, ret);
            self.regs.pc = addr;
            24
        } else {
            12
        }
    }

    /// Conditional `RET`: 20 T-states taken, 8 not taken. (Unconditional `RET`
    /// and `RETI` are handled at their opcode arms — they cost 16.)
    pub(super) fn ret_cc<B: Bus>(&mut self, bus: &mut B, take: bool) -> u32 {
        if take {
            self.regs.pc = self.pop16(bus);
            20
        } else {
            8
        }
    }

    /// `RST`: push `PC` and jump to a fixed low vector. 16 T-states.
    pub(super) fn rst<B: Bus>(&mut self, bus: &mut B, vector: u16) -> u32 {
        let ret = self.regs.pc;
        self.push16(bus, ret);
        self.regs.pc = vector;
        16
    }
}

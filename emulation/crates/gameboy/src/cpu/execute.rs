//! Base (unprefixed) opcode dispatch.
//!
//! [`Cpu::execute_base`] is the 256-entry decode table. The four large regular
//! blocks — `LD r,r'` (`0x40-0x7F`), the 8-bit ALU (`0x80-0xBF`), and (inside
//! `execute_misc`) the register `INC`/`DEC`/`LD r,d8` families — are decoded
//! *structurally* from the opcode bits so they are correct by construction and
//! don't need 100+ near-identical arms. The genuinely irregular opcodes
//! (`0x00-0x3F`, `0xC0-0xFF`) are spelled out to mirror the opcode matrix.
//!
//! Every arm returns the instruction's T-state count; the `(HL)` operand adds a
//! memory-access cycle to the register-register base cost. The eleven undefined
//! opcodes lock the CPU (see [`Cpu::lock_up`]).

use super::{alu, Bus, Cpu, Flags, RunState};

/// `KEY1` (`0xFF4D`), the `[CGB]` prepare-speed-switch register.
const KEY1_ADDR: u16 = 0xFF4D;
/// `KEY1` bit 0: set to request a speed switch on the next `STOP`.
const KEY1_PREPARE_SWITCH: u8 = 0x01;
/// `KEY1` bit 7: current speed (`1` = double). Toggled by a switch.
const KEY1_CURRENT_DOUBLE: u8 = 0x80;

impl Cpu {
    /// Decode and execute one unprefixed opcode; returns its T-states.
    pub(super) fn execute_base<B: Bus>(&mut self, bus: &mut B, opcode: u8) -> u32 {
        match opcode {
            // LD r, r' (0x40-0x7F), with 0x76 carved out as HALT.
            0x76 => self.halt(bus),
            0x40..=0x7F => {
                let dst = (opcode >> 3) & 0x07;
                let src = opcode & 0x07;
                let value = self.read_r(bus, src);
                self.write_r(bus, dst, value);
                if Cpu::is_hl_operand(dst) || Cpu::is_hl_operand(src) {
                    8
                } else {
                    4
                }
            }

            // ALU A, r (0x80-0xBF).
            0x80..=0xBF => {
                let op = (opcode >> 3) & 0x07;
                let src = opcode & 0x07;
                let operand = self.read_r(bus, src);
                self.alu_a(op, operand);
                if Cpu::is_hl_operand(src) {
                    8
                } else {
                    4
                }
            }

            _ => self.execute_misc(bus, opcode),
        }
    }

    /// The irregular opcodes outside the two big structural blocks.
    fn execute_misc<B: Bus>(&mut self, bus: &mut B, opcode: u8) -> u32 {
        match opcode {
            0x00 => 4,
            0x10 => self.stop(bus),

            // 16-bit immediate loads.
            0x01 => {
                let v = self.fetch16(bus);
                self.regs.set_bc(v);
                12
            }
            0x11 => {
                let v = self.fetch16(bus);
                self.regs.set_de(v);
                12
            }
            0x21 => {
                let v = self.fetch16(bus);
                self.regs.set_hl(v);
                12
            }
            0x31 => {
                self.regs.sp = self.fetch16(bus);
                12
            }

            // LD (a16), SP.
            0x08 => {
                let addr = self.fetch16(bus);
                bus.write16(addr, self.regs.sp);
                20
            }

            // Indirect stores/loads of A through register pairs.
            0x02 => {
                bus.write8(self.regs.bc(), self.regs.a);
                8
            }
            0x12 => {
                bus.write8(self.regs.de(), self.regs.a);
                8
            }
            0x22 => {
                let hl = self.regs.hl();
                bus.write8(hl, self.regs.a);
                self.regs.set_hl(hl.wrapping_add(1));
                8
            }
            0x32 => {
                let hl = self.regs.hl();
                bus.write8(hl, self.regs.a);
                self.regs.set_hl(hl.wrapping_sub(1));
                8
            }
            0x0A => {
                self.regs.a = bus.read8(self.regs.bc());
                8
            }
            0x1A => {
                self.regs.a = bus.read8(self.regs.de());
                8
            }
            0x2A => {
                let hl = self.regs.hl();
                self.regs.a = bus.read8(hl);
                self.regs.set_hl(hl.wrapping_add(1));
                8
            }
            0x3A => {
                let hl = self.regs.hl();
                self.regs.a = bus.read8(hl);
                self.regs.set_hl(hl.wrapping_sub(1));
                8
            }

            // 16-bit INC/DEC (no flags).
            0x03 => {
                self.regs.set_bc(self.regs.bc().wrapping_add(1));
                8
            }
            0x13 => {
                self.regs.set_de(self.regs.de().wrapping_add(1));
                8
            }
            0x23 => {
                self.regs.set_hl(self.regs.hl().wrapping_add(1));
                8
            }
            0x33 => {
                self.regs.sp = self.regs.sp.wrapping_add(1);
                8
            }
            0x0B => {
                self.regs.set_bc(self.regs.bc().wrapping_sub(1));
                8
            }
            0x1B => {
                self.regs.set_de(self.regs.de().wrapping_sub(1));
                8
            }
            0x2B => {
                self.regs.set_hl(self.regs.hl().wrapping_sub(1));
                8
            }
            0x3B => {
                self.regs.sp = self.regs.sp.wrapping_sub(1);
                8
            }

            // ADD HL, rr.
            0x09 => self.add_hl(self.regs.bc()),
            0x19 => self.add_hl(self.regs.de()),
            0x29 => self.add_hl(self.regs.hl()),
            0x39 => self.add_hl(self.regs.sp),

            // 8-bit INC r (0x04..0x3C, step 8): preserves carry.
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => {
                let code = (opcode >> 3) & 0x07;
                let (result, flags) = alu::inc8(self.read_r(bus, code), self.regs.f.c());
                self.write_r(bus, code, result);
                self.regs.f = flags;
                if Cpu::is_hl_operand(code) {
                    12
                } else {
                    4
                }
            }
            // 8-bit DEC r (0x05..0x3D, step 8): preserves carry.
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => {
                let code = (opcode >> 3) & 0x07;
                let (result, flags) = alu::dec8(self.read_r(bus, code), self.regs.f.c());
                self.write_r(bus, code, result);
                self.regs.f = flags;
                if Cpu::is_hl_operand(code) {
                    12
                } else {
                    4
                }
            }
            // 8-bit immediate load LD r, d8 (0x06..0x3E, step 8).
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => {
                let code = (opcode >> 3) & 0x07;
                let value = self.fetch8(bus);
                self.write_r(bus, code, value);
                if Cpu::is_hl_operand(code) {
                    12
                } else {
                    8
                }
            }

            // Accumulator rotates (Z always cleared, unlike their $CB forms).
            0x07 => {
                let (r, mut f) = alu::rlc(self.regs.a);
                f.set_z(false);
                self.regs.a = r;
                self.regs.f = f;
                4
            }
            0x0F => {
                let (r, mut f) = alu::rrc(self.regs.a);
                f.set_z(false);
                self.regs.a = r;
                self.regs.f = f;
                4
            }
            0x17 => {
                let (r, mut f) = alu::rl(self.regs.a, self.regs.f.c());
                f.set_z(false);
                self.regs.a = r;
                self.regs.f = f;
                4
            }
            0x1F => {
                let (r, mut f) = alu::rr(self.regs.a, self.regs.f.c());
                f.set_z(false);
                self.regs.a = r;
                self.regs.f = f;
                4
            }

            // Accumulator/flag misc.
            0x27 => {
                let (r, f) = alu::daa(self.regs.a, self.regs.f);
                self.regs.a = r;
                self.regs.f = f;
                4
            }
            0x2F => {
                let (r, f) = alu::cpl(self.regs.a, self.regs.f);
                self.regs.a = r;
                self.regs.f = f;
                4
            }
            0x37 => {
                self.regs.f = Flags::new(self.regs.f.z(), false, false, true);
                4
            }
            0x3F => {
                let c = self.regs.f.c();
                self.regs.f = Flags::new(self.regs.f.z(), false, false, !c);
                4
            }

            // Relative jumps.
            0x18 => self.jr(bus, true),
            0x20 | 0x28 | 0x30 | 0x38 => {
                let cc = (opcode >> 3) & 0x03;
                let take = self.condition(cc);
                self.jr(bus, take)
            }

            // Absolute jumps.
            0xC3 => self.jp(bus, true),
            0xC2 | 0xCA | 0xD2 | 0xDA => {
                let cc = (opcode >> 3) & 0x03;
                let take = self.condition(cc);
                self.jp(bus, take)
            }
            0xE9 => {
                self.regs.pc = self.regs.hl();
                4
            }

            // Calls.
            0xCD => self.call(bus, true),
            0xC4 | 0xCC | 0xD4 | 0xDC => {
                let cc = (opcode >> 3) & 0x03;
                let take = self.condition(cc);
                self.call(bus, take)
            }

            // Returns.
            0xC9 => {
                self.regs.pc = self.pop16(bus);
                16
            }
            0xD9 => {
                self.regs.pc = self.pop16(bus);
                self.ime = true;
                self.ei_delay = 0;
                16
            }
            0xC0 | 0xC8 | 0xD0 | 0xD8 => {
                let cc = (opcode >> 3) & 0x03;
                let take = self.condition(cc);
                self.ret_cc(bus, take)
            }

            // Restarts.
            0xC7 => self.rst(bus, 0x00),
            0xCF => self.rst(bus, 0x08),
            0xD7 => self.rst(bus, 0x10),
            0xDF => self.rst(bus, 0x18),
            0xE7 => self.rst(bus, 0x20),
            0xEF => self.rst(bus, 0x28),
            0xF7 => self.rst(bus, 0x30),
            0xFF => self.rst(bus, 0x38),

            // Stack pushes/pops.
            0xC1 => {
                let v = self.pop16(bus);
                self.regs.set_bc(v);
                12
            }
            0xD1 => {
                let v = self.pop16(bus);
                self.regs.set_de(v);
                12
            }
            0xE1 => {
                let v = self.pop16(bus);
                self.regs.set_hl(v);
                12
            }
            0xF1 => {
                let v = self.pop16(bus);
                self.regs.set_af(v);
                12
            }
            0xC5 => {
                self.push16(bus, self.regs.bc());
                16
            }
            0xD5 => {
                self.push16(bus, self.regs.de());
                16
            }
            0xE5 => {
                self.push16(bus, self.regs.hl());
                16
            }
            0xF5 => {
                self.push16(bus, self.regs.af());
                16
            }

            // ALU A, d8.
            0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                let op = (opcode >> 3) & 0x07;
                let operand = self.fetch8(bus);
                self.alu_a(op, operand);
                8
            }

            // High-page ($FF00+) I/O.
            0xE0 => {
                let addr = 0xFF00 | self.fetch8(bus) as u16;
                bus.write8(addr, self.regs.a);
                12
            }
            0xF0 => {
                let addr = 0xFF00 | self.fetch8(bus) as u16;
                self.regs.a = bus.read8(addr);
                12
            }
            0xE2 => {
                let addr = 0xFF00 | self.regs.c as u16;
                bus.write8(addr, self.regs.a);
                8
            }
            0xF2 => {
                let addr = 0xFF00 | self.regs.c as u16;
                self.regs.a = bus.read8(addr);
                8
            }

            // Absolute A loads/stores.
            0xEA => {
                let addr = self.fetch16(bus);
                bus.write8(addr, self.regs.a);
                16
            }
            0xFA => {
                let addr = self.fetch16(bus);
                self.regs.a = bus.read8(addr);
                16
            }

            // 16-bit stack pointer arithmetic.
            0xE8 => {
                let offset = self.fetch_signed(bus);
                let (sp, flags) = alu::add_sp_r8(self.regs.sp, offset);
                self.regs.sp = sp;
                self.regs.f = flags;
                16
            }
            0xF8 => {
                let offset = self.fetch_signed(bus);
                let (hl, flags) = alu::add_sp_r8(self.regs.sp, offset);
                self.regs.set_hl(hl);
                self.regs.f = flags;
                12
            }
            0xF9 => {
                self.regs.sp = self.regs.hl();
                8
            }

            // Interrupt master enable/disable.
            0xF3 => {
                self.ime = false;
                self.ei_delay = 0;
                4
            }
            0xFB => {
                self.ei_delay = 2;
                4
            }

            // Undefined opcodes lock up the real CPU.
            0xD3 | 0xDB | 0xDD | 0xE3 | 0xE4 | 0xEB | 0xEC | 0xED | 0xF4 | 0xFC | 0xFD => {
                self.lock_up(opcode)
            }

            // Everything in 0x00-0x3F / 0xC0-0xFF is enumerated above; the two
            // structural ranges are handled in execute_base and never arrive here.
            _ => unreachable!("opcode {opcode:#04x} not handled by execute_misc"),
        }
    }

    /// `ADD HL, rr`: 8 T-states, flags per [`alu::add16`].
    fn add_hl(&mut self, rr: u16) -> u32 {
        let (result, flags) = alu::add16(self.regs.hl(), rr, self.regs.f);
        self.regs.set_hl(result);
        self.regs.f = flags;
        8
    }

    /// `HALT`: enter low-power sleep until an interrupt is pending. Fires the
    /// halt bug (re-reading the following byte) when `IME=0` and an interrupt is
    /// already pending.
    fn halt<B: Bus>(&mut self, bus: &mut B) -> u32 {
        let pending = self.pending_interrupts(bus);
        if !self.ime && pending != 0 {
            self.halt_bug = true;
        } else {
            self.state = RunState::Halted;
        }
        4
    }

    /// `STOP`: consume the mandatory second byte, then either perform a `[CGB]`
    /// speed switch or enter standby.
    ///
    /// When `KEY1` (`0xFF4D`) bit 0 (prepare-speed-switch) is set, `STOP` performs
    /// the switch — toggling the current-speed bit and clearing the prepare bit —
    /// and **execution continues** with the next instruction. CGB-aware ROMs
    /// (including Blargg's `cpu_instrs`, which runs its tests in double speed) use
    /// exactly this `JOYP=0x30` / `KEY1=0x01` / `STOP` idiom, and hang forever if
    /// `STOP` is modeled as an unconditional standby. We don't yet model the
    /// doubled clock (a `[CGB — future]` timing item), only the control flow, so
    /// the tests run at single speed but no longer stall.
    ///
    /// Otherwise (no switch armed) `STOP` enters standby; real DMG wake is via the
    /// joypad, which isn't wired to standby exit yet, so the CPU idles until reset.
    fn stop<B: Bus>(&mut self, bus: &mut B) -> u32 {
        let _ = self.fetch8(bus);
        let key1 = bus.read8(KEY1_ADDR);
        if key1 & KEY1_PREPARE_SWITCH != 0 {
            let switched = (key1 ^ KEY1_CURRENT_DOUBLE) & !KEY1_PREPARE_SWITCH;
            bus.write8(KEY1_ADDR, switched);
        } else {
            self.state = RunState::Stopped;
        }
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::FlatMemory;

    /// Build a CPU whose PC points at a program loaded at 0x0100.
    fn cpu_running(program: &[u8]) -> (Cpu, FlatMemory) {
        let mut cpu = Cpu::new();
        cpu.regs.pc = 0x0100;
        cpu.regs.sp = 0xFFFE;
        let mut mem = FlatMemory::new();
        mem.load(0x0100, program);
        (cpu, mem)
    }

    #[test]
    fn nop_consumes_four_cycles_and_advances_pc() {
        let (mut cpu, mut mem) = cpu_running(&[0x00]);
        let cycles = cpu.step(&mut mem);
        assert_eq!(cycles, 4);
        assert_eq!(cpu.regs.pc, 0x0101);
    }

    #[test]
    fn ld_between_registers_and_hl_indirect_roundtrip() {
        // LD B, A ; LD (HL), B ; LD C, (HL)
        let (mut cpu, mut mem) = cpu_running(&[0x47, 0x70, 0x4E]);
        cpu.regs.a = 0x5A;
        cpu.regs.set_hl(0xC000);
        assert_eq!(cpu.step(&mut mem), 4); // LD B, A
        assert_eq!(cpu.regs.b, 0x5A);
        assert_eq!(cpu.step(&mut mem), 8); // LD (HL), B — memory operand
        assert_eq!(mem.read8(0xC000), 0x5A);
        assert_eq!(cpu.step(&mut mem), 8); // LD C, (HL)
        assert_eq!(cpu.regs.c, 0x5A);
    }

    #[test]
    fn ld_d16_decodes_little_endian_and_advances_pc() {
        let (mut cpu, mut mem) = cpu_running(&[0x21, 0xCD, 0xAB]); // LD HL, 0xABCD
        assert_eq!(cpu.step(&mut mem), 12);
        assert_eq!(cpu.regs.hl(), 0xABCD);
        assert_eq!(cpu.regs.pc, 0x0103);
    }

    #[test]
    fn ld_a16_sp_stores_stack_pointer_little_endian() {
        let (mut cpu, mut mem) = cpu_running(&[0x08, 0x00, 0xC0]); // LD (0xC000), SP
        cpu.regs.sp = 0xBEEF;
        assert_eq!(cpu.step(&mut mem), 20);
        assert_eq!(mem.read8(0xC000), 0xEF);
        assert_eq!(mem.read8(0xC001), 0xBE);
    }

    #[test]
    fn ldi_and_ldd_move_hl() {
        let (mut cpu, mut mem) = cpu_running(&[0x22, 0x32]); // LD (HL+),A ; LD (HL-),A
        cpu.regs.a = 0x11;
        cpu.regs.set_hl(0xC000);
        cpu.step(&mut mem);
        assert_eq!(cpu.regs.hl(), 0xC001);
        cpu.step(&mut mem);
        assert_eq!(cpu.regs.hl(), 0xC000);
        assert_eq!(mem.read8(0xC000), 0x11);
        assert_eq!(mem.read8(0xC001), 0x11);
    }

    #[test]
    fn ldh_and_c_indirect_hit_the_high_io_page() {
        // LDH (0x80), A ; LD (C), A ; LDH A, (0x80)
        let (mut cpu, mut mem) = cpu_running(&[0xE0, 0x80, 0xE2, 0xF0, 0x80]);
        cpu.regs.a = 0x99;
        cpu.regs.c = 0x81;
        assert_eq!(cpu.step(&mut mem), 12); // LDH (0x80), A
        assert_eq!(mem.read8(0xFF80), 0x99);
        assert_eq!(cpu.step(&mut mem), 8); // LD (C), A -> 0xFF81
        assert_eq!(mem.read8(0xFF81), 0x99);
        cpu.regs.a = 0x00;
        assert_eq!(cpu.step(&mut mem), 12); // LDH A, (0x80)
        assert_eq!(cpu.regs.a, 0x99);
    }

    #[test]
    fn stop_with_speed_switch_armed_continues() {
        // KEY1 armed (bit 0), STOP (0x10 0x00), then INC A.
        let (mut cpu, mut mem) = cpu_running(&[0x10, 0x00, 0x3C]);
        cpu.regs.a = 0x00;
        mem.write8(0xFF4D, 0x01);
        cpu.step(&mut mem); // STOP: performs the switch and continues
        assert_eq!(mem.read8(0xFF4D), 0x80, "switched to double, prepare bit cleared");
        cpu.step(&mut mem); // INC A runs (not stopped)
        assert_eq!(cpu.regs.a, 0x01);
    }

    #[test]
    fn stop_without_speed_switch_enters_standby() {
        let (mut cpu, mut mem) = cpu_running(&[0x10, 0x00, 0x3C]); // STOP ; INC A
        cpu.regs.a = 0x00;
        cpu.step(&mut mem); // STOP -> standby
        let pc = cpu.regs.pc;
        cpu.step(&mut mem);
        cpu.step(&mut mem);
        assert_eq!(cpu.regs.pc, pc, "PC frozen while stopped");
        assert_eq!(cpu.regs.a, 0x00, "INC A never ran");
    }

    #[test]
    fn inc_dec_hl_indirect_read_modify_write() {
        let (mut cpu, mut mem) = cpu_running(&[0x34, 0x35]); // INC (HL) ; DEC (HL)
        cpu.regs.set_hl(0xC000);
        mem.write8(0xC000, 0x0F);
        assert_eq!(cpu.step(&mut mem), 12);
        assert_eq!(mem.read8(0xC000), 0x10);
        assert!(cpu.regs.f.h());
        assert_eq!(cpu.step(&mut mem), 12);
        assert_eq!(mem.read8(0xC000), 0x0F);
        assert!(cpu.regs.f.n());
    }

    #[test]
    fn push_then_pop_roundtrips() {
        let (mut cpu, mut mem) = cpu_running(&[0xC5, 0xD1]); // PUSH BC ; POP DE
        cpu.regs.set_bc(0x1234);
        assert_eq!(cpu.step(&mut mem), 16);
        assert_eq!(cpu.regs.sp, 0xFFFC);
        assert_eq!(cpu.step(&mut mem), 12);
        assert_eq!(cpu.regs.de(), 0x1234);
        assert_eq!(cpu.regs.sp, 0xFFFE);
    }

    #[test]
    fn pop_af_masks_the_flag_low_nibble() {
        let (mut cpu, mut mem) = cpu_running(&[0xF1]); // POP AF
        cpu.regs.sp = 0xC000;
        mem.write16(0xC000, 0x12FF);
        cpu.step(&mut mem);
        assert_eq!(cpu.regs.a, 0x12);
        assert_eq!(cpu.regs.f.to_u8(), 0xF0);
    }

    #[test]
    fn jr_nz_costs_twelve_taken_and_eight_not_taken() {
        // Taken: Z clear.
        let (mut cpu, mut mem) = cpu_running(&[0x20, 0x05]); // JR NZ, +5
        cpu.regs.f.set_z(false);
        assert_eq!(cpu.step(&mut mem), 12);
        assert_eq!(cpu.regs.pc, 0x0102 + 5);

        // Not taken: Z set.
        let (mut cpu, mut mem) = cpu_running(&[0x20, 0x05]);
        cpu.regs.f.set_z(true);
        assert_eq!(cpu.step(&mut mem), 8);
        assert_eq!(cpu.regs.pc, 0x0102);
    }

    #[test]
    fn jr_handles_negative_offset() {
        let (mut cpu, mut mem) = cpu_running(&[0x18, 0xFE]); // JR -2 (infinite loop)
        cpu.step(&mut mem);
        assert_eq!(cpu.regs.pc, 0x0100);
    }

    #[test]
    fn call_cc_costs_twenty_four_taken_and_twelve_not_taken() {
        // Taken.
        let (mut cpu, mut mem) = cpu_running(&[0xC4, 0x00, 0x20]); // CALL NZ, 0x2000
        cpu.regs.f.set_z(false);
        assert_eq!(cpu.step(&mut mem), 24);
        assert_eq!(cpu.regs.pc, 0x2000);
        assert_eq!(cpu.regs.sp, 0xFFFC);
        assert_eq!(mem.read16(0xFFFC), 0x0103); // return address pushed

        // Not taken.
        let (mut cpu, mut mem) = cpu_running(&[0xC4, 0x00, 0x20]);
        cpu.regs.f.set_z(true);
        assert_eq!(cpu.step(&mut mem), 12);
        assert_eq!(cpu.regs.pc, 0x0103);
    }

    #[test]
    fn ret_cc_costs_twenty_taken_and_eight_not_taken() {
        let (mut cpu, mut mem) = cpu_running(&[0xC0]); // RET NZ
        cpu.regs.sp = 0xC000;
        mem.write16(0xC000, 0x4000);
        cpu.regs.f.set_z(false);
        assert_eq!(cpu.step(&mut mem), 20);
        assert_eq!(cpu.regs.pc, 0x4000);

        let (mut cpu, mut mem) = cpu_running(&[0xC0]);
        cpu.regs.f.set_z(true);
        assert_eq!(cpu.step(&mut mem), 8);
        assert_eq!(cpu.regs.pc, 0x0101);
    }

    #[test]
    fn rst_pushes_pc_and_jumps_to_vector() {
        let (mut cpu, mut mem) = cpu_running(&[0xDF]); // RST 18H
        assert_eq!(cpu.step(&mut mem), 16);
        assert_eq!(cpu.regs.pc, 0x0018);
        assert_eq!(mem.read16(0xFFFC), 0x0101);
    }

    #[test]
    fn add_a_d8_sets_flags() {
        let (mut cpu, mut mem) = cpu_running(&[0xC6, 0x01]); // ADD A, 1
        cpu.regs.a = 0xFF;
        assert_eq!(cpu.step(&mut mem), 8);
        assert_eq!(cpu.regs.a, 0x00);
        assert!(cpu.regs.f.z() && cpu.regs.f.h() && cpu.regs.f.c());
    }

    #[test]
    fn add_then_daa_produces_bcd_result() {
        // A=0x45 ; ADD A, 0x38 ; DAA -> 0x83.
        let (mut cpu, mut mem) = cpu_running(&[0xC6, 0x38, 0x27]);
        cpu.regs.a = 0x45;
        cpu.step(&mut mem); // ADD
        cpu.step(&mut mem); // DAA
        assert_eq!(cpu.regs.a, 0x83);
    }

    #[test]
    fn add_sp_r8_and_ld_hl_sp_r8_share_flag_semantics() {
        let (mut cpu, mut mem) = cpu_running(&[0xE8, 0x01]); // ADD SP, 1
        cpu.regs.sp = 0x000F;
        assert_eq!(cpu.step(&mut mem), 16);
        assert_eq!(cpu.regs.sp, 0x0010);
        assert!(cpu.regs.f.h() && !cpu.regs.f.z() && !cpu.regs.f.n());

        let (mut cpu, mut mem) = cpu_running(&[0xF8, 0xFF]); // LD HL, SP-1
        cpu.regs.sp = 0x00FF;
        assert_eq!(cpu.step(&mut mem), 12);
        assert_eq!(cpu.regs.hl(), 0x00FE);
        assert!(cpu.regs.f.h() && cpu.regs.f.c());
    }

    #[test]
    fn scf_and_ccf_manage_the_carry() {
        let (mut cpu, mut mem) = cpu_running(&[0x37, 0x3F]); // SCF ; CCF
        cpu.step(&mut mem);
        assert!(cpu.regs.f.c());
        cpu.step(&mut mem);
        assert!(!cpu.regs.f.c());
    }

    #[test]
    fn rlca_clears_zero_even_when_result_is_zero() {
        let (mut cpu, mut mem) = cpu_running(&[0x07]); // RLCA
        cpu.regs.a = 0x00;
        cpu.step(&mut mem);
        assert!(!cpu.regs.f.z());
    }

    #[test]
    fn illegal_opcode_locks_cpu_and_reports_fault() {
        let (mut cpu, mut mem) = cpu_running(&[0xD3]);
        assert_eq!(cpu.step(&mut mem), 4);
        assert!(matches!(
            cpu.fault(),
            Some(crate::cpu::CpuFault::IllegalOpcode { opcode: 0xD3, .. })
        ));
        // Subsequent steps are idle and PC does not advance.
        let pc = cpu.regs.pc;
        assert_eq!(cpu.step(&mut mem), 4);
        assert_eq!(cpu.regs.pc, pc);
    }
}

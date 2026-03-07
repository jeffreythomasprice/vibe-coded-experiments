use super::instruction::{Instruction, Operand, Program, RegisterId};
use super::memory::Memory;
use super::registers::RegisterFile;

#[derive(Debug, Clone, PartialEq)]
pub enum TickResult {
    Halted,
    BudgetExhausted,
    ProgramWrapped,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Scan,
    Fire,
}

pub struct VmState {
    pub program: Program,
    pub pc: usize,
    pub registers: RegisterFile,
    pub memory: Memory,
    pub executed_pcs: Vec<usize>,
}

impl VmState {
    pub fn new(program: Program) -> Self {
        Self {
            program,
            pc: 0,
            registers: RegisterFile::new(),
            memory: Memory::new(),
            executed_pcs: Vec::new(),
        }
    }

    fn resolve(&self, op: &Operand) -> u32 {
        match op {
            Operand::Register(id) => self.registers.read(*id),
            Operand::Immediate(v) => *v as u32,
            Operand::ImmediateFloat(v) => v.to_bits(),
            Operand::Label(_) => panic!("unresolved label in executor"),
        }
    }

    fn resolve_i32(&self, op: &Operand) -> i32 {
        self.resolve(op) as i32
    }

    fn resolve_f32(&self, op: &Operand) -> f32 {
        f32::from_bits(self.resolve(op))
    }
}

const SCAN_COST: u32 = 5;
const FIRE_COST: u32 = 3;

pub fn execute_tick(state: &mut VmState, cycle_budget: u32) -> (TickResult, Vec<Action>) {
    let mut actions = Vec::new();
    let result = execute_tick_with_handler(state, cycle_budget, |_state, action| {
        actions.push(action);
    });
    (result, actions)
}

pub fn execute_tick_with_handler(
    state: &mut VmState,
    cycle_budget: u32,
    mut on_action: impl FnMut(&mut VmState, Action),
) -> TickResult {
    let mut budget = cycle_budget;
    let prog_len = state.program.instructions.len();
    state.executed_pcs.clear();

    if prog_len == 0 {
        return TickResult::Halted;
    }

    loop {
        if state.pc >= prog_len {
            state.pc = 0;
            return TickResult::ProgramWrapped;
        }

        state.executed_pcs.push(state.pc);
        let instr = state.program.instructions[state.pc].clone();
        let cost = match &instr {
            Instruction::Scan => SCAN_COST,
            Instruction::Fire => FIRE_COST,
            _ => 1,
        };

        if budget < cost {
            return TickResult::BudgetExhausted;
        }
        budget -= cost;

        let mut advance_pc = true;

        match instr {
            Instruction::Add { dst, ref src1, ref src2 } => {
                let a = state.resolve_i32(src1);
                let b = state.resolve_i32(src2);
                state.registers.write_i32(dst, a.wrapping_add(b)).unwrap();
            }
            Instruction::Sub { dst, ref src1, ref src2 } => {
                let a = state.resolve_i32(src1);
                let b = state.resolve_i32(src2);
                state.registers.write_i32(dst, a.wrapping_sub(b)).unwrap();
            }
            Instruction::Mul { dst, ref src1, ref src2 } => {
                let a = state.resolve_i32(src1);
                let b = state.resolve_i32(src2);
                state.registers.write_i32(dst, a.wrapping_mul(b)).unwrap();
            }
            Instruction::Div { dst, ref src1, ref src2 } => {
                let a = state.resolve_i32(src1);
                let b = state.resolve_i32(src2);
                let result = if b == 0 { 0 } else { a.wrapping_div(b) };
                state.registers.write_i32(dst, result).unwrap();
            }
            Instruction::Mod { dst, ref src1, ref src2 } => {
                let a = state.resolve_i32(src1);
                let b = state.resolve_i32(src2);
                let result = if b == 0 { 0 } else { a.wrapping_rem(b) };
                state.registers.write_i32(dst, result).unwrap();
            }
            Instruction::Neg { dst, ref src } => {
                let a = state.resolve_i32(src);
                state.registers.write_i32(dst, a.wrapping_neg()).unwrap();
            }
            Instruction::And { dst, ref src1, ref src2 } => {
                let a = state.resolve(src1);
                let b = state.resolve(src2);
                state.registers.write(dst, a & b).unwrap();
            }
            Instruction::Or { dst, ref src1, ref src2 } => {
                let a = state.resolve(src1);
                let b = state.resolve(src2);
                state.registers.write(dst, a | b).unwrap();
            }
            Instruction::Xor { dst, ref src1, ref src2 } => {
                let a = state.resolve(src1);
                let b = state.resolve(src2);
                state.registers.write(dst, a ^ b).unwrap();
            }
            Instruction::Not { dst, ref src } => {
                let a = state.resolve(src);
                state.registers.write(dst, !a).unwrap();
            }
            Instruction::Shl { dst, ref src1, ref src2 } => {
                let a = state.resolve_i32(src1);
                let b = state.resolve_i32(src2);
                let shift = (b as u32) & 31;
                state.registers.write_i32(dst, a.wrapping_shl(shift)).unwrap();
            }
            Instruction::Shr { dst, ref src1, ref src2 } => {
                let a = state.resolve_i32(src1);
                let b = state.resolve_i32(src2);
                let shift = (b as u32) & 31;
                state.registers.write_i32(dst, a.wrapping_shr(shift)).unwrap();
            }

            Instruction::FAdd { dst, ref src1, ref src2 } => {
                let a = state.resolve_f32(src1);
                let b = state.resolve_f32(src2);
                state.registers.write_f32(dst, a + b).unwrap();
            }
            Instruction::FSub { dst, ref src1, ref src2 } => {
                let a = state.resolve_f32(src1);
                let b = state.resolve_f32(src2);
                state.registers.write_f32(dst, a - b).unwrap();
            }
            Instruction::FMul { dst, ref src1, ref src2 } => {
                let a = state.resolve_f32(src1);
                let b = state.resolve_f32(src2);
                state.registers.write_f32(dst, a * b).unwrap();
            }
            Instruction::FDiv { dst, ref src1, ref src2 } => {
                let a = state.resolve_f32(src1);
                let b = state.resolve_f32(src2);
                state.registers.write_f32(dst, a / b).unwrap();
            }
            Instruction::FNeg { dst, ref src } => {
                let a = state.resolve_f32(src);
                state.registers.write_f32(dst, -a).unwrap();
            }
            Instruction::FSin { dst, ref src } => {
                let a = state.resolve_f32(src);
                state.registers.write_f32(dst, a.sin()).unwrap();
            }
            Instruction::FCos { dst, ref src } => {
                let a = state.resolve_f32(src);
                state.registers.write_f32(dst, a.cos()).unwrap();
            }
            Instruction::FSqrt { dst, ref src } => {
                let a = state.resolve_f32(src);
                state.registers.write_f32(dst, a.sqrt()).unwrap();
            }

            Instruction::FCeil { dst, src } => {
                let a = state.registers.read_f32(src);
                state.registers.write_i32(dst, a.ceil() as i32).unwrap();
            }
            Instruction::FFloor { dst, src } => {
                let a = state.registers.read_f32(src);
                state.registers.write_i32(dst, a.floor() as i32).unwrap();
            }
            Instruction::FRound { dst, src } => {
                let a = state.registers.read_f32(src);
                state.registers.write_i32(dst, a.round_ties_even() as i32).unwrap();
            }

            Instruction::ItoF { dst, ref src } => {
                let a = state.resolve_i32(src);
                state.registers.write_f32(dst, a as f32).unwrap();
            }
            Instruction::FtoI { dst, ref src } => {
                let a = state.resolve_f32(src);
                state.registers.write_i32(dst, a as i32).unwrap();
            }

            Instruction::Mov { dst, ref src } => {
                let val = state.resolve(src);
                state.registers.write(dst, val).unwrap();
            }

            Instruction::Ld { dst, ref addr } => {
                let a = state.resolve(addr) as u16;
                let val = state.memory.read(a).unwrap();
                state.registers.write(dst, val).unwrap();
            }
            Instruction::St { ref src, ref addr } => {
                let val = state.resolve(src);
                let a = state.resolve(addr) as u16;
                state.memory.write(a, val).unwrap();
            }
            Instruction::Ldr { dst, base, ref offset } => {
                let base_val = state.registers.read(base);
                let off = state.resolve(offset);
                let addr = base_val.wrapping_add(off) as u16;
                let val = state.memory.read(addr).unwrap();
                state.registers.write(dst, val).unwrap();
            }
            Instruction::Str { ref src, base, ref offset } => {
                let val = state.resolve(src);
                let base_val = state.registers.read(base);
                let off = state.resolve(offset);
                let addr = base_val.wrapping_add(off) as u16;
                state.memory.write(addr, val).unwrap();
            }

            Instruction::Push { ref src } => {
                let val = state.resolve(src);
                let sp = state.registers.read(RegisterId::Sp).wrapping_sub(1);
                state.registers.write(RegisterId::Sp, sp).unwrap();
                state.memory.write(sp as u16, val).unwrap();
            }
            Instruction::Pop { dst } => {
                let sp = state.registers.read(RegisterId::Sp);
                let val = state.memory.read(sp as u16).unwrap();
                state.registers.write(RegisterId::Sp, sp.wrapping_add(1)).unwrap();
                state.registers.write(dst, val).unwrap();
            }

            Instruction::Cmp { ref src1, ref src2 } => {
                let a = state.resolve_i32(src1);
                let b = state.resolve_i32(src2);
                state.registers.flags.equal = a == b;
                state.registers.flags.less = a < b;
                state.registers.flags.greater = a > b;
            }
            Instruction::FCmp { ref src1, ref src2 } => {
                let a = state.resolve_f32(src1);
                let b = state.resolve_f32(src2);
                state.registers.flags.equal = a == b;
                state.registers.flags.less = a < b;
                state.registers.flags.greater = a > b;
            }

            Instruction::Jmp { addr } => {
                state.pc = addr;
                advance_pc = false;
            }
            Instruction::Jeq { addr } => {
                if state.registers.flags.equal {
                    state.pc = addr;
                    advance_pc = false;
                }
            }
            Instruction::Jne { addr } => {
                if !state.registers.flags.equal {
                    state.pc = addr;
                    advance_pc = false;
                }
            }
            Instruction::Jlt { addr } => {
                if state.registers.flags.less {
                    state.pc = addr;
                    advance_pc = false;
                }
            }
            Instruction::Jgt { addr } => {
                if state.registers.flags.greater {
                    state.pc = addr;
                    advance_pc = false;
                }
            }
            Instruction::Jle { addr } => {
                if state.registers.flags.less || state.registers.flags.equal {
                    state.pc = addr;
                    advance_pc = false;
                }
            }
            Instruction::Jge { addr } => {
                if state.registers.flags.greater || state.registers.flags.equal {
                    state.pc = addr;
                    advance_pc = false;
                }
            }

            Instruction::Call { addr } => {
                let return_addr = (state.pc + 1) as u32;
                let sp = state.registers.read(RegisterId::Sp).wrapping_sub(1);
                state.registers.write(RegisterId::Sp, sp).unwrap();
                state.memory.write(sp as u16, return_addr).unwrap();
                state.pc = addr;
                advance_pc = false;
            }
            Instruction::Ret => {
                let sp = state.registers.read(RegisterId::Sp);
                let return_addr = state.memory.read(sp as u16).unwrap();
                state.registers.write(RegisterId::Sp, sp.wrapping_add(1)).unwrap();
                state.pc = return_addr as usize;
                advance_pc = false;
            }

            Instruction::Scan => {
                on_action(state, Action::Scan);
            }
            Instruction::Fire => {
                on_action(state, Action::Fire);
            }

            Instruction::Nop => {}

            Instruction::Yield => {
                state.pc += 1;
                return TickResult::Halted;
            }
        }

        if advance_pc {
            state.pc += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::instruction::{Instruction, Operand, Program, RegisterId};

    fn make_state(instructions: Vec<Instruction>) -> VmState {
        VmState::new(Program { instructions })
    }

    fn reg(id: RegisterId) -> Operand {
        Operand::Register(id)
    }

    fn imm(v: i32) -> Operand {
        Operand::Immediate(v)
    }

    fn immf(v: f32) -> Operand {
        Operand::ImmediateFloat(v)
    }

    #[test]
    fn simple_program_several_ticks() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::R0, src: imm(1) },
            Instruction::Add { dst: RegisterId::R0, src1: reg(RegisterId::R0), src2: imm(1) },
            Instruction::Yield,
        ]);

        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::Halted);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 2);
        assert_eq!(state.pc, 3);

        state.pc = 0;
        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::Halted);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 2);
    }

    #[test]
    fn arithmetic_integer_overflow() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::R0, src: imm(i32::MAX) },
            Instruction::Add { dst: RegisterId::R1, src1: reg(RegisterId::R0), src2: imm(1) },
        ]);
        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::ProgramWrapped);
        assert_eq!(state.registers.read_i32(RegisterId::R1), i32::MIN);
    }

    #[test]
    fn division_by_zero() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::R0, src: imm(42) },
            Instruction::Div { dst: RegisterId::R1, src1: reg(RegisterId::R0), src2: imm(0) },
            Instruction::Mod { dst: RegisterId::R2, src1: reg(RegisterId::R0), src2: imm(0) },
        ]);
        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::ProgramWrapped);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 0);
        assert_eq!(state.registers.read_i32(RegisterId::R2), 0);
    }

    #[test]
    fn branch_jeq() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::R0, src: imm(5) },
            Instruction::Cmp { src1: reg(RegisterId::R0), src2: imm(5) },
            Instruction::Jeq { addr: 4 },
            Instruction::Mov { dst: RegisterId::R1, src: imm(99) }, // skipped
            Instruction::Mov { dst: RegisterId::R2, src: imm(42) },
        ]);
        let (_, _) = execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 0);
        assert_eq!(state.registers.read_i32(RegisterId::R2), 42);
    }

    #[test]
    fn branch_jne() {
        let mut state = make_state(vec![
            Instruction::Cmp { src1: imm(1), src2: imm(2) },
            Instruction::Jne { addr: 3 },
            Instruction::Mov { dst: RegisterId::R0, src: imm(99) }, // skipped
            Instruction::Mov { dst: RegisterId::R1, src: imm(42) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 0);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 42);
    }

    #[test]
    fn branch_jlt() {
        let mut state = make_state(vec![
            Instruction::Cmp { src1: imm(1), src2: imm(2) },
            Instruction::Jlt { addr: 3 },
            Instruction::Mov { dst: RegisterId::R0, src: imm(99) },
            Instruction::Mov { dst: RegisterId::R1, src: imm(1) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 0);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 1);
    }

    #[test]
    fn branch_jgt() {
        let mut state = make_state(vec![
            Instruction::Cmp { src1: imm(3), src2: imm(2) },
            Instruction::Jgt { addr: 3 },
            Instruction::Mov { dst: RegisterId::R0, src: imm(99) },
            Instruction::Mov { dst: RegisterId::R1, src: imm(1) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 0);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 1);
    }

    #[test]
    fn branch_jle() {
        let mut state = make_state(vec![
            Instruction::Cmp { src1: imm(2), src2: imm(2) },
            Instruction::Jle { addr: 3 },
            Instruction::Mov { dst: RegisterId::R0, src: imm(99) },
            Instruction::Mov { dst: RegisterId::R1, src: imm(1) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 1);

        let mut state = make_state(vec![
            Instruction::Cmp { src1: imm(1), src2: imm(2) },
            Instruction::Jle { addr: 3 },
            Instruction::Mov { dst: RegisterId::R0, src: imm(99) },
            Instruction::Mov { dst: RegisterId::R1, src: imm(1) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 1);
    }

    #[test]
    fn branch_jge() {
        let mut state = make_state(vec![
            Instruction::Cmp { src1: imm(2), src2: imm(2) },
            Instruction::Jge { addr: 3 },
            Instruction::Mov { dst: RegisterId::R0, src: imm(99) },
            Instruction::Mov { dst: RegisterId::R1, src: imm(1) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 1);

        let mut state = make_state(vec![
            Instruction::Cmp { src1: imm(3), src2: imm(2) },
            Instruction::Jge { addr: 3 },
            Instruction::Mov { dst: RegisterId::R0, src: imm(99) },
            Instruction::Mov { dst: RegisterId::R1, src: imm(1) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 1);
    }

    #[test]
    fn branch_not_taken() {
        let mut state = make_state(vec![
            Instruction::Cmp { src1: imm(1), src2: imm(2) },
            Instruction::Jeq { addr: 3 },
            Instruction::Mov { dst: RegisterId::R0, src: imm(42) },
            Instruction::Nop,
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 42);
    }

    #[test]
    fn stack_push_pop() {
        let mut state = make_state(vec![
            Instruction::Push { src: imm(10) },
            Instruction::Push { src: imm(20) },
            Instruction::Push { src: imm(30) },
            Instruction::Pop { dst: RegisterId::R0 },
            Instruction::Pop { dst: RegisterId::R1 },
            Instruction::Pop { dst: RegisterId::R2 },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 30);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 20);
        assert_eq!(state.registers.read_i32(RegisterId::R2), 10);
        assert_eq!(state.registers.read(RegisterId::Sp), 4095);
    }

    #[test]
    fn call_ret() {
        let mut state = make_state(vec![
            Instruction::Call { addr: 3 },
            Instruction::Mov { dst: RegisterId::R1, src: imm(99) },
            Instruction::Yield,
            Instruction::Mov { dst: RegisterId::R0, src: imm(42) },
            Instruction::Ret,
        ]);
        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::Halted);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 42);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 99);
    }

    #[test]
    fn nested_call_ret() {
        let mut state = make_state(vec![
            Instruction::Call { addr: 4 },
            Instruction::Mov { dst: RegisterId::R2, src: imm(3) },
            Instruction::Yield,
            Instruction::Nop,
            Instruction::Mov { dst: RegisterId::R0, src: imm(1) },
            Instruction::Call { addr: 8 },
            Instruction::Mov { dst: RegisterId::R1, src: imm(2) },
            Instruction::Ret,
            Instruction::Mov { dst: RegisterId::R3, src: imm(4) },
            Instruction::Ret,
        ]);
        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::Halted);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 1);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 2);
        assert_eq!(state.registers.read_i32(RegisterId::R2), 3);
        assert_eq!(state.registers.read_i32(RegisterId::R3), 4);
    }

    #[test]
    fn cycle_budget_exhaustion() {
        let mut state = make_state(vec![
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
            Instruction::Nop,
        ]);
        let (result, _) = execute_tick(&mut state, 3);
        assert_eq!(result, TickResult::BudgetExhausted);
        assert_eq!(state.pc, 3);
    }

    #[test]
    fn scan_fire_cycle_costs() {
        let mut state = make_state(vec![
            Instruction::Scan,
            Instruction::Fire,
            Instruction::Nop,
        ]);
        let (result, actions) = execute_tick(&mut state, 8);
        assert_eq!(result, TickResult::BudgetExhausted);
        assert_eq!(actions, vec![Action::Scan, Action::Fire]);
        assert_eq!(state.pc, 2);
    }

    #[test]
    fn scan_fire_actions_collected() {
        let mut state = make_state(vec![
            Instruction::Scan,
            Instruction::Fire,
            Instruction::Scan,
        ]);
        let (_, actions) = execute_tick(&mut state, 100);
        assert_eq!(actions, vec![Action::Scan, Action::Fire, Action::Scan]);
    }

    #[test]
    fn yield_stops_tick_resumes_next() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::R0, src: imm(1) },
            Instruction::Yield,
            Instruction::Mov { dst: RegisterId::R0, src: imm(2) },
            Instruction::Yield,
        ]);

        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::Halted);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 1);
        assert_eq!(state.pc, 2);

        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::Halted);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 2);
        assert_eq!(state.pc, 4);
    }

    #[test]
    fn program_wrapping() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::R0, src: imm(1) },
            Instruction::Mov { dst: RegisterId::R1, src: imm(2) },
        ]);
        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::ProgramWrapped);
        assert_eq!(state.pc, 0);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 1);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 2);
    }

    #[test]
    fn yield_at_end_wraps_next_tick() {
        let mut state = make_state(vec![
            Instruction::Nop,
            Instruction::Yield,
        ]);
        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::Halted);
        assert_eq!(state.pc, 2);

        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::ProgramWrapped);
        assert_eq!(state.pc, 0);
    }

    #[test]
    fn empty_program() {
        let mut state = make_state(vec![]);
        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::Halted);
    }

    #[test]
    fn memory_load_store() {
        let mut state = make_state(vec![
            Instruction::St { src: imm(42), addr: imm(100) },
            Instruction::Ld { dst: RegisterId::R0, addr: imm(100) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 42);
    }

    #[test]
    fn memory_ldr_str() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::R5, src: imm(100) },
            Instruction::Str { src: imm(77), base: RegisterId::R5, offset: imm(5) },
            Instruction::Ldr { dst: RegisterId::R0, base: RegisterId::R5, offset: imm(5) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 77);
    }

    #[test]
    fn float_arithmetic() {
        let mut state = make_state(vec![
            Instruction::FAdd { dst: RegisterId::F0, src1: immf(1.5), src2: immf(2.5) },
            Instruction::FSub { dst: RegisterId::F1, src1: immf(5.0), src2: immf(3.0) },
            Instruction::FMul { dst: RegisterId::F2, src1: immf(2.0), src2: immf(3.0) },
            Instruction::FDiv { dst: RegisterId::F3, src1: immf(10.0), src2: immf(4.0) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_f32(RegisterId::F0), 4.0);
        assert_eq!(state.registers.read_f32(RegisterId::F1), 2.0);
        assert_eq!(state.registers.read_f32(RegisterId::F2), 6.0);
        assert_eq!(state.registers.read_f32(RegisterId::F3), 2.5);
    }

    #[test]
    fn float_unary() {
        let mut state = make_state(vec![
            Instruction::FNeg { dst: RegisterId::F0, src: immf(3.0) },
            Instruction::FSqrt { dst: RegisterId::F1, src: immf(9.0) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_f32(RegisterId::F0), -3.0);
        assert_eq!(state.registers.read_f32(RegisterId::F1), 3.0);
    }

    #[test]
    fn itof_ftoi() {
        let mut state = make_state(vec![
            Instruction::ItoF { dst: RegisterId::F0, src: imm(42) },
            Instruction::FtoI { dst: RegisterId::R1, src: immf(3.7) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_f32(RegisterId::F0), 42.0);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 3);
    }

    #[test]
    fn fcmp_branch() {
        let mut state = make_state(vec![
            Instruction::FCmp { src1: immf(1.0), src2: immf(2.0) },
            Instruction::Jlt { addr: 3 },
            Instruction::Mov { dst: RegisterId::R0, src: imm(99) },
            Instruction::Mov { dst: RegisterId::R1, src: imm(1) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 0);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 1);
    }

    #[test]
    fn bitwise_ops() {
        let mut state = make_state(vec![
            Instruction::And { dst: RegisterId::R0, src1: imm(0xFF), src2: imm(0x0F) },
            Instruction::Or { dst: RegisterId::R1, src1: imm(0xF0), src2: imm(0x0F) },
            Instruction::Xor { dst: RegisterId::R2, src1: imm(0xFF), src2: imm(0x0F) },
            Instruction::Not { dst: RegisterId::R3, src: imm(0) },
            Instruction::Shl { dst: RegisterId::R4, src1: imm(1), src2: imm(4) },
            Instruction::Shr { dst: RegisterId::R5, src1: imm(16), src2: imm(2) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read(RegisterId::R0), 0x0F);
        assert_eq!(state.registers.read(RegisterId::R1), 0xFF);
        assert_eq!(state.registers.read(RegisterId::R2), 0xF0);
        assert_eq!(state.registers.read(RegisterId::R3), 0xFFFFFFFF);
        assert_eq!(state.registers.read_i32(RegisterId::R4), 16);
        assert_eq!(state.registers.read_i32(RegisterId::R5), 4);
    }

    #[test]
    fn neg_instruction() {
        let mut state = make_state(vec![
            Instruction::Neg { dst: RegisterId::R0, src: imm(42) },
            Instruction::Neg { dst: RegisterId::R1, src: imm(-1) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), -42);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 1);
    }

    #[test]
    fn jmp_unconditional() {
        let mut state = make_state(vec![
            Instruction::Jmp { addr: 2 },
            Instruction::Mov { dst: RegisterId::R0, src: imm(99) },
            Instruction::Mov { dst: RegisterId::R1, src: imm(1) },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 0);
        assert_eq!(state.registers.read_i32(RegisterId::R1), 1);
    }

    #[test]
    fn loop_with_budget() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::R0, src: imm(0) },
            Instruction::Add { dst: RegisterId::R0, src1: reg(RegisterId::R0), src2: imm(1) },
            Instruction::Cmp { src1: reg(RegisterId::R0), src2: imm(5) },
            Instruction::Jlt { addr: 1 },
        ]);
        let (result, _) = execute_tick(&mut state, 100);
        assert_eq!(result, TickResult::ProgramWrapped);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 5);
    }

    #[test]
    fn fceil_instruction() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::F0, src: immf(2.3) },
            Instruction::FCeil { dst: RegisterId::R0, src: RegisterId::F0 },
            Instruction::Mov { dst: RegisterId::F1, src: immf(-2.3) },
            Instruction::FCeil { dst: RegisterId::R1, src: RegisterId::F1 },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 3);
        assert_eq!(state.registers.read_i32(RegisterId::R1), -2);
    }

    #[test]
    fn ffloor_instruction() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::F0, src: immf(2.7) },
            Instruction::FFloor { dst: RegisterId::R0, src: RegisterId::F0 },
            Instruction::Mov { dst: RegisterId::F1, src: immf(-2.7) },
            Instruction::FFloor { dst: RegisterId::R1, src: RegisterId::F1 },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 2);
        assert_eq!(state.registers.read_i32(RegisterId::R1), -3);
    }

    #[test]
    fn fround_instruction() {
        let mut state = make_state(vec![
            Instruction::Mov { dst: RegisterId::F0, src: immf(2.5) },
            Instruction::FRound { dst: RegisterId::R0, src: RegisterId::F0 },
            Instruction::Mov { dst: RegisterId::F1, src: immf(3.5) },
            Instruction::FRound { dst: RegisterId::R1, src: RegisterId::F1 },
            Instruction::Mov { dst: RegisterId::F2, src: immf(2.3) },
            Instruction::FRound { dst: RegisterId::R2, src: RegisterId::F2 },
        ]);
        execute_tick(&mut state, 100);
        assert_eq!(state.registers.read_i32(RegisterId::R0), 2); // ties to even
        assert_eq!(state.registers.read_i32(RegisterId::R1), 4); // ties to even
        assert_eq!(state.registers.read_i32(RegisterId::R2), 2);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegisterBank {
    Integer,
    Float,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegisterId {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,

    F0,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,

    Sp,
    Spd,
    Trn,
    Trt,

    Hp,
    X,
    Y,
    Hdg,
    Thr,
    SpdCur,
    ScDist,
    ScType,
    ScId,
    Tick,
    Rnd,
}

impl RegisterId {
    pub fn bank(self) -> RegisterBank {
        match self {
            RegisterId::R0
            | RegisterId::R1
            | RegisterId::R2
            | RegisterId::R3
            | RegisterId::R4
            | RegisterId::R5
            | RegisterId::R6
            | RegisterId::R7
            | RegisterId::Sp
            | RegisterId::Hp
            | RegisterId::Tick
            | RegisterId::ScType
            | RegisterId::ScId
            | RegisterId::Rnd => RegisterBank::Integer,

            RegisterId::F0
            | RegisterId::F1
            | RegisterId::F2
            | RegisterId::F3
            | RegisterId::F4
            | RegisterId::F5
            | RegisterId::F6
            | RegisterId::F7
            | RegisterId::Spd
            | RegisterId::Trn
            | RegisterId::Trt
            | RegisterId::X
            | RegisterId::Y
            | RegisterId::Hdg
            | RegisterId::Thr
            | RegisterId::SpdCur
            | RegisterId::ScDist => RegisterBank::Float,
        }
    }

    pub fn is_writable(self) -> bool {
        matches!(
            self,
            RegisterId::R0
                | RegisterId::R1
                | RegisterId::R2
                | RegisterId::R3
                | RegisterId::R4
                | RegisterId::R5
                | RegisterId::R6
                | RegisterId::R7
                | RegisterId::F0
                | RegisterId::F1
                | RegisterId::F2
                | RegisterId::F3
                | RegisterId::F4
                | RegisterId::F5
                | RegisterId::F6
                | RegisterId::F7
                | RegisterId::Sp
                | RegisterId::Spd
                | RegisterId::Trn
                | RegisterId::Trt
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Register(RegisterId),
    Immediate(i32),
    ImmediateFloat(f32),
    Label(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    // Arithmetic integer
    Add { dst: RegisterId, src1: Operand, src2: Operand },
    Sub { dst: RegisterId, src1: Operand, src2: Operand },
    Mul { dst: RegisterId, src1: Operand, src2: Operand },
    Div { dst: RegisterId, src1: Operand, src2: Operand },
    Mod { dst: RegisterId, src1: Operand, src2: Operand },
    Neg { dst: RegisterId, src: Operand },
    And { dst: RegisterId, src1: Operand, src2: Operand },
    Or { dst: RegisterId, src1: Operand, src2: Operand },
    Xor { dst: RegisterId, src1: Operand, src2: Operand },
    Not { dst: RegisterId, src: Operand },
    Shl { dst: RegisterId, src1: Operand, src2: Operand },
    Shr { dst: RegisterId, src1: Operand, src2: Operand },

    // Arithmetic float
    FAdd { dst: RegisterId, src1: Operand, src2: Operand },
    FSub { dst: RegisterId, src1: Operand, src2: Operand },
    FMul { dst: RegisterId, src1: Operand, src2: Operand },
    FDiv { dst: RegisterId, src1: Operand, src2: Operand },
    FNeg { dst: RegisterId, src: Operand },
    FSin { dst: RegisterId, src: Operand },
    FCos { dst: RegisterId, src: Operand },
    FSqrt { dst: RegisterId, src: Operand },

    // Rounding (cross-bank: int dst, float src)
    FCeil { dst: RegisterId, src: RegisterId },
    FFloor { dst: RegisterId, src: RegisterId },
    FRound { dst: RegisterId, src: RegisterId },

    // Conversion
    ItoF { dst: RegisterId, src: Operand },
    FtoI { dst: RegisterId, src: Operand },

    // Data movement
    Mov { dst: RegisterId, src: Operand },
    Ld { dst: RegisterId, addr: Operand },
    St { src: Operand, addr: Operand },
    Ldr { dst: RegisterId, base: RegisterId, offset: Operand },
    Str { src: Operand, base: RegisterId, offset: Operand },
    Push { src: Operand },
    Pop { dst: RegisterId },

    // Compare/branch
    Cmp { src1: Operand, src2: Operand },
    FCmp { src1: Operand, src2: Operand },
    Jmp { addr: usize },
    Jeq { addr: usize },
    Jne { addr: usize },
    Jlt { addr: usize },
    Jgt { addr: usize },
    Jle { addr: usize },
    Jge { addr: usize },
    Call { addr: usize },
    Ret,

    // Actions
    Scan,
    Fire,
    Nop,
    Yield,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub instructions: Vec<Instruction>,
}

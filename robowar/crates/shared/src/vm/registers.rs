use super::instruction::RegisterId;
use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Default)]
pub struct Flags {
    pub equal: bool,
    pub less: bool,
    pub greater: bool,
}

#[derive(Debug, Clone)]
pub struct RegisterFile {
    int_gp: [u32; 8],
    float_gp: [u32; 8],
    pub flags: Flags,

    // Read/write special-purpose
    sp: u32,
    spd: u32,
    trn: u32,
    trt: u32,

    // Read-only special-purpose (updated by simulation)
    hp: u32,
    x: u32,
    y: u32,
    hdg: u32,
    thr: u32,
    spd_cur: u32,
    sc_dist: u32,
    sc_type: u32,
    sc_id: u32,
    tick: u32,
    rnd: u32,
}

impl RegisterFile {
    pub fn new() -> Self {
        Self {
            int_gp: [0; 8],
            float_gp: [0; 8],
            flags: Flags::default(),
            sp: 4095,
            spd: 0,
            trn: 0,
            trt: 0,
            hp: 0,
            x: 0,
            y: 0,
            hdg: 0,
            thr: 0,
            spd_cur: 0,
            sc_dist: 0,
            sc_type: 0,
            sc_id: 0,
            tick: 0,
            rnd: 0,
        }
    }

    pub fn read(&self, id: RegisterId) -> u32 {
        match id {
            RegisterId::R0 => self.int_gp[0],
            RegisterId::R1 => self.int_gp[1],
            RegisterId::R2 => self.int_gp[2],
            RegisterId::R3 => self.int_gp[3],
            RegisterId::R4 => self.int_gp[4],
            RegisterId::R5 => self.int_gp[5],
            RegisterId::R6 => self.int_gp[6],
            RegisterId::R7 => self.int_gp[7],
            RegisterId::F0 => self.float_gp[0],
            RegisterId::F1 => self.float_gp[1],
            RegisterId::F2 => self.float_gp[2],
            RegisterId::F3 => self.float_gp[3],
            RegisterId::F4 => self.float_gp[4],
            RegisterId::F5 => self.float_gp[5],
            RegisterId::F6 => self.float_gp[6],
            RegisterId::F7 => self.float_gp[7],
            RegisterId::Sp => self.sp,
            RegisterId::Spd => self.spd,
            RegisterId::Trn => self.trn,
            RegisterId::Trt => self.trt,
            RegisterId::Hp => self.hp,
            RegisterId::X => self.x,
            RegisterId::Y => self.y,
            RegisterId::Hdg => self.hdg,
            RegisterId::Thr => self.thr,
            RegisterId::SpdCur => self.spd_cur,
            RegisterId::ScDist => self.sc_dist,
            RegisterId::ScType => self.sc_type,
            RegisterId::ScId => self.sc_id,
            RegisterId::Tick => self.tick,
            RegisterId::Rnd => self.rnd,
        }
    }

    pub fn write(&mut self, id: RegisterId, val: u32) -> Result<()> {
        match id {
            RegisterId::R0 => self.int_gp[0] = val,
            RegisterId::R1 => self.int_gp[1] = val,
            RegisterId::R2 => self.int_gp[2] = val,
            RegisterId::R3 => self.int_gp[3] = val,
            RegisterId::R4 => self.int_gp[4] = val,
            RegisterId::R5 => self.int_gp[5] = val,
            RegisterId::R6 => self.int_gp[6] = val,
            RegisterId::R7 => self.int_gp[7] = val,
            RegisterId::F0 => self.float_gp[0] = val,
            RegisterId::F1 => self.float_gp[1] = val,
            RegisterId::F2 => self.float_gp[2] = val,
            RegisterId::F3 => self.float_gp[3] = val,
            RegisterId::F4 => self.float_gp[4] = val,
            RegisterId::F5 => self.float_gp[5] = val,
            RegisterId::F6 => self.float_gp[6] = val,
            RegisterId::F7 => self.float_gp[7] = val,
            RegisterId::Sp => self.sp = val,
            RegisterId::Spd => self.spd = val,
            RegisterId::Trn => self.trn = val,
            RegisterId::Trt => self.trt = val,
            RegisterId::Hp
            | RegisterId::X
            | RegisterId::Y
            | RegisterId::Hdg
            | RegisterId::Thr
            | RegisterId::SpdCur
            | RegisterId::ScDist
            | RegisterId::ScType
            | RegisterId::ScId
            | RegisterId::Tick
            | RegisterId::Rnd => {
                return Err(anyhow!("cannot write to read-only register {:?}", id));
            }
        }
        Ok(())
    }

    pub fn write_readonly(&mut self, id: RegisterId, val: u32) {
        match id {
            RegisterId::Hp => self.hp = val,
            RegisterId::X => self.x = val,
            RegisterId::Y => self.y = val,
            RegisterId::Hdg => self.hdg = val,
            RegisterId::Thr => self.thr = val,
            RegisterId::SpdCur => self.spd_cur = val,
            RegisterId::ScDist => self.sc_dist = val,
            RegisterId::ScType => self.sc_type = val,
            RegisterId::ScId => self.sc_id = val,
            RegisterId::Tick => self.tick = val,
            RegisterId::Rnd => self.rnd = val,
            other => self.write(other, val).expect("writable register"),
        }
    }

    pub fn read_i32(&self, id: RegisterId) -> i32 {
        self.read(id) as i32
    }

    pub fn write_i32(&mut self, id: RegisterId, val: i32) -> Result<()> {
        self.write(id, val as u32)
    }

    pub fn read_f32(&self, id: RegisterId) -> f32 {
        f32::from_bits(self.read(id))
    }

    pub fn write_f32(&mut self, id: RegisterId, val: f32) -> Result<()> {
        self.write(id, val.to_bits())
    }
}

impl Default for RegisterFile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_gp_read_write() {
        let mut regs = RegisterFile::new();
        regs.write(RegisterId::R0, 42).unwrap();
        assert_eq!(regs.read(RegisterId::R0), 42);
        regs.write(RegisterId::R7, 0xDEAD_BEEF).unwrap();
        assert_eq!(regs.read(RegisterId::R7), 0xDEAD_BEEF);
    }

    #[test]
    fn float_gp_read_write() {
        let mut regs = RegisterFile::new();
        regs.write_f32(RegisterId::F0, 3.14).unwrap();
        assert!((regs.read_f32(RegisterId::F0) - 3.14).abs() < f32::EPSILON);
        regs.write_f32(RegisterId::F7, -1.5).unwrap();
        assert_eq!(regs.read_f32(RegisterId::F7), -1.5);
    }

    #[test]
    fn rw_special_registers() {
        let mut regs = RegisterFile::new();
        assert_eq!(regs.read(RegisterId::Sp), 4095);
        regs.write(RegisterId::Sp, 4000).unwrap();
        assert_eq!(regs.read(RegisterId::Sp), 4000);

        regs.write_f32(RegisterId::Spd, 3.5).unwrap();
        assert_eq!(regs.read_f32(RegisterId::Spd), 3.5);

        regs.write_f32(RegisterId::Trn, -1.0).unwrap();
        assert_eq!(regs.read_f32(RegisterId::Trn), -1.0);

        regs.write_f32(RegisterId::Trt, 2.0).unwrap();
        assert_eq!(regs.read_f32(RegisterId::Trt), 2.0);
    }

    #[test]
    fn readonly_rejects_write() {
        let mut regs = RegisterFile::new();
        let read_only = [
            RegisterId::Hp,
            RegisterId::X,
            RegisterId::Y,
            RegisterId::Hdg,
            RegisterId::Thr,
            RegisterId::SpdCur,
            RegisterId::ScDist,
            RegisterId::ScType,
            RegisterId::ScId,
            RegisterId::Tick,
            RegisterId::Rnd,
        ];
        for id in read_only {
            assert!(regs.write(id, 1).is_err());
        }
    }

    #[test]
    fn write_readonly_from_simulation() {
        let mut regs = RegisterFile::new();
        regs.write_readonly(RegisterId::Hp, 100);
        assert_eq!(regs.read(RegisterId::Hp), 100);

        regs.write_readonly(RegisterId::X, 5.0f32.to_bits());
        assert_eq!(regs.read_f32(RegisterId::X), 5.0);

        regs.write_readonly(RegisterId::Tick, 42);
        assert_eq!(regs.read(RegisterId::Tick), 42);
    }

    #[test]
    fn i32_helpers() {
        let mut regs = RegisterFile::new();
        regs.write_i32(RegisterId::R0, -1).unwrap();
        assert_eq!(regs.read_i32(RegisterId::R0), -1);
        assert_eq!(regs.read(RegisterId::R0), u32::MAX);
    }

    #[test]
    fn f32_helpers() {
        let mut regs = RegisterFile::new();
        regs.write_f32(RegisterId::F0, std::f32::consts::PI).unwrap();
        let val = regs.read_f32(RegisterId::F0);
        assert!((val - std::f32::consts::PI).abs() < f32::EPSILON);
    }

    #[test]
    fn flags_default() {
        let regs = RegisterFile::new();
        assert!(!regs.flags.equal);
        assert!(!regs.flags.less);
        assert!(!regs.flags.greater);
    }

    #[test]
    fn all_int_gp_registers_independent() {
        let mut regs = RegisterFile::new();
        for i in 0u32..8 {
            let id = int_gp_register(i);
            regs.write(id, i + 100).unwrap();
        }
        for i in 0u32..8 {
            let id = int_gp_register(i);
            assert_eq!(regs.read(id), i + 100);
        }
    }

    #[test]
    fn all_float_gp_registers_independent() {
        let mut regs = RegisterFile::new();
        for i in 0u32..8 {
            let id = float_gp_register(i);
            regs.write_f32(id, i as f32 + 1.5).unwrap();
        }
        for i in 0u32..8 {
            let id = float_gp_register(i);
            assert_eq!(regs.read_f32(id), i as f32 + 1.5);
        }
    }

    fn int_gp_register(i: u32) -> RegisterId {
        match i {
            0 => RegisterId::R0,
            1 => RegisterId::R1,
            2 => RegisterId::R2,
            3 => RegisterId::R3,
            4 => RegisterId::R4,
            5 => RegisterId::R5,
            6 => RegisterId::R6,
            7 => RegisterId::R7,
            _ => panic!("invalid int GP register index"),
        }
    }

    fn float_gp_register(i: u32) -> RegisterId {
        match i {
            0 => RegisterId::F0,
            1 => RegisterId::F1,
            2 => RegisterId::F2,
            3 => RegisterId::F3,
            4 => RegisterId::F4,
            5 => RegisterId::F5,
            6 => RegisterId::F6,
            7 => RegisterId::F7,
            _ => panic!("invalid float GP register index"),
        }
    }
}

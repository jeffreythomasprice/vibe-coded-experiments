use anyhow::{bail, Result};

pub const MEMORY_SIZE: usize = 4096;

pub struct Memory {
    cells: [u32; MEMORY_SIZE],
}

impl Memory {
    pub fn new() -> Self {
        Self {
            cells: [0; MEMORY_SIZE],
        }
    }

    pub fn read(&self, addr: u16) -> Result<u32> {
        let idx = addr as usize;
        if idx >= MEMORY_SIZE {
            bail!("memory read out of bounds: address {addr}");
        }
        Ok(self.cells[idx])
    }

    pub fn write(&mut self, addr: u16, value: u32) -> Result<()> {
        let idx = addr as usize;
        if idx >= MEMORY_SIZE {
            bail!("memory write out of bounds: address {addr}");
        }
        self.cells[idx] = value;
        Ok(())
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_roundtrip() {
        let mut mem = Memory::new();
        mem.write(0, 42).unwrap();
        mem.write(100, 0xDEAD_BEEF).unwrap();
        assert_eq!(mem.read(0).unwrap(), 42);
        assert_eq!(mem.read(100).unwrap(), 0xDEAD_BEEF);
    }

    #[test]
    fn initialized_to_zero() {
        let mem = Memory::new();
        assert_eq!(mem.read(0).unwrap(), 0);
        assert_eq!(mem.read(4095).unwrap(), 0);
    }

    #[test]
    fn max_valid_address() {
        let mut mem = Memory::new();
        mem.write(4095, 99).unwrap();
        assert_eq!(mem.read(4095).unwrap(), 99);
    }

    #[test]
    fn out_of_bounds_read() {
        let mem = Memory::new();
        assert!(mem.read(4096).is_err());
    }

    #[test]
    fn out_of_bounds_write() {
        let mut mem = Memory::new();
        assert!(mem.write(4096, 1).is_err());
    }

    #[test]
    fn reinterpret_as_f32() {
        let mut mem = Memory::new();
        let val = 3.14_f32;
        mem.write(0, val.to_bits()).unwrap();
        assert_eq!(f32::from_bits(mem.read(0).unwrap()), val);
    }

    #[test]
    fn reinterpret_as_i32() {
        let mut mem = Memory::new();
        let val: i32 = -42;
        mem.write(0, val as u32).unwrap();
        assert_eq!(mem.read(0).unwrap() as i32, val);
    }
}

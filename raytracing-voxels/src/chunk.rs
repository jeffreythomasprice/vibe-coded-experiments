use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use glam::IVec3;

const SIZE: usize = 16;
const VOLUME: usize = SIZE * SIZE * SIZE;

pub struct Chunk {
    voxels: [u8; VOLUME],
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            voxels: [0; VOLUME],
        }
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> u8 {
        if x >= SIZE || y >= SIZE || z >= SIZE {
            return 0;
        }
        self.voxels[x + y * SIZE + z * SIZE * SIZE]
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, value: u8) {
        if x >= SIZE || y >= SIZE || z >= SIZE {
            return;
        }
        self.voxels[x + y * SIZE + z * SIZE * SIZE] = value;
    }

    pub fn data(&self) -> &[u8; VOLUME] {
        &self.voxels
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.voxels)
            .with_context(|| format!("failed to write chunk to {}", path.display()))
    }

    pub fn load_from_file(path: &Path) -> Result<Chunk> {
        let data = std::fs::read(path)
            .with_context(|| format!("failed to read chunk from {}", path.display()))?;
        let voxels: [u8; VOLUME] = data
            .try_into()
            .map_err(|v: Vec<u8>| {
                anyhow::anyhow!(
                    "chunk file {} has wrong size: expected {} bytes, got {}",
                    path.display(),
                    VOLUME,
                    v.len()
                )
            })?;
        Ok(Chunk { voxels })
    }
}

pub fn chunk_file_path(dir: &Path, pos: IVec3) -> PathBuf {
    dir.join(format!("chunk_{}_{}_{}", pos.x, pos.y, pos.z))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_all_zeros() {
        let chunk = Chunk::new();
        assert!(chunk.data().iter().all(|&v| v == 0));
    }

    #[test]
    fn set_get_roundtrip() {
        let mut chunk = Chunk::new();
        chunk.set(3, 7, 11, 42);
        assert_eq!(chunk.get(3, 7, 11), 42);
    }

    #[test]
    fn out_of_bounds_get_returns_zero() {
        let chunk = Chunk::new();
        assert_eq!(chunk.get(16, 0, 0), 0);
        assert_eq!(chunk.get(0, 16, 0), 0);
        assert_eq!(chunk.get(0, 0, 16), 0);
        assert_eq!(chunk.get(100, 100, 100), 0);
    }

    #[test]
    fn out_of_bounds_set_is_noop() {
        let mut chunk = Chunk::new();
        chunk.set(16, 0, 0, 1);
        chunk.set(0, 16, 0, 1);
        chunk.set(0, 0, 16, 1);
        assert!(chunk.data().iter().all(|&v| v == 0));
    }

    #[test]
    fn save_load_roundtrip() -> anyhow::Result<()> {
        let dir = std::env::temp_dir().join("chunk_test_save_load_roundtrip");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("test_chunk");

        let mut chunk = Chunk::new();
        chunk.set(0, 0, 0, 1);
        chunk.set(5, 10, 15, 42);
        chunk.set(15, 15, 15, 255);
        chunk.save_to_file(&path)?;

        let loaded = Chunk::load_from_file(&path)?;
        assert_eq!(chunk.data(), loaded.data());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn load_wrong_size_returns_error() -> anyhow::Result<()> {
        let dir = std::env::temp_dir().join("chunk_test_load_wrong_size");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("bad_chunk");

        std::fs::write(&path, &[0u8; 100])?;
        let result = Chunk::load_from_file(&path);
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn chunk_file_path_format() {
        use glam::IVec3;

        let dir = Path::new("/world/chunks");
        assert_eq!(
            super::chunk_file_path(dir, IVec3::new(1, 2, 3)),
            PathBuf::from("/world/chunks/chunk_1_2_3")
        );
        assert_eq!(
            super::chunk_file_path(dir, IVec3::new(-5, 0, 10)),
            PathBuf::from("/world/chunks/chunk_-5_0_10")
        );
    }

}

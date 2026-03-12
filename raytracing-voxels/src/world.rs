use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Vec3};

use crate::chunk::Chunk;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuChunkInfo {
    pub world_offset: Vec3,
    pub data_offset: u32,
}

pub struct World {
    chunks: HashMap<IVec3, Chunk>,
    dirty: bool,
}

impl World {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            dirty: false,
        }
    }

    pub fn insert(&mut self, pos: IVec3, chunk: Chunk) {
        self.chunks.insert(pos, chunk);
        self.dirty = true;
    }

    pub fn remove(&mut self, pos: &IVec3) -> Option<Chunk> {
        let removed = self.chunks.remove(pos);
        if removed.is_some() {
            self.dirty = true;
        }
        removed
    }

    pub fn get(&self, pos: &IVec3) -> Option<&Chunk> {
        self.chunks.get(pos)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&IVec3, &Chunk)> {
        self.chunks.iter()
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    pub fn pack_gpu_data(&self) -> (Vec<u8>, Vec<GpuChunkInfo>) {
        let mut keys: Vec<IVec3> = self.chunks.keys().copied().collect();
        keys.sort_by_key(|a| a.to_array());

        let mut voxel_data = Vec::with_capacity(keys.len() * 4096);
        let mut chunk_infos = Vec::with_capacity(keys.len());

        for (i, key) in keys.iter().enumerate() {
            let chunk = &self.chunks[key];
            voxel_data.extend_from_slice(chunk.data());

            chunk_infos.push(GpuChunkInfo {
                world_offset: key.as_vec3() * 16.0,
                data_offset: i as u32 * 1024, // 4096 bytes = 1024 u32s
            });
        }

        (voxel_data, chunk_infos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::generate_test_chunk;

    #[test]
    fn insert_and_get() {
        let mut world = World::new();
        let chunk = generate_test_chunk();
        world.insert(IVec3::ZERO, chunk);
        assert!(world.get(&IVec3::ZERO).is_some());
        assert!(world.get(&IVec3::X).is_none());
    }

    #[test]
    fn remove_returns_chunk_and_clears() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, generate_test_chunk());
        let removed = world.remove(&IVec3::ZERO);
        assert!(removed.is_some());
        assert!(world.get(&IVec3::ZERO).is_none());
    }

    #[test]
    fn iter_yields_all() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, generate_test_chunk());
        world.insert(IVec3::X, generate_test_chunk());
        world.insert(IVec3::Y, generate_test_chunk());
        assert_eq!(world.iter().count(), 3);
        assert_eq!(world.chunk_count(), 3);
    }

    #[test]
    fn pack_single_chunk() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, generate_test_chunk());
        let (voxel_data, infos) = world.pack_gpu_data();
        assert_eq!(voxel_data.len(), 4096);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].data_offset, 0);
        assert_eq!(infos[0].world_offset, Vec3::ZERO);
    }

    #[test]
    fn pack_two_chunks() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, generate_test_chunk());
        world.insert(IVec3::X, generate_test_chunk());
        let (voxel_data, infos) = world.pack_gpu_data();
        assert_eq!(voxel_data.len(), 8192);
        assert_eq!(infos.len(), 2);
        assert_eq!(infos[1].data_offset, 1024);
        assert_eq!(infos[1].world_offset, Vec3::new(16.0, 0.0, 0.0));
    }

    #[test]
    fn new_world_not_dirty() {
        let world = World::new();
        assert!(!world.is_dirty());
    }

    #[test]
    fn insert_sets_dirty() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, generate_test_chunk());
        assert!(world.is_dirty());
    }

    #[test]
    fn clear_dirty_resets() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, generate_test_chunk());
        world.clear_dirty();
        assert!(!world.is_dirty());
    }

    #[test]
    fn remove_sets_dirty() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, generate_test_chunk());
        world.clear_dirty();
        world.remove(&IVec3::ZERO);
        assert!(world.is_dirty());
    }
}

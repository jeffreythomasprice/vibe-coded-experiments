use std::collections::{HashMap, HashSet};

use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Vec3};

use crate::bvh::{self, GpuBvhNode};
use crate::chunk::Chunk;
use crate::mesh_catalog::{MeshCatalog, MESH_VOXEL_BASE};

const CHUNK_SIZE: i32 = 16;
pub const WATER_VOXEL_ID: u8 = 7;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuChunkInfo {
    pub world_offset: Vec3,
    pub data_offset: u32,
}

pub struct RaycastHit {
    pub chunk_pos: IVec3,
    pub local_pos: [usize; 3],
    pub normal: IVec3,
    pub voxel_id: u8,
}

pub struct World {
    chunks: HashMap<IVec3, Chunk>,
    dirty: bool,
    modified_chunks: HashSet<IVec3>,
}

impl World {
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            dirty: false,
            modified_chunks: HashSet::new(),
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
            self.modified_chunks.remove(pos);
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

    pub fn take_modified(&mut self) -> HashSet<IVec3> {
        std::mem::take(&mut self.modified_chunks)
    }

    pub fn is_chunk_modified(&self, pos: &IVec3) -> bool {
        self.modified_chunks.contains(pos)
    }

    pub fn mark_chunk_saved(&mut self, pos: &IVec3) {
        self.modified_chunks.remove(pos);
    }

    pub fn get_mut(&mut self, pos: &IVec3) -> Option<&mut Chunk> {
        self.chunks.get_mut(pos)
    }

    pub fn is_chunk_loaded(&self, chunk_pos: &IVec3) -> bool {
        self.chunks.contains_key(chunk_pos)
    }

    pub fn world_to_chunk(voxel_pos: IVec3) -> (IVec3, [usize; 3]) {
        let chunk_pos = IVec3::new(
            voxel_pos.x.div_euclid(CHUNK_SIZE),
            voxel_pos.y.div_euclid(CHUNK_SIZE),
            voxel_pos.z.div_euclid(CHUNK_SIZE),
        );
        let local = [
            voxel_pos.x.rem_euclid(CHUNK_SIZE) as usize,
            voxel_pos.y.rem_euclid(CHUNK_SIZE) as usize,
            voxel_pos.z.rem_euclid(CHUNK_SIZE) as usize,
        ];
        (chunk_pos, local)
    }

    pub fn set_voxel(&mut self, voxel_pos: IVec3, value: u8) -> bool {
        let (chunk_pos, local) = Self::world_to_chunk(voxel_pos);
        if let Some(chunk) = self.chunks.get_mut(&chunk_pos) {
            chunk.set(local[0], local[1], local[2], value);
            self.dirty = true;
            self.modified_chunks.insert(chunk_pos);
            true
        } else {
            false
        }
    }

    pub fn get_voxel(&self, voxel_pos: IVec3) -> u8 {
        let (chunk_pos, local) = Self::world_to_chunk(voxel_pos);
        self.chunks
            .get(&chunk_pos)
            .map_or(0, |c| c.get(local[0], local[1], local[2]))
    }

    pub fn is_solid_voxel(&self, voxel_pos: IVec3) -> bool {
        let v = self.get_voxel(voxel_pos);
        v != 0 && v != WATER_VOXEL_ID
    }

    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        mesh_catalog: Option<&MeshCatalog>,
    ) -> Option<RaycastHit> {
        let dir = direction.normalize();

        // Starting voxel coordinates
        let mut voxel = IVec3::new(
            origin.x.floor() as i32,
            origin.y.floor() as i32,
            origin.z.floor() as i32,
        );

        // Check if starting inside a solid voxel
        let id = self.get_voxel(voxel);
        if id != 0 {
            if id >= MESH_VOXEL_BASE {
                // Starting inside a mesh voxel — do a mesh raycast from origin
                if let Some(catalog) = mesh_catalog {
                    let voxel_min = Vec3::new(voxel.x as f32, voxel.y as f32, voxel.z as f32);
                    let local_origin = origin - voxel_min;
                    let mesh_type = id - MESH_VOXEL_BASE;
                    if let Some((_t, normal)) = catalog.raycast(mesh_type, local_origin, dir) {
                        let (chunk_pos, local_pos) = Self::world_to_chunk(voxel);
                        return Some(RaycastHit {
                            chunk_pos,
                            local_pos,
                            normal: IVec3::new(
                                normal.x.round() as i32,
                                normal.y.round() as i32,
                                normal.z.round() as i32,
                            ),
                            voxel_id: id,
                        });
                    }
                    // No mesh triangle hit — continue DDA
                }
                // No catalog provided — continue DDA
            } else if id != WATER_VOXEL_ID {
                let (chunk_pos, local_pos) = Self::world_to_chunk(voxel);
                return Some(RaycastHit {
                    chunk_pos,
                    local_pos,
                    normal: IVec3::ZERO,
                    voxel_id: id,
                });
            }
        }

        let step = IVec3::new(
            if dir.x >= 0.0 { 1 } else { -1 },
            if dir.y >= 0.0 { 1 } else { -1 },
            if dir.z >= 0.0 { 1 } else { -1 },
        );

        let t_delta = Vec3::new(
            if dir.x != 0.0 {
                (1.0 / dir.x).abs()
            } else {
                f32::MAX
            },
            if dir.y != 0.0 {
                (1.0 / dir.y).abs()
            } else {
                f32::MAX
            },
            if dir.z != 0.0 {
                (1.0 / dir.z).abs()
            } else {
                f32::MAX
            },
        );

        let mut t_max = Vec3::new(
            if dir.x >= 0.0 {
                ((voxel.x as f32 + 1.0) - origin.x) * t_delta.x
            } else {
                (origin.x - voxel.x as f32) * t_delta.x
            },
            if dir.y >= 0.0 {
                ((voxel.y as f32 + 1.0) - origin.y) * t_delta.y
            } else {
                (origin.y - voxel.y as f32) * t_delta.y
            },
            if dir.z >= 0.0 {
                ((voxel.z as f32 + 1.0) - origin.z) * t_delta.z
            } else {
                (origin.z - voxel.z as f32) * t_delta.z
            },
        );

        let mut normal;
        let max_dist_sq = max_distance * max_distance;

        loop {
            // Step along the axis with smallest t_max
            if t_max.x < t_max.y {
                if t_max.x < t_max.z {
                    voxel.x += step.x;
                    t_max.x += t_delta.x;
                    normal = IVec3::new(-step.x, 0, 0);
                } else {
                    voxel.z += step.z;
                    t_max.z += t_delta.z;
                    normal = IVec3::new(0, 0, -step.z);
                }
            } else if t_max.y < t_max.z {
                voxel.y += step.y;
                t_max.y += t_delta.y;
                normal = IVec3::new(0, -step.y, 0);
            } else {
                voxel.z += step.z;
                t_max.z += t_delta.z;
                normal = IVec3::new(0, 0, -step.z);
            }

            // Check distance
            let diff = Vec3::new(
                voxel.x as f32 + 0.5 - origin.x,
                voxel.y as f32 + 0.5 - origin.y,
                voxel.z as f32 + 0.5 - origin.z,
            );
            if diff.length_squared() > max_dist_sq {
                return None;
            }

            let id = self.get_voxel(voxel);
            if id != 0 {
                if id >= MESH_VOXEL_BASE {
                    // Mesh voxel: test ray against mesh triangles
                    if let Some(catalog) = mesh_catalog {
                        let voxel_min = Vec3::new(voxel.x as f32, voxel.y as f32, voxel.z as f32);
                        // Compute t_entry: the t value where the ray enters this voxel
                        let t_entry = {
                            let t_min = (voxel_min - origin) / dir;
                            let t_max_box = (voxel_min + Vec3::ONE - origin) / dir;
                            let t_near = t_min.min(t_max_box);
                            t_near.x.max(t_near.y).max(t_near.z).max(0.0)
                        };
                        let local_origin = origin + dir * t_entry - voxel_min;
                        let mesh_type = id - MESH_VOXEL_BASE;
                        if let Some((_t, tri_normal)) = catalog.raycast(mesh_type, local_origin, dir) {
                            let (chunk_pos, local_pos) = Self::world_to_chunk(voxel);
                            return Some(RaycastHit {
                                chunk_pos,
                                local_pos,
                                normal: IVec3::new(
                                    tri_normal.x.round() as i32,
                                    tri_normal.y.round() as i32,
                                    tri_normal.z.round() as i32,
                                ),
                                voxel_id: id,
                            });
                        }
                        // No triangle hit — ray passes through, continue DDA
                    }
                    // No catalog — treat as passthrough, continue DDA
                } else if id != WATER_VOXEL_ID {
                    let (chunk_pos, local_pos) = Self::world_to_chunk(voxel);
                    return Some(RaycastHit {
                        chunk_pos,
                        local_pos,
                        normal,
                        voxel_id: id,
                    });
                }
            }
        }
    }

    pub fn pack_gpu_data(&self) -> (Vec<u8>, Vec<GpuChunkInfo>, Vec<GpuBvhNode>) {
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

        let bvh_nodes = bvh::build_bvh(&chunk_infos);

        (voxel_data, chunk_infos, bvh_nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::TerrainGenerator;

    fn test_chunk() -> Chunk {
        TerrainGenerator::new(0).generate_chunk(IVec3::ZERO)
    }

    #[test]
    fn insert_and_get() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, test_chunk());
        assert!(world.get(&IVec3::ZERO).is_some());
        assert!(world.get(&IVec3::X).is_none());
    }

    #[test]
    fn remove_returns_chunk_and_clears() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, test_chunk());
        let removed = world.remove(&IVec3::ZERO);
        assert!(removed.is_some());
        assert!(world.get(&IVec3::ZERO).is_none());
    }

    #[test]
    fn iter_yields_all() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, test_chunk());
        world.insert(IVec3::X, test_chunk());
        world.insert(IVec3::Y, test_chunk());
        assert_eq!(world.iter().count(), 3);
        assert_eq!(world.chunk_count(), 3);
    }

    #[test]
    fn pack_single_chunk() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, test_chunk());
        let (voxel_data, infos, bvh_nodes) = world.pack_gpu_data();
        assert_eq!(voxel_data.len(), 4096);
        assert_eq!(infos.len(), 1);
        assert_eq!(bvh_nodes.len(), 1);
        assert_eq!(infos[0].data_offset, 0);
        assert_eq!(infos[0].world_offset, Vec3::ZERO);
    }

    #[test]
    fn pack_two_chunks() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, test_chunk());
        world.insert(IVec3::X, test_chunk());
        let (voxel_data, infos, bvh_nodes) = world.pack_gpu_data();
        assert_eq!(voxel_data.len(), 8192);
        assert_eq!(infos.len(), 2);
        assert_eq!(bvh_nodes.len(), 3);
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
        world.insert(IVec3::ZERO, test_chunk());
        assert!(world.is_dirty());
    }

    #[test]
    fn clear_dirty_resets() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, test_chunk());
        world.clear_dirty();
        assert!(!world.is_dirty());
    }

    #[test]
    fn remove_sets_dirty() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, test_chunk());
        world.clear_dirty();
        world.remove(&IVec3::ZERO);
        assert!(world.is_dirty());
    }

    #[test]
    fn world_to_chunk_positive() {
        let (chunk, local) = World::world_to_chunk(IVec3::new(17, 5, 33));
        assert_eq!(chunk, IVec3::new(1, 0, 2));
        assert_eq!(local, [1, 5, 1]);
    }

    #[test]
    fn world_to_chunk_negative() {
        let (chunk, local) = World::world_to_chunk(IVec3::new(-1, 0, 0));
        assert_eq!(chunk, IVec3::new(-1, 0, 0));
        assert_eq!(local, [15, 0, 0]);
    }

    #[test]
    fn set_voxel_loaded_chunk() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, Chunk::new());
        world.clear_dirty();
        assert!(world.set_voxel(IVec3::new(5, 5, 5), 3));
        assert!(world.is_dirty());
        assert_eq!(world.get_voxel(IVec3::new(5, 5, 5)), 3);
    }

    #[test]
    fn set_voxel_unloaded_returns_false() {
        let mut world = World::new();
        assert!(!world.set_voxel(IVec3::new(5, 5, 5), 3));
    }

    #[test]
    fn get_voxel_unloaded_returns_zero() {
        let world = World::new();
        assert_eq!(world.get_voxel(IVec3::new(100, 100, 100)), 0);
    }

    #[test]
    fn raycast_hit_known_voxel() {
        let mut world = World::new();
        let mut chunk = Chunk::new();
        chunk.set(8, 8, 8, 2);
        world.insert(IVec3::ZERO, chunk);

        let hit = world
            .raycast(Vec3::new(8.5, 8.5, 0.5), Vec3::new(0.0, 0.0, 1.0), 50.0, None)
            .expect("should hit");
        assert_eq!(hit.chunk_pos, IVec3::ZERO);
        assert_eq!(hit.local_pos, [8, 8, 8]);
        assert_eq!(hit.voxel_id, 2);
    }

    #[test]
    fn raycast_correct_normal() {
        let mut world = World::new();
        let mut chunk = Chunk::new();
        chunk.set(8, 8, 8, 1);
        world.insert(IVec3::ZERO, chunk);

        // Approaching from -Z direction
        let hit = world
            .raycast(Vec3::new(8.5, 8.5, 0.5), Vec3::new(0.0, 0.0, 1.0), 50.0, None)
            .expect("should hit");
        assert_eq!(hit.normal, IVec3::new(0, 0, -1));
    }

    #[test]
    fn raycast_miss_returns_none() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, Chunk::new());
        assert!(world
            .raycast(Vec3::new(8.5, 8.5, 0.5), Vec3::new(0.0, 0.0, 1.0), 50.0, None)
            .is_none());
    }

    #[test]
    fn raycast_inside_solid_returns_immediately() {
        let mut world = World::new();
        let mut chunk = Chunk::new();
        chunk.set(5, 5, 5, 4);
        world.insert(IVec3::ZERO, chunk);

        let hit = world
            .raycast(Vec3::new(5.5, 5.5, 5.5), Vec3::new(1.0, 0.0, 0.0), 50.0, None)
            .expect("should hit");
        assert_eq!(hit.voxel_id, 4);
        assert_eq!(hit.normal, IVec3::ZERO);
    }

    #[test]
    fn set_voxel_adds_to_modified() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, Chunk::new());
        world.set_voxel(IVec3::new(5, 5, 5), 3);
        assert!(world.is_chunk_modified(&IVec3::ZERO));
    }

    #[test]
    fn take_modified_returns_and_clears() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, Chunk::new());
        world.insert(IVec3::X, Chunk::new());
        world.set_voxel(IVec3::new(5, 5, 5), 1);
        world.set_voxel(IVec3::new(16, 0, 0), 2);
        let modified = world.take_modified();
        assert!(modified.contains(&IVec3::ZERO));
        assert!(modified.contains(&IVec3::X));
        assert!(!world.is_chunk_modified(&IVec3::ZERO));
        assert!(!world.is_chunk_modified(&IVec3::X));
    }

    #[test]
    fn remove_clears_from_modified() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, Chunk::new());
        world.set_voxel(IVec3::new(5, 5, 5), 3);
        assert!(world.is_chunk_modified(&IVec3::ZERO));
        world.remove(&IVec3::ZERO);
        assert!(!world.is_chunk_modified(&IVec3::ZERO));
    }

    #[test]
    fn is_chunk_loaded_returns_true_for_inserted() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, Chunk::new());
        assert!(world.is_chunk_loaded(&IVec3::ZERO));
    }

    #[test]
    fn is_chunk_loaded_returns_false_for_missing() {
        let world = World::new();
        assert!(!world.is_chunk_loaded(&IVec3::ZERO));
    }

    #[test]
    fn raycast_mesh_continues_past_miss() {
        use crate::mesh_catalog::{MeshCatalog, MESH_TORCH};

        let mut world = World::new();
        let mut chunk = Chunk::new();
        // Place a mesh torch at (8, 8, 8)
        chunk.set(8, 8, 8, MESH_TORCH);
        // Place a solid block behind it at (8, 8, 10)
        chunk.set(8, 8, 10, 1);
        world.insert(IVec3::ZERO, chunk);

        let catalog = MeshCatalog::build();
        // Cast a ray through the corner of the torch voxel (should miss mesh triangles)
        // and hit the solid block behind it
        let hit = world
            .raycast(
                Vec3::new(8.05, 8.5, 0.5),
                Vec3::new(0.0, 0.0, 1.0),
                50.0,
                Some(&catalog),
            );
        assert!(hit.is_some(), "should hit the solid block behind the torch");
        let hit = match hit {
            Some(h) => h,
            None => panic!("expected hit"),
        };
        assert_eq!(hit.local_pos, [8, 8, 10]);
        assert_eq!(hit.voxel_id, 1);
    }

    #[test]
    fn water_not_solid() {
        let mut world = World::new();
        let mut chunk = Chunk::new();
        chunk.set(8, 8, 8, WATER_VOXEL_ID);
        world.insert(IVec3::ZERO, chunk);
        assert!(!world.is_solid_voxel(IVec3::new(8, 8, 8)));
    }

    #[test]
    fn raycast_skips_water() {
        let mut world = World::new();
        let mut chunk = Chunk::new();
        chunk.set(8, 8, 8, WATER_VOXEL_ID);
        chunk.set(8, 8, 10, 1);
        world.insert(IVec3::ZERO, chunk);

        let hit = world
            .raycast(Vec3::new(8.5, 8.5, 0.5), Vec3::new(0.0, 0.0, 1.0), 50.0, None)
            .expect("should hit stone behind water");
        assert_eq!(hit.local_pos, [8, 8, 10]);
        assert_eq!(hit.voxel_id, 1);
    }

    #[test]
    fn raycast_across_chunk_boundary() {
        let mut world = World::new();
        world.insert(IVec3::ZERO, Chunk::new());
        let mut chunk1 = Chunk::new();
        chunk1.set(0, 8, 8, 3);
        world.insert(IVec3::X, chunk1);

        let hit = world
            .raycast(Vec3::new(14.5, 8.5, 8.5), Vec3::new(1.0, 0.0, 0.0), 50.0, None)
            .expect("should hit");
        assert_eq!(hit.chunk_pos, IVec3::X);
        assert_eq!(hit.local_pos, [0, 8, 8]);
        assert_eq!(hit.voxel_id, 3);
    }
}

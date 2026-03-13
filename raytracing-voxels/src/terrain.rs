use glam::IVec3;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::chunk::Chunk;

const SIZE: usize = 16;
const SIZE_I32: i32 = SIZE as i32;
const BASE_HEIGHT: f64 = 32.0;
const AMPLITUDE: f64 = 24.0;
const DIRT_LAYERS: i32 = 3;

/// Voxel IDs for tree blocks.
const WOOD: u8 = 5;
const LEAVES: u8 = 6;
const GRASS: u8 = 3;

/// How far outside the chunk XZ range to scan for tree candidates.
const TREE_SCAN_MARGIN: i32 = 10;

/// Tree placement threshold. A tree is placed when `tree_hash(...) % TREE_DENSITY_MOD == 0`.
/// With a 1-in-200 chance per column, roughly 1 tree per ~14x14 area on average.
const TREE_DENSITY_MOD: u32 = 200;

/// Deterministic hash for tree placement decisions, modeled after `pixel_hash`.
fn tree_hash(x: i32, z: i32, seed: u32) -> u32 {
    let mut h = (x as u32)
        .wrapping_mul(374761393)
        .wrapping_add((z as u32).wrapping_mul(668265263))
        .wrapping_add(seed.wrapping_mul(1274126177));
    h = (h ^ (h >> 13)).wrapping_mul(1103515245);
    h = (h ^ (h >> 16)).wrapping_mul(2654435769);
    h ^ (h >> 13)
}

/// Whether a tree has a 1x1 or 2x2 trunk footprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TreeKind {
    /// 1x1 base, 6-10 units tall (60% of trees)
    Small,
    /// 2x2 base, 8-14 units tall (40% of trees)
    Large,
}

/// Parameters for a single tree derived deterministically from its position.
struct TreeParams {
    kind: TreeKind,
    trunk_height: i32,
    canopy_radius: i32,
}

impl TreeParams {
    fn from_position(wx: i32, wz: i32, seed: u32) -> Self {
        let h = tree_hash(wx.wrapping_add(7919), wz.wrapping_add(6271), seed);
        // 60% small, 40% large
        let kind = if (h % 10) < 6 {
            TreeKind::Small
        } else {
            TreeKind::Large
        };
        match kind {
            TreeKind::Small => {
                let trunk_height = 6 + ((h >> 4) % 5) as i32; // 6..=10
                let canopy_radius = 2 + ((h >> 8) % 2) as i32; // 2 or 3
                Self { kind, trunk_height, canopy_radius }
            }
            TreeKind::Large => {
                let trunk_height = 8 + ((h >> 4) % 7) as i32; // 8..=14
                let canopy_radius = 3 + ((h >> 8) % 2) as i32; // 3 or 4
                Self { kind, trunk_height, canopy_radius }
            }
        }
    }
}

pub struct TerrainGenerator {
    height_noise: Fbm<Perlin>,
    seed: u32,
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        let height_noise = Fbm::<Perlin>::new(seed)
            .set_octaves(4)
            .set_frequency(0.02)
            .set_lacunarity(2.0)
            .set_persistence(0.5);
        Self { height_noise, seed }
    }

    /// Compute the surface height at world coordinates (wx, wz).
    pub fn surface_height(&self, wx: i32, wz: i32) -> i32 {
        let noise_val = self.height_noise.get([wx as f64, wz as f64]);
        (BASE_HEIGHT + noise_val * AMPLITUDE) as i32
    }

    /// Returns true if a tree should be placed at world column (wx, wz).
    fn has_tree_at(&self, wx: i32, wz: i32) -> bool {
        let h = tree_hash(wx, wz, self.seed);
        if !h.is_multiple_of(TREE_DENSITY_MOD) {
            return false;
        }
        // Only place trees on grass surfaces
        let surface = self.surface_height(wx, wz);
        let surface_voxel = self.terrain_voxel_at(wx, surface, wz);
        surface_voxel == GRASS
    }

    /// Return the base terrain voxel at a world position (before trees).
    fn terrain_voxel_at(&self, _wx: i32, wy: i32, _wz: i32) -> u8 {
        let height = self.surface_height(_wx, _wz);
        if wy > height {
            0
        } else if wy == height {
            GRASS
        } else if wy > height - DIRT_LAYERS {
            2 // dirt
        } else {
            1 // stone
        }
    }

    pub fn generate_chunk(&self, chunk_pos: IVec3) -> Chunk {
        let mut chunk = Chunk::new();
        let world_x = chunk_pos.x * SIZE_I32;
        let world_y = chunk_pos.y * SIZE_I32;
        let world_z = chunk_pos.z * SIZE_I32;

        // Base terrain pass
        for lx in 0..SIZE {
            for lz in 0..SIZE {
                let wx = world_x + lx as i32;
                let wz = world_z + lz as i32;
                let height = self.surface_height(wx, wz);

                for ly in 0..SIZE {
                    let wy = world_y + ly as i32;
                    let voxel = if wy > height {
                        0 // air
                    } else if wy == height {
                        3 // grass
                    } else if wy > height - DIRT_LAYERS {
                        2 // dirt
                    } else {
                        1 // stone
                    };
                    if voxel != 0 {
                        chunk.set(lx, ly, lz, voxel);
                    }
                }
            }
        }

        // Tree placement pass: scan candidate positions including margin
        let scan_x_min = world_x - TREE_SCAN_MARGIN;
        let scan_x_max = world_x + SIZE_I32 + TREE_SCAN_MARGIN;
        let scan_z_min = world_z - TREE_SCAN_MARGIN;
        let scan_z_max = world_z + SIZE_I32 + TREE_SCAN_MARGIN;

        for tx in scan_x_min..scan_x_max {
            for tz in scan_z_min..scan_z_max {
                if !self.has_tree_at(tx, tz) {
                    continue;
                }

                let surface_y = self.surface_height(tx, tz);
                let params = TreeParams::from_position(tx, tz, self.seed);

                // Trunk footprint offsets: 1x1 for small, 2x2 for large
                let trunk_offsets: &[(i32, i32)] = match params.kind {
                    TreeKind::Small => &[(0, 0)],
                    TreeKind::Large => &[(0, 0), (1, 0), (0, 1), (1, 1)],
                };

                // For 2x2 trees, fill ground under corners that are lower than
                // the tree's base so no air gaps appear under the trunk.
                if params.kind == TreeKind::Large {
                    for &(ox, oz) in trunk_offsets {
                        let wx = tx + ox;
                        let wz = tz + oz;
                        let corner_surface = self.surface_height(wx, wz);
                        if corner_surface < surface_y {
                            for wy in (corner_surface + 1)..=surface_y {
                                let lx = wx - world_x;
                                let ly = wy - world_y;
                                let lz = wz - world_z;
                                if (0..SIZE_I32).contains(&lx) && (0..SIZE_I32).contains(&ly) && (0..SIZE_I32).contains(&lz) {
                                    if chunk.get(lx as usize, ly as usize, lz as usize) == 0 {
                                        chunk.set(lx as usize, ly as usize, lz as usize, GRASS);
                                    }
                                }
                            }
                        }
                    }
                }

                // Place trunk
                for dy in 1..=params.trunk_height {
                    let wy = surface_y + dy;
                    let ly = wy - world_y;
                    for &(ox, oz) in trunk_offsets {
                        let lx = (tx + ox) - world_x;
                        let lz = (tz + oz) - world_z;
                        if (0..SIZE_I32).contains(&lx) && (0..SIZE_I32).contains(&ly) && (0..SIZE_I32).contains(&lz) {
                            if chunk.get(lx as usize, ly as usize, lz as usize) == 0 {
                                chunk.set(lx as usize, ly as usize, lz as usize, WOOD);
                            }
                        }
                    }
                }

                // Place canopy raised so some trunk segments are visible below leaves
                let r = params.canopy_radius;
                let r_sq = r * r;
                // Center canopy near the top of the trunk, leaving a few segments visible below
                let canopy_center_y = surface_y + params.trunk_height - r + 1;

                // For large trees, offset canopy center to middle of 2x2 footprint
                let canopy_cx = match params.kind {
                    TreeKind::Small => tx as f32,
                    TreeKind::Large => tx as f32 + 0.5,
                };
                let canopy_cz = match params.kind {
                    TreeKind::Small => tz as f32,
                    TreeKind::Large => tz as f32 + 0.5,
                };

                for dx in -(r + 1)..=(r + 1) {
                    for dy in -r..=r {
                        for dz in -(r + 1)..=(r + 1) {
                            let fx = tx as f32 + dx as f32 - canopy_cx;
                            let fz = tz as f32 + dz as f32 - canopy_cz;
                            let dist_sq = fx * fx + (dy * dy) as f32 + fz * fz;
                            if dist_sq > (r * r) as f32 {
                                continue;
                            }

                            let bx = tx + dx;
                            let by = canopy_center_y + dy;
                            let bz = tz + dz;

                            let lx = bx - world_x;
                            let ly = by - world_y;
                            let lz = bz - world_z;

                            if (0..SIZE_I32).contains(&lx) && (0..SIZE_I32).contains(&ly) && (0..SIZE_I32).contains(&lz) {
                                // Only place leaves in air (don't overwrite terrain or wood)
                                if chunk.get(lx as usize, ly as usize, lz as usize) == 0 {
                                    chunk.set(lx as usize, ly as usize, lz as usize, LEAVES);
                                }
                            }
                        }
                    }
                }

                // Cap trunk columns: place leaves above any exposed wood so no
                // wood is open to the sky. Walk upward from trunk top until we
                // hit existing leaves or place one leaf cap.
                for &(ox, oz) in trunk_offsets {
                    let wx = tx + ox;
                    let wz = tz + oz;
                    let lx = wx - world_x;
                    let lz = wz - world_z;
                    if !(0..SIZE_I32).contains(&lx) || !(0..SIZE_I32).contains(&lz) {
                        continue;
                    }
                    // Start from the block above the trunk top
                    let mut wy = surface_y + params.trunk_height + 1;
                    loop {
                        let ly = wy - world_y;
                        if !(0..SIZE_I32).contains(&ly) {
                            break;
                        }
                        let v = chunk.get(lx as usize, ly as usize, lz as usize);
                        if v == LEAVES {
                            // Already covered by canopy
                            break;
                        }
                        if v == 0 {
                            chunk.set(lx as usize, ly as usize, lz as usize, LEAVES);
                            break;
                        }
                        // v is WOOD or terrain — keep going up
                        wy += 1;
                    }
                }
            }
        }

        chunk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_chunk_not_empty() {
        let generator = TerrainGenerator::new(0);
        let chunk = generator.generate_chunk(IVec3::ZERO);
        assert!(chunk.data().iter().any(|&v| v != 0));
    }

    #[test]
    fn terrain_chunk_above_surface_is_air() {
        let generator = TerrainGenerator::new(0);
        let chunk = generator.generate_chunk(IVec3::new(0, 100, 0));
        assert!(chunk.data().iter().all(|&v| v == 0));
    }

    #[test]
    fn terrain_consistent_across_calls() {
        let generator = TerrainGenerator::new(42);
        let a = generator.generate_chunk(IVec3::new(3, 0, -2));
        let b = generator.generate_chunk(IVec3::new(3, 0, -2));
        assert_eq!(a.data(), b.data());
    }

    #[test]
    fn terrain_chunks_share_border_heights() {
        let generator = TerrainGenerator::new(42);
        // Two vertically adjacent chunks should produce consistent columns:
        // if the surface height for a column falls at world y=15, the lower
        // chunk should have grass at local y=15, and the upper chunk should
        // have air at local y=0 for that same column.
        let lower = generator.generate_chunk(IVec3::new(0, 0, 0));
        let upper = generator.generate_chunk(IVec3::new(0, 1, 0));

        for x in 0..16 {
            for z in 0..16 {
                // If the top of the lower chunk (y=15) is air, then
                // the bottom of the upper chunk (y=0) must also be air
                // (surface is below both).
                // If the bottom of the upper chunk (y=0) is solid,
                // then the top of the lower chunk (y=15) must also be solid
                // (surface is above both).
                let lower_top = lower.get(x, 15, z);
                let upper_bottom = upper.get(x, 0, z);
                // Skip tree voxels — trees can place leaves/wood in air above terrain
                if lower_top == 0 {
                    // upper_bottom can be 0 (air) or a tree voxel placed by cross-chunk scan
                    if upper_bottom != 0 && upper_bottom != WOOD && upper_bottom != LEAVES {
                        panic!("column ({x},{z}): lower top is air but upper bottom is terrain {upper_bottom}");
                    }
                }
                if upper_bottom != 0 && upper_bottom != WOOD && upper_bottom != LEAVES {
                    assert_ne!(lower_top, 0,
                        "column ({x},{z}): upper bottom is solid terrain but lower top is air");
                }
            }
        }
    }

    #[test]
    fn terrain_with_trees_has_wood() {
        let generator = TerrainGenerator::new(42);
        let mut found_wood = false;
        // Search across several chunks near terrain surface
        for cy in 0..4 {
            for cx in -2..3 {
                for cz in -2..3 {
                    let chunk = generator.generate_chunk(IVec3::new(cx, cy, cz));
                    if chunk.data().iter().any(|&v| v == WOOD) {
                        found_wood = true;
                    }
                }
            }
        }
        assert!(found_wood, "expected to find at least one wood voxel in generated terrain");
    }

    #[test]
    fn terrain_with_trees_has_leaves() {
        let generator = TerrainGenerator::new(42);
        let mut found_leaves = false;
        for cy in 0..4 {
            for cx in -2..3 {
                for cz in -2..3 {
                    let chunk = generator.generate_chunk(IVec3::new(cx, cy, cz));
                    if chunk.data().iter().any(|&v| v == LEAVES) {
                        found_leaves = true;
                    }
                }
            }
        }
        assert!(found_leaves, "expected to find at least one leaves voxel in generated terrain");
    }

    #[test]
    fn tree_placement_deterministic() {
        let gen1 = TerrainGenerator::new(99);
        let gen2 = TerrainGenerator::new(99);
        for cy in 0..3 {
            let pos = IVec3::new(1, cy, -1);
            let a = gen1.generate_chunk(pos);
            let b = gen2.generate_chunk(pos);
            assert_eq!(a.data(), b.data(), "chunks at {pos} differ between generators with same seed");
        }
    }

    #[test]
    fn tree_placement_cross_chunk_consistent() {
        let generator = TerrainGenerator::new(42);
        // Find a tree trunk that spans a chunk boundary by looking for wood
        // at y=15 in a lower chunk and checking the upper chunk has wood at y=0.
        let mut found_spanning_trunk = false;

        'outer: for cx in -3..4 {
            for cz in -3..4 {
                for cy in 0..4 {
                    let lower = generator.generate_chunk(IVec3::new(cx, cy, cz));
                    let upper = generator.generate_chunk(IVec3::new(cx, cy + 1, cz));

                    for x in 0..SIZE {
                        for z in 0..SIZE {
                            if lower.get(x, 15, z) == WOOD && upper.get(x, 0, z) == WOOD {
                                found_spanning_trunk = true;
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }

        assert!(found_spanning_trunk,
            "expected to find at least one trunk spanning a vertical chunk boundary");
    }

    #[test]
    fn terrain_deterministic_regardless_of_order() {
        let generator = TerrainGenerator::new(42);
        let positions = vec![
            IVec3::new(0, 0, 0),
            IVec3::new(1, 0, 0),
            IVec3::new(0, 0, 1),
        ];

        let forward: Vec<_> = positions.iter().map(|&p| generator.generate_chunk(p)).collect();
        let reverse: Vec<_> = positions.iter().rev().map(|&p| generator.generate_chunk(p)).collect();

        for (i, pos) in positions.iter().enumerate() {
            let rev_i = positions.len() - 1 - i;
            assert_eq!(
                forward[i].data(),
                reverse[rev_i].data(),
                "chunk at {pos} differs when generated in different order"
            );
        }
    }

    #[test]
    fn trees_only_on_surface() {
        let generator = TerrainGenerator::new(42);
        for cy in 0..4 {
            for cx in -2..3 {
                for cz in -2..3 {
                    let chunk_pos = IVec3::new(cx, cy, cz);
                    let chunk = generator.generate_chunk(chunk_pos);
                    let world_y = chunk_pos.y * SIZE_I32;
                    let world_x = chunk_pos.x * SIZE_I32;
                    let world_z = chunk_pos.z * SIZE_I32;

                    for lx in 0..SIZE {
                        for ly in 0..SIZE {
                            for lz in 0..SIZE {
                                if chunk.get(lx, ly, lz) == WOOD {
                                    let wx = world_x + lx as i32;
                                    let wy = world_y + ly as i32;
                                    let wz = world_z + lz as i32;
                                    let surface = generator.surface_height(wx, wz);
                                    assert!(wy > surface,
                                        "wood at world ({wx},{wy},{wz}) is not above surface height {surface}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

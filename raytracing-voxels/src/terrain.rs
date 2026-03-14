use glam::IVec3;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};
use tracing::trace;

use crate::chunk::Chunk;

const SIZE: usize = 16;
const SIZE_I32: i32 = SIZE as i32;
const BASE_HEIGHT: f64 = 40.0;
const MIN_AMPLITUDE: f64 = 16.0;
const MAX_AMPLITUDE: f64 = 160.0;
const DIRT_LAYERS: i32 = 3;

const STONE: u8 = 1;
const WOOD: u8 = 5;
const LEAVES: u8 = 6;
const GRASS: u8 = 3;
pub const WATER: u8 = 7;
pub const WATER_LEVEL: i32 = 36;

/// Height above which mountain peaks are bare stone instead of grass.
const BARE_PEAK_HEIGHT: f64 = BASE_HEIGHT + MAX_AMPLITUDE * 0.6;

/// Cave spaghetti tunnel threshold: lower = thinner tunnels.
const WORM_THRESHOLD: f64 = 0.016;
/// Cave cavern threshold: higher = fewer/smaller caverns.
const CAVERN_THRESHOLD: f64 = 0.72;

/// Fraction of 8x8 regions that allow surface cave entrances (1 in N).
const ENTRANCE_ZONE_DIVISOR: u32 = 3;
/// Depth at which cave thresholds reach full strength.
const CAVE_SURFACE_RAMP_DEPTH: f64 = 10.0;
/// Near-surface worm threshold multiplier (scales down tunnel size near surface).
const CAVE_SURFACE_WORM_SCALE: f64 = 1.0;

/// How far outside the chunk XZ range to scan for tree candidates.
const TREE_SCAN_MARGIN: i32 = 10;

/// Tree placement threshold for flat areas.
const TREE_DENSITY_MOD: u32 = 150;

/// No trees above this height (near mountain peaks).
const TREE_HEIGHT_LIMIT: f64 = BASE_HEIGHT + MAX_AMPLITUDE * 0.7;

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

#[derive(Clone)]
pub struct TerrainGenerator {
    height_noise: Fbm<Perlin>,
    mountain_noise: Fbm<Perlin>,
    cave_worm_noise_a: Fbm<Perlin>,
    cave_worm_noise_b: Fbm<Perlin>,
    cave_cavern_noise: Fbm<Perlin>,
    seed: u32,
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        let height_noise = Fbm::<Perlin>::new(seed)
            .set_octaves(4)
            .set_frequency(0.01)
            .set_lacunarity(2.0)
            .set_persistence(0.5);
        let mountain_noise = Fbm::<Perlin>::new(seed.wrapping_add(1))
            .set_octaves(3)
            .set_frequency(0.005)
            .set_lacunarity(2.0)
            .set_persistence(0.5);
        let cave_worm_noise_a = Fbm::<Perlin>::new(seed.wrapping_add(10))
            .set_octaves(3)
            .set_frequency(0.014)
            .set_lacunarity(2.0)
            .set_persistence(0.5);
        let cave_worm_noise_b = Fbm::<Perlin>::new(seed.wrapping_add(11))
            .set_octaves(3)
            .set_frequency(0.014)
            .set_lacunarity(2.0)
            .set_persistence(0.5);
        let cave_cavern_noise = Fbm::<Perlin>::new(seed.wrapping_add(12))
            .set_octaves(2)
            .set_frequency(0.015)
            .set_lacunarity(2.0)
            .set_persistence(0.5);
        Self {
            height_noise, mountain_noise,
            cave_worm_noise_a, cave_worm_noise_b, cave_cavern_noise,
            seed,
        }
    }

    /// Compute the surface height at world coordinates (wx, wz).
    pub fn surface_height(&self, wx: i32, wz: i32) -> i32 {
        let height_val = self.height_noise.get([wx as f64, wz as f64]);
        let mountain_raw = self.mountain_noise.get([wx as f64, wz as f64]);
        let mountain_factor = ((mountain_raw + 0.3) * 1.5).clamp(0.0, 1.0);
        let amplitude = MIN_AMPLITUDE + (MAX_AMPLITUDE - MIN_AMPLITUDE) * mountain_factor;
        (BASE_HEIGHT + height_val * amplitude) as i32
    }

    /// Returns true if the voxel at (wx, wy, wz) should be carved out as a cave.
    pub fn is_cave_at(&self, wx: i32, wy: i32, wz: i32) -> bool {
        let surface = self.surface_height(wx, wz);
        let depth = surface - wy;

        // Near-surface: only allow caves in entrance zones
        if depth < 3 {
            let entrance_hash = tree_hash(wx.div_euclid(8), wz.div_euclid(8), self.seed.wrapping_add(20));
            if !entrance_hash.is_multiple_of(ENTRANCE_ZONE_DIVISOR) {
                return false;
            }
        }

        let coords = [wx as f64, wy as f64, wz as f64];

        // Spaghetti caves: intersection of two noise channels near zero
        let a = self.cave_worm_noise_a.get(coords);
        let b = self.cave_worm_noise_b.get(coords);
        let worm_val = a * a + b * b;

        // Caverns: large open spaces
        let cavern_val = self.cave_cavern_noise.get(coords);

        // Depth bias: make caves rarer near the surface
        let (worm_thresh, cavern_thresh) = if depth < 10 {
            let factor = (depth as f64).max(0.0) / CAVE_SURFACE_RAMP_DEPTH;
            (WORM_THRESHOLD * factor * CAVE_SURFACE_WORM_SCALE, CAVERN_THRESHOLD + (1.0 - factor) * 0.3)
        } else {
            (WORM_THRESHOLD, CAVERN_THRESHOLD)
        };

        worm_val < worm_thresh || cavern_val > cavern_thresh
    }

    /// Approximate slope at (wx, wz) as max absolute height difference to cardinal neighbors.
    fn slope_at(&self, wx: i32, wz: i32) -> i32 {
        let center = self.surface_height(wx, wz);
        let n = self.surface_height(wx, wz - 1);
        let s = self.surface_height(wx, wz + 1);
        let e = self.surface_height(wx + 1, wz);
        let w = self.surface_height(wx - 1, wz);
        (center - n).abs()
            .max((center - s).abs())
            .max((center - e).abs())
            .max((center - w).abs())
    }

    /// Returns true if a tree should be placed at world column (wx, wz).
    fn has_tree_at(&self, wx: i32, wz: i32) -> bool {
        let h = tree_hash(wx, wz, self.seed);
        if !h.is_multiple_of(TREE_DENSITY_MOD) {
            return false;
        }
        let surface = self.surface_height(wx, wz);
        if surface < WATER_LEVEL {
            return false;
        }
        // No trees near mountain peaks
        if surface as f64 > TREE_HEIGHT_LIMIT {
            return false;
        }
        // Slope-based thinning
        let slope = self.slope_at(wx, wz);
        if slope > 3 {
            return false;
        }
        if slope >= 1 {
            let slope_hash = tree_hash(wx, wz, self.seed.wrapping_add(2));
            if !slope_hash.is_multiple_of(slope as u32 * 3) {
                return false;
            }
        }
        // Only place trees on grass surfaces
        let surface_voxel = self.terrain_voxel_at(wx, surface, wz);
        surface_voxel == GRASS
    }

    /// Return the base terrain voxel at a world position (before trees).
    fn terrain_voxel_at(&self, wx: i32, wy: i32, wz: i32) -> u8 {
        let height = self.surface_height(wx, wz);
        Self::terrain_voxel_at_with_height(wy, height)
    }

    /// Same as `terrain_voxel_at` but uses a precomputed surface height.
    fn terrain_voxel_at_with_height(wy: i32, height: i32) -> u8 {
        if wy > height {
            0
        } else if wy == height {
            if height as f64 > BARE_PEAK_HEIGHT {
                STONE
            } else {
                GRASS
            }
        } else if wy > height - DIRT_LAYERS {
            2 // dirt
        } else {
            STONE
        }
    }

    /// Same as `is_cave_at` but uses a precomputed surface height.
    fn is_cave_at_with_surface(&self, wx: i32, wy: i32, wz: i32, surface: i32) -> bool {
        let depth = surface - wy;

        // Near-surface: only allow caves in entrance zones
        if depth < 3 {
            let entrance_hash = tree_hash(wx.div_euclid(8), wz.div_euclid(8), self.seed.wrapping_add(20));
            if !entrance_hash.is_multiple_of(ENTRANCE_ZONE_DIVISOR) {
                return false;
            }
        }

        let coords = [wx as f64, wy as f64, wz as f64];

        let a = self.cave_worm_noise_a.get(coords);
        let b = self.cave_worm_noise_b.get(coords);
        let worm_val = a * a + b * b;

        let cavern_val = self.cave_cavern_noise.get(coords);

        let (worm_thresh, cavern_thresh) = if depth < 10 {
            let factor = (depth as f64).max(0.0) / CAVE_SURFACE_RAMP_DEPTH;
            (WORM_THRESHOLD * factor * CAVE_SURFACE_WORM_SCALE, CAVERN_THRESHOLD + (1.0 - factor) * 0.3)
        } else {
            (WORM_THRESHOLD, CAVERN_THRESHOLD)
        };

        worm_val < worm_thresh || cavern_val > cavern_thresh
    }

    pub fn generate_chunk(&self, chunk_pos: IVec3) -> Chunk {
        trace!(?chunk_pos, "generating terrain chunk");
        let mut chunk = Chunk::new();
        let world_x = chunk_pos.x * SIZE_I32;
        let world_y = chunk_pos.y * SIZE_I32;
        let world_z = chunk_pos.z * SIZE_I32;

        // Precompute surface heights per column (avoids redundant noise evals)
        let mut heights = [[0i32; SIZE]; SIZE];
        for (lx, col) in heights.iter_mut().enumerate() {
            for (lz, h) in col.iter_mut().enumerate() {
                *h = self.surface_height(world_x + lx as i32, world_z + lz as i32);
            }
        }

        // Combined terrain + cave pass using cached heights
        for (lx, col) in heights.iter().enumerate() {
            for (lz, &height) in col.iter().enumerate() {
                let wx = world_x + lx as i32;
                let wz = world_z + lz as i32;

                for ly in 0..SIZE {
                    let wy = world_y + ly as i32;
                    let voxel = Self::terrain_voxel_at_with_height(wy, height);
                    if voxel != 0 && !self.is_cave_at_with_surface(wx, wy, wz, height) {
                        chunk.set(lx, ly, lz, voxel);
                    }
                }
            }
        }

        // Cleanup pass: erode thin floating features near cave entrances.
        // Iteratively remove any terrain block (not tree) near the surface that
        // has fewer than 2 solid cardinal neighbours.  This collapses 1-wide
        // lines and isolated blocks that remain after cave carving.
        loop {
            let mut changed = false;
            for lx in 0..SIZE {
                for lz in 0..SIZE {
                    let wx = world_x + lx as i32;
                    let wz = world_z + lz as i32;
                    let surface = heights[lx][lz];

                    for ly in 0..SIZE {
                        let v = chunk.get(lx, ly, lz);
                        // Only erode terrain blocks (stone, dirt, grass), not trees
                        if v == 0 || v == WOOD || v == LEAVES {
                            continue;
                        }
                        let wy = world_y + ly as i32;
                        // Only clean up near the surface (within ramp depth)
                        let depth = surface - wy;
                        if depth > CAVE_SURFACE_RAMP_DEPTH as i32 + 2 {
                            continue;
                        }

                        // Count solid cardinal neighbours (6 directions).
                        // Out-of-chunk neighbours are resolved via world-space
                        // terrain + cave queries so chunk edges are handled.
                        let mut solid = 0u8;
                        for &(dx, dy, dz) in &[
                            (-1i32, 0i32, 0i32), (1, 0, 0),
                            (0, -1, 0), (0, 1, 0),
                            (0, 0, -1), (0, 0, 1),
                        ] {
                            let nx = lx as i32 + dx;
                            let ny = ly as i32 + dy;
                            let nz = lz as i32 + dz;
                            let is_solid = if (0..SIZE_I32).contains(&nx)
                                && (0..SIZE_I32).contains(&ny)
                                && (0..SIZE_I32).contains(&nz)
                            {
                                chunk.get(nx as usize, ny as usize, nz as usize) != 0
                            } else {
                                // World-space check for out-of-chunk neighbours
                                let nwx = wx + dx;
                                let nwy = wy + dy;
                                let nwz = wz + dz;
                                let nh = self.surface_height(nwx, nwz);
                                let base = Self::terrain_voxel_at_with_height(nwy, nh);
                                base != 0 && !self.is_cave_at_with_surface(nwx, nwy, nwz, nh)
                            };
                            if is_solid {
                                solid += 1;
                            }
                        }

                        if solid < 2 {
                            chunk.set(lx, ly, lz, 0);
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // Water fill pass: flood empty space below the water level
        for lx in 0..SIZE {
            for lz in 0..SIZE {
                for ly in 0..SIZE {
                    let wy = world_y + ly as i32;
                    if wy <= WATER_LEVEL && chunk.get(lx, ly, lz) == 0 {
                        chunk.set(lx, ly, lz, WATER);
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
                // Skip trees over cave entrances
                if self.is_cave_at(tx, surface_y, tz) || self.is_cave_at(tx, surface_y - 1, tz) {
                    continue;
                }
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
        // Mountains can reach ~200, so use a very high Y
        let chunk = generator.generate_chunk(IVec3::new(0, 200, 0));
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
        // Two vertically adjacent chunks should produce consistent columns.
        // Cave carving can create air pockets, so we only check terrain
        // consistency where caves aren't involved.
        let lower = generator.generate_chunk(IVec3::new(0, 0, 0));
        let upper = generator.generate_chunk(IVec3::new(0, 1, 0));

        for x in 0..16 {
            for z in 0..16 {
                let wx = x as i32;
                let wz = z as i32;
                let lower_wy = 15; // world y of top of lower chunk
                let upper_wy = 16; // world y of bottom of upper chunk

                // Skip columns where caves are involved
                if generator.is_cave_at(wx, lower_wy, wz) || generator.is_cave_at(wx, upper_wy, wz) {
                    continue;
                }

                let lower_top = lower.get(x, 15, z);
                let upper_bottom = upper.get(x, 0, z);
                // Skip tree voxels
                if lower_top == 0 {
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
        for cy in 0..10 {
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
        for cy in 0..10 {
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
        for cy in 0..10 {
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

    #[test]
    fn terrain_has_tall_mountains() {
        let generator = TerrainGenerator::new(42);
        let mut max_height = 0;
        for cx in -20..20 {
            for cz in -20..20 {
                for lx in 0..SIZE_I32 {
                    for lz in 0..SIZE_I32 {
                        let wx = cx * SIZE_I32 + lx;
                        let wz = cz * SIZE_I32 + lz;
                        let h = generator.surface_height(wx, wz);
                        if h > max_height {
                            max_height = h;
                        }
                    }
                }
            }
        }
        assert!(max_height > 120, "expected mountain heights above 120, got {max_height}");
    }

    #[test]
    fn terrain_has_low_valleys() {
        let generator = TerrainGenerator::new(42);
        let mut min_height = i32::MAX;
        for cx in -20..20 {
            for cz in -20..20 {
                for lx in 0..SIZE_I32 {
                    for lz in 0..SIZE_I32 {
                        let wx = cx * SIZE_I32 + lx;
                        let wz = cz * SIZE_I32 + lz;
                        let h = generator.surface_height(wx, wz);
                        if h < min_height {
                            min_height = h;
                        }
                    }
                }
            }
        }
        assert!(min_height < 50, "expected valley heights below 50, got {min_height}");
    }

    #[test]
    fn trees_sparser_on_slopes() {
        let generator = TerrainGenerator::new(42);
        let mut flat_trees = 0u32;
        let mut steep_trees = 0u32;
        let mut flat_cols = 0u32;
        let mut steep_cols = 0u32;

        for cx in -10..10 {
            for cz in -10..10 {
                for lx in 0..SIZE_I32 {
                    for lz in 0..SIZE_I32 {
                        let wx = cx * SIZE_I32 + lx;
                        let wz = cz * SIZE_I32 + lz;
                        let slope = generator.slope_at(wx, wz);
                        let has_tree = generator.has_tree_at(wx, wz);
                        if slope == 0 {
                            flat_cols += 1;
                            if has_tree { flat_trees += 1; }
                        } else if slope >= 2 {
                            steep_cols += 1;
                            if has_tree { steep_trees += 1; }
                        }
                    }
                }
            }
        }

        // Normalize to density (trees per 10000 columns)
        let flat_density = if flat_cols > 0 { flat_trees as f64 / flat_cols as f64 } else { 0.0 };
        let steep_density = if steep_cols > 0 { steep_trees as f64 / steep_cols as f64 } else { 0.0 };
        assert!(flat_density > steep_density,
            "expected flat areas to have more trees: flat={flat_density:.6} steep={steep_density:.6}");
    }

    #[test]
    fn cave_generation_deterministic() {
        let gen1 = TerrainGenerator::new(42);
        let gen2 = TerrainGenerator::new(42);
        for x in -50..50 {
            for z in -50..50 {
                for y in 0..30 {
                    assert_eq!(gen1.is_cave_at(x, y, z), gen2.is_cave_at(x, y, z));
                }
            }
        }
    }

    #[test]
    fn caves_exist_underground() {
        let generator = TerrainGenerator::new(42);
        let mut found_cave = false;
        'outer: for cx in -5..5 {
            for cz in -5..5 {
                for lx in 0..SIZE_I32 {
                    for lz in 0..SIZE_I32 {
                        let wx = cx * SIZE_I32 + lx;
                        let wz = cz * SIZE_I32 + lz;
                        let surface = generator.surface_height(wx, wz);
                        for wy in 1..surface - 3 {
                            if generator.is_cave_at(wx, wy, wz) {
                                let base_voxel = generator.terrain_voxel_at(wx, wy, wz);
                                if base_voxel != 0 {
                                    found_cave = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(found_cave, "expected to find at least one cave voxel underground");
    }

    #[test]
    fn caves_mostly_underground() {
        let generator = TerrainGenerator::new(42);
        let mut total_cave = 0u32;
        let mut underground_cave = 0u32;
        for cx in -3..3 {
            for cz in -3..3 {
                for lx in 0..SIZE_I32 {
                    for lz in 0..SIZE_I32 {
                        let wx = cx * SIZE_I32 + lx;
                        let wz = cz * SIZE_I32 + lz;
                        let surface = generator.surface_height(wx, wz);
                        for wy in 1..surface {
                            if generator.is_cave_at(wx, wy, wz) {
                                total_cave += 1;
                                if wy < surface {
                                    underground_cave += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        if total_cave > 0 {
            let ratio = underground_cave as f64 / total_cave as f64;
            assert!(ratio > 0.9, "expected >90% caves underground, got {ratio:.2} ({underground_cave}/{total_cave})");
        }
    }

    #[test]
    fn no_trees_in_caves() {
        let generator = TerrainGenerator::new(42);
        for cy in 0..10 {
            for cx in -2..3 {
                for cz in -2..3 {
                    let chunk_pos = IVec3::new(cx, cy, cz);
                    let chunk = generator.generate_chunk(chunk_pos);
                    let world_x = chunk_pos.x * SIZE_I32;
                    let world_y = chunk_pos.y * SIZE_I32;
                    let world_z = chunk_pos.z * SIZE_I32;

                    for lx in 0..SIZE {
                        for ly in 0..SIZE {
                            for lz in 0..SIZE {
                                let v = chunk.get(lx, ly, lz);
                                if v == WOOD || v == LEAVES {
                                    let wx = world_x + lx as i32;
                                    let wy = world_y + ly as i32;
                                    let wz = world_z + lz as i32;
                                    assert!(!generator.is_cave_at(wx, wy, wz),
                                        "tree block at ({wx},{wy},{wz}) is inside a cave");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn caves_carve_through_stone() {
        let generator = TerrainGenerator::new(42);
        let mut found = false;
        'outer: for cx in -5..5 {
            for cz in -5..5 {
                let chunk_pos = IVec3::new(cx, 0, cz);
                let chunk = generator.generate_chunk(chunk_pos);
                let world_x = chunk_pos.x * SIZE_I32;
                let world_z = chunk_pos.z * SIZE_I32;

                for lx in 0..SIZE {
                    for lz in 0..SIZE {
                        let wx = world_x + lx as i32;
                        let wz = world_z + lz as i32;
                        let surface = generator.surface_height(wx, wz);
                        for ly in 0..SIZE {
                            let wy = ly as i32;
                            let v = chunk.get(lx, ly, lz);
                            if wy < surface - DIRT_LAYERS && (v == 0 || v == WATER) {
                                if generator.is_cave_at(wx, wy, wz) {
                                    found = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(found, "expected to find stone carved out by caves");
    }

    #[test]
    fn water_fills_below_level() {
        let generator = TerrainGenerator::new(42);
        let chunk = generator.generate_chunk(IVec3::new(0, 2, 0));
        assert!(
            chunk.data().iter().any(|&v| v == WATER),
            "expected water voxels in chunk spanning water level"
        );
    }

    #[test]
    fn water_does_not_fill_solid() {
        let generator = TerrainGenerator::new(42);
        for cx in -3..3 {
            for cz in -3..3 {
                for cy in 0..4 {
                    let chunk_pos = IVec3::new(cx, cy, cz);
                    let chunk = generator.generate_chunk(chunk_pos);
                    let world_x = chunk_pos.x * SIZE_I32;
                    let world_y = chunk_pos.y * SIZE_I32;
                    let world_z = chunk_pos.z * SIZE_I32;

                    for lx in 0..SIZE {
                        for lz in 0..SIZE {
                            let wx = world_x + lx as i32;
                            let wz = world_z + lz as i32;
                            let height = generator.surface_height(wx, wz);
                            for ly in 0..SIZE {
                                let wy = world_y + ly as i32;
                                if chunk.get(lx, ly, lz) == WATER {
                                    let base = TerrainGenerator::terrain_voxel_at_with_height(wy, height);
                                    // Water should only appear where the voxel would be air,
                                    // either naturally or because a cave carved it out
                                    let is_air = base == 0
                                        || generator.is_cave_at_with_surface(wx, wy, wz, height);
                                    assert!(
                                        is_air,
                                        "water at ({wx},{wy},{wz}) replaced solid terrain (base voxel {base})"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn no_trees_underwater() {
        let generator = TerrainGenerator::new(42);
        for cx in -3..3 {
            for cz in -3..3 {
                for cy in 0..4 {
                    let chunk_pos = IVec3::new(cx, cy, cz);
                    let chunk = generator.generate_chunk(chunk_pos);
                    let world_y = chunk_pos.y * SIZE_I32;

                    for lx in 0..SIZE {
                        for ly in 0..SIZE {
                            let wy = world_y + ly as i32;
                            if wy < WATER_LEVEL {
                                for lz in 0..SIZE {
                                    let v = chunk.get(lx, ly, lz);
                                    assert!(
                                        v != WOOD && v != LEAVES,
                                        "tree voxel {v} found at world y={wy} below water level"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn terrain_caves_persistent_across_restart() {
        let gen1 = TerrainGenerator::new(123);
        let gen2 = TerrainGenerator::new(123);
        for cy in 0..3 {
            for cx in -2..3 {
                for cz in -2..3 {
                    let pos = IVec3::new(cx, cy, cz);
                    assert_eq!(gen1.generate_chunk(pos).data(), gen2.generate_chunk(pos).data(),
                        "chunks at {pos} differ between generator instances with same seed");
                }
            }
        }
    }
}

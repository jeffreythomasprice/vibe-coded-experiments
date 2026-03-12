use glam::IVec3;
use noise::{Fbm, MultiFractal, NoiseFn, Perlin};

use crate::chunk::Chunk;

const SIZE: usize = 16;
const BASE_HEIGHT: f64 = 32.0;
const AMPLITUDE: f64 = 24.0;
const DIRT_LAYERS: i32 = 3;

pub struct TerrainGenerator {
    height_noise: Fbm<Perlin>,
}

impl TerrainGenerator {
    pub fn new(seed: u32) -> Self {
        let height_noise = Fbm::<Perlin>::new(seed)
            .set_octaves(4)
            .set_frequency(0.02)
            .set_lacunarity(2.0)
            .set_persistence(0.5);
        Self { height_noise }
    }

    pub fn generate_chunk(&self, chunk_pos: IVec3) -> Chunk {
        let mut chunk = Chunk::new();
        let world_x = chunk_pos.x * SIZE as i32;
        let world_y = chunk_pos.y * SIZE as i32;
        let world_z = chunk_pos.z * SIZE as i32;

        for lx in 0..SIZE {
            for lz in 0..SIZE {
                let wx = world_x + lx as i32;
                let wz = world_z + lz as i32;
                let noise_val = self.height_noise.get([wx as f64, wz as f64]);
                let height = (BASE_HEIGHT + noise_val * AMPLITUDE) as i32;

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
                if lower_top == 0 {
                    assert_eq!(upper_bottom, 0,
                        "column ({x},{z}): lower top is air but upper bottom is solid");
                }
                if upper_bottom != 0 {
                    assert_ne!(lower_top, 0,
                        "column ({x},{z}): upper bottom is solid but lower top is air");
                }
            }
        }
    }
}

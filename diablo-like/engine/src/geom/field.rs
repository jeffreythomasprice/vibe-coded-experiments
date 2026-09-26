use glam::Vec2;

use super::aabb::Aabb;
use super::parts::Level;

pub const CHUNK_WORLD_SIZE: f32 = 16.0;
pub const TEXELS_PER_UNIT: u32 = 8;
pub const CORE_RES: u32 = CHUNK_WORLD_SIZE as u32 * TEXELS_PER_UNIT;
/// Only needs to cover bilinear filtering across a chunk seam, not the glow
/// radius — each chunk is baked against the *global* `Level`, so a texel
/// near the edge already reports the true distance to a wall in the
/// neighbouring chunk. See the design plan's Chunked field section.
pub const APRON: u32 = 2;
pub const STORED_RES: u32 = CORE_RES + APRON * 2;
pub const CLAMP_RANGE: f32 = 4.0;

const BLOCK_SIZE: u32 = 16;
const BLOCKS_PER_AXIS: u32 = CORE_RES / BLOCK_SIZE;
const BLOCK_WORLD_SIZE: f32 = BLOCK_SIZE as f32 / TEXELS_PER_UNIT as f32;
/// One step per 16x16 core block, plus one finishing step for the (much
/// smaller) apron ring.
const TOTAL_STEPS: u32 = BLOCKS_PER_AXIS * BLOCKS_PER_AXIS + 1;

pub type ChunkCoord = (i32, i32);

pub fn world_to_chunk(p: Vec2) -> ChunkCoord {
    (
        (p.x / CHUNK_WORLD_SIZE).floor() as i32,
        (p.y / CHUNK_WORLD_SIZE).floor() as i32,
    )
}

pub fn chunk_origin(coord: ChunkCoord) -> Vec2 {
    Vec2::new(coord.0 as f32, coord.1 as f32) * CHUNK_WORLD_SIZE
}

pub fn chunk_world_rect(coord: ChunkCoord) -> Aabb {
    let origin = chunk_origin(coord);
    Aabb {
        min: origin,
        max: origin + Vec2::splat(CHUNK_WORLD_SIZE),
    }
}

pub fn chebyshev_distance(a: ChunkCoord, b: ChunkCoord) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

fn block_half_diagonal() -> f32 {
    BLOCK_WORLD_SIZE * std::f32::consts::SQRT_2 / 2.0
}

/// texel `(tx, ty)` of the stored (apron-inclusive) buffer to a world
/// position. `ty` increases with world `+y`, no flip — chunk quads are
/// placed directly in world space, unlike the single fullscreen debug quad
/// in `render/field_debug.rs`, so there's no NDC-vs-image row convention to
/// reconcile.
fn texel_world_pos(coord: ChunkCoord, tx: u32, ty: u32) -> Vec2 {
    let origin = chunk_origin(coord);
    let texel_size = 1.0 / TEXELS_PER_UNIT as f32;
    Vec2::new(
        origin.x + (tx as f32 - APRON as f32 + 0.5) * texel_size,
        origin.y + (ty as f32 - APRON as f32 + 0.5) * texel_size,
    )
}

fn encode(d: f32) -> u16 {
    half::f16::from_f32(d.clamp(-CLAMP_RANGE, CLAMP_RANGE)).to_bits()
}

/// A tile pre-filled with `+CLAMP_RANGE` everywhere — solid rock under this
/// project's sign convention (negative inside floor/walkable space). Shared
/// by every chunk that has no geometry within clamp range anywhere in it,
/// and by chunks still waiting in the bake queue, so nothing ever appears
/// as a black rectangle before its real bake completes.
pub fn void_tile() -> Vec<u16> {
    vec![encode(CLAMP_RANGE); (STORED_RES * STORED_RES) as usize]
}

/// Resumable, block-pruned bake of one chunk's field tile. `step` does a
/// bounded amount of work (one 16x16 block, or the apron-ring finishing
/// step) so a caller can budget bake work across frames — see
/// `render::chunks` for the scheduler that drives this under a per-frame
/// millisecond budget.
pub struct BakeJob {
    pub coord: ChunkCoord,
    data: Vec<u16>,
    next_step: u32,
}

impl BakeJob {
    pub fn new(coord: ChunkCoord) -> Self {
        Self {
            coord,
            data: vec![0u16; (STORED_RES * STORED_RES) as usize],
            next_step: 0,
        }
    }

    pub fn is_done(&self) -> bool {
        self.next_step >= TOTAL_STEPS
    }

    pub fn step(&mut self, level: &Level) {
        if self.is_done() {
            return;
        }
        if self.next_step < BLOCKS_PER_AXIS * BLOCKS_PER_AXIS {
            let block_x = self.next_step % BLOCKS_PER_AXIS;
            let block_y = self.next_step / BLOCKS_PER_AXIS;
            self.bake_core_block(level, block_x, block_y);
        } else {
            self.bake_apron(level);
        }
        self.next_step += 1;
    }

    pub fn into_data(self) -> Vec<u16> {
        self.data
    }

    fn write_texel(&mut self, tx: u32, ty: u32, d: f32) {
        self.data[(ty * STORED_RES + tx) as usize] = encode(d);
    }

    fn bake_core_block(&mut self, level: &Level, block_x: u32, block_y: u32) {
        let base_tx = APRON + block_x * BLOCK_SIZE;
        let base_ty = APRON + block_y * BLOCK_SIZE;
        let center = texel_world_pos(
            self.coord,
            base_tx + BLOCK_SIZE / 2,
            base_ty + BLOCK_SIZE / 2,
        );
        let center_d = level.eval(center).d;

        // The field is a conservative underestimate of true distance (see
        // the design plan), and 1-Lipschitz, so no point in this block can
        // differ from `center_d` by more than the block's half-diagonal —
        // if that's already past the clamp range, the whole block clamps
        // uniformly with no need to evaluate every texel.
        if center_d.abs() > block_half_diagonal() + CLAMP_RANGE {
            let flat = CLAMP_RANGE.copysign(center_d);
            for ty in base_ty..base_ty + BLOCK_SIZE {
                for tx in base_tx..base_tx + BLOCK_SIZE {
                    self.write_texel(tx, ty, flat);
                }
            }
            return;
        }

        for ty in base_ty..base_ty + BLOCK_SIZE {
            for tx in base_tx..base_tx + BLOCK_SIZE {
                let p = texel_world_pos(self.coord, tx, ty);
                let d = level.eval(p).d;
                self.write_texel(tx, ty, d);
            }
        }
    }

    fn bake_apron(&mut self, level: &Level) {
        for ty in 0..STORED_RES {
            for tx in 0..STORED_RES {
                let in_core =
                    (APRON..APRON + CORE_RES).contains(&tx) && (APRON..APRON + CORE_RES).contains(&ty);
                if in_core {
                    continue;
                }
                let p = texel_world_pos(self.coord, tx, ty);
                let d = level.eval(p).d;
                self.write_texel(tx, ty, d);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::primitive::{Circle, Primitive};

    fn decode(bits: u16) -> f32 {
        half::f16::from_bits(bits).to_f32()
    }

    fn run_to_completion(job: &mut BakeJob, level: &Level) {
        while !job.is_done() {
            job.step(level);
        }
    }

    #[test]
    fn chunk_coord_round_trips_through_world_position() {
        let coord = (3, -2);
        let origin = chunk_origin(coord);
        assert_eq!(world_to_chunk(origin + Vec2::splat(0.1)), coord);
        assert_eq!(chunk_world_rect(coord).min, origin);
    }

    #[test]
    fn baked_chunk_matches_level_eval_at_sample_points() {
        let mut level = Level::new(CHUNK_WORLD_SIZE);
        let circle = level.csg.leaf(Primitive::Circle(Circle { radius: 3.0 }));
        level.csg.finalize();
        level.add_part(circle);

        let coord = (0, 0);
        let mut job = BakeJob::new(coord);
        run_to_completion(&mut job, &level);
        let data = job.into_data();

        // Sample at exact texel centers (via the same mapping the bake
        // itself uses) rather than arbitrary points, so the comparison
        // isn't muddied by up to half a texel of positional slop.
        for (tx, ty) in [(10, 10), (40, 4), (100, 100), (20, 90)] {
            let p = texel_world_pos(coord, tx, ty);
            let baked = decode(data[(ty * STORED_RES + tx) as usize]);
            let expected = level.eval(p).d.clamp(-CLAMP_RANGE, CLAMP_RANGE);
            assert!(
                (baked - expected).abs() < 0.01,
                "at texel ({tx},{ty}) / world {p:?}: baked={baked}, expected={expected}"
            );
        }
    }

    #[test]
    fn chunk_far_from_geometry_bakes_to_void() {
        let mut level = Level::new(CHUNK_WORLD_SIZE);
        let circle = level.csg.leaf(Primitive::Circle(Circle { radius: 1.0 }));
        level.csg.finalize();
        level.add_part(circle);

        // Far enough away that no texel, even with the apron, comes within
        // clamp range of the circle at the origin.
        let coord = (10, 10);
        assert!(!level.any_part_overlaps(chunk_world_rect(coord).expand(CLAMP_RANGE)));

        let mut job = BakeJob::new(coord);
        run_to_completion(&mut job, &level);
        let data = job.into_data();

        let void_bits = encode(CLAMP_RANGE);
        assert!(data.iter().all(|&bits| bits == void_bits));
    }
}

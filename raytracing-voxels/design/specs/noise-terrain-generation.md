# Noise-Based Terrain Generation

**Summary:** Replace the existing `generate_test_chunk()` wireframe generator with a noise-based terrain generator that produces coherent heightmap terrain across chunk boundaries, using layered voxel types (grass, dirt, stone).
**Depends on:** None

---

## Steps

### 1.1 Create `TerrainGenerator` struct with noise-based heightmap

**Files:** `src/terrain.rs`

Create a new `TerrainGenerator` struct that owns a seeded noise function and generates chunks based on their world position.

- `TerrainGenerator` fields: `seed: u32`, `height_noise: Fbm<Perlin>` (from the `noise` crate).
- `fn new(seed: u32) -> Self` — constructs the Fbm noise with the given seed, configure octaves (4), frequency (~0.02 for gentle hills at chunk scale), and lacunarity/persistence for natural-looking terrain.
- `fn generate_chunk(&self, chunk_pos: IVec3) -> Chunk` — for each (x, z) column in the 16x16 chunk footprint, compute world coordinates `(chunk_pos.x * 16 + x, chunk_pos.z * 16 + z)`, sample the 2D heightmap noise, scale it to a reasonable range (e.g. base height 32, amplitude 24 for heights ~8–56), then fill voxels below the height: top layer = grass (3), next 3 layers = dirt (2), everything below = stone (1). Voxels above the height = air (0).
- Tests:
  - `terrain_chunk_not_empty` — a chunk at y=0 with default seed has some solid voxels.
  - `terrain_chunk_above_surface_is_air` — a chunk at a very high y position (e.g. y=100) should be all air.
  - `terrain_consistent_across_calls` — generating the same chunk position twice with the same seed produces identical data.
  - `terrain_chunks_share_border_heights` — verify that adjacent chunks at a shared edge produce the same column heights (terrain is continuous).

### 1.2 Add seed to `Config` and wire `TerrainGenerator` into `ChunkManager`

**Files:** `src/config.rs`, `src/chunk_manager.rs`, `src/main.rs`

- Add `seed: u32` field to `Config` (default: `12345`), read from `voxels.toml` under `[world]` section (e.g. `seed = 12345`).
- Change `load_or_generate_chunk` in `chunk_manager.rs` to accept a `&TerrainGenerator` and call `generator.generate_chunk(pos)` instead of `generate_test_chunk()`.
- Create `TerrainGenerator` in `chunk_loader_task` (or pass it in) using the seed from config.
- Update `ChunkManager::new` to accept the seed and forward it to the background task.
- Update `App::new` in `main.rs` to pass `config.seed` when constructing `ChunkManager`.
- Tests:
  - `load_or_generate_uses_terrain` — verify `load_or_generate_chunk` with a `TerrainGenerator` returns a chunk with terrain (not the old wireframe pattern).

### 1.3 Remove old `generate_test_chunk` and update tests

**Files:** `src/chunk.rs`, `src/world.rs`, `src/chunk_manager.rs`

- Delete `generate_test_chunk()` from `chunk.rs`.
- Update all tests in `chunk.rs`, `world.rs`, and `chunk_manager.rs` that relied on `generate_test_chunk()`:
  - Replace with `Chunk::new()` + manual `set()` calls for tests that need specific voxel layouts.
  - For tests that just need "some non-empty chunk", use `TerrainGenerator::new(0).generate_chunk(IVec3::ZERO)`.
- Register `mod terrain` in `main.rs`.
- Verify all existing tests still pass.

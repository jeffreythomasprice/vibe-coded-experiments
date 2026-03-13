# Mountains and Caves

**Summary:** Overhaul terrain generation to produce dramatic mountain ranges with valleys, slope-dependent tree density, and a persistent 3D cave system with narrow tunnels, large caverns, and occasional surface entrances.
**Depends on:** noise-terrain-generation, trees

---

## Steps

### 1. Heightmap overhaul for mountains

**Files:** `src/terrain.rs`

Replace the current flat height generation (`BASE_HEIGHT=32`, `AMPLITUDE=24`, giving ~8–56 range) with a two-layer system that produces both flat valleys and tall mountains:

- Add a new `Fbm<Perlin>` noise field `mountain_noise` to `TerrainGenerator`, seeded deterministically from `seed + 1` (or offset). Use low frequency (~0.005) so mountains are large-scale features.
- The mountain noise output (normalized roughly -1..1) controls a "mountainousness" factor. When the factor is low, terrain stays flat and low. When high, it amplifies the height dramatically.
- New constants: `BASE_HEIGHT = 40.0`, `MIN_AMPLITUDE = 16.0`, `MAX_AMPLITUDE = 160.0`. The effective amplitude interpolates between min and max based on the mountain factor.
- `surface_height()` should combine: `BASE_HEIGHT + height_noise * lerp(MIN_AMPLITUDE, MAX_AMPLITUDE, mountain_factor)` where `mountain_factor = clamp((mountain_noise + 0.3) * 1.5, 0.0, 1.0)` (biased so flat areas are more common).
- Update the height noise frequency to ~0.01 (slightly lower than current 0.02) so terrain features are broader at mountain scale.
- Update `terrain_voxel_at()`: increase `DIRT_LAYERS` on steeper slopes (optional), and above a certain height threshold replace grass with stone (bare mountain peaks).

**Tests:**
- `terrain_has_tall_mountains`: generate many chunks across a wide area, assert that at least one surface height exceeds 120.
- `terrain_has_low_valleys`: assert that some surface heights are below 50.
- Existing `terrain_consistent_across_calls` and `terrain_deterministic_regardless_of_order` should still pass.

### 2. Slope-dependent tree density

**Files:** `src/terrain.rs`

Modify tree placement so valleys have thick forests and mountain slopes are sparse:

- Add a helper method `slope_at(wx, wz) -> f32` to `TerrainGenerator` that samples `surface_height` at the 4 cardinal neighbors and returns the max absolute difference (a discrete slope approximation).
- In `has_tree_at()`, after the existing density check, compute the slope. If slope > 3, reject the tree (no trees on steep cliffs). If slope is 1–3, apply additional thinning (e.g., `tree_hash(..., seed+2) % (slope * 3) != 0`).
- Also reject trees above a height threshold (e.g., surface > BASE_HEIGHT + MAX_AMPLITUDE * 0.7) — no trees near mountain peaks.
- Keep valley tree density the same or slightly increase it (lower `TREE_DENSITY_MOD` to ~150 for flat areas).

**Tests:**
- `trees_sparser_on_slopes`: generate terrain, count trees per column in flat vs steep areas, assert flat areas have more.
- Existing tree tests should still pass (may need updated height ranges in `trees_only_on_surface`).

### 3. Cave noise system

**Files:** `src/terrain.rs`

Add 3D noise functions for cave generation. Use a "Swiss cheese" + "spaghetti" approach:

- Add two new noise fields to `TerrainGenerator`:
  - `cave_worm_noise_a: Fbm<Perlin>` — 3D noise for spaghetti caves (seed offset +10). Frequency ~0.04, 3 octaves.
  - `cave_worm_noise_b: Fbm<Perlin>` — second 3D noise channel (seed offset +11). Same params but different seed.
  - `cave_cavern_noise: Fbm<Perlin>` — lower frequency (~0.015) 3D noise for large caverns (seed offset +12). 2 octaves.
- Add method `is_cave_at(wx, wy, wz) -> bool`:
  - **Spaghetti caves**: Compute `a = cave_worm_noise_a.get([wx, wy, wz])` and `b = cave_worm_noise_b.get([wx, wy, wz])`. The cave exists where `a*a + b*b < WORM_THRESHOLD` (intersection of two noise channels near zero creates winding tunnels). `WORM_THRESHOLD ≈ 0.012`.
  - **Caverns**: Compute `c = cave_cavern_noise.get([wx, wy, wz])`. Cave exists where `c > CAVERN_THRESHOLD` (≈ 0.6). This creates periodic large open spaces.
  - Return true if either spaghetti or cavern condition is met.
- Add a **depth bias**: caves should be more likely deeper underground. Add a factor based on `(surface_height - wy)` — reduce thresholds (make caves more likely) when deeper, increase thresholds near the surface. Specifically:
  - `depth = surface_height(wx, wz) - wy`
  - If `depth < 3`: no caves (preserve surface layer).
  - If `depth < 10`: multiply thresholds by a factor that makes caves rare near surface.
  - Deeper: use base thresholds.
- **Surface entrances**: For a small percentage of locations, relax the near-surface restriction. Use `tree_hash(wx/8, wz/8, seed+20) % 20 == 0` to deterministically select ~5% of 8x8 column regions as "entrance zones" where the depth<10 restriction is lifted (but depth<3 still applies to keep the very top intact, or even depth<1).

**Tests:**
- `cave_generation_deterministic`: same seed produces same `is_cave_at` results.
- `caves_exist_underground`: generate chunks, find at least one air voxel below surface that would have been solid without caves.
- `caves_mostly_underground`: sample many cave voxels, assert >90% have surface above them.

### 4. Integrate cave carving into chunk generation

**Files:** `src/terrain.rs`

Modify `generate_chunk()` to carve caves after base terrain (but before trees):

- After the base terrain pass (the existing `for lx/lz/ly` loop), add a cave carving pass:
  - For each voxel in the chunk that is currently solid (not air), call `is_cave_at(wx, wy, wz)`. If true, set the voxel to air (0).
  - Skip carving the very top surface block to avoid removing the ground under the player's feet everywhere (the depth check in `is_cave_at` handles this).
- Move the tree placement pass to after cave carving. Modify tree placement: if any trunk base position has been carved into a cave (i.e., the surface block was removed by a cave), skip that tree entirely. Check: if `is_cave_at(wx, surface_y, wz)` or `is_cave_at(wx, surface_y-1, wz)`, don't place the tree.
- This ensures no trees spawn floating over cave entrances and no tree blocks exist inside caves.

**Tests:**
- `no_trees_in_caves`: for all wood/leaves voxels, verify `is_cave_at()` is false at that position.
- `caves_carve_through_stone`: verify some voxels that would be stone (below dirt layers, below surface) are air due to caves.
- Existing determinism tests still pass.

### 5. Update existing tests for new height ranges

**Files:** `src/terrain.rs`

Several existing tests assume the old height range (~8–56). Update them:

- `terrain_chunk_above_surface_is_air`: change `IVec3::new(0, 100, 0)` to a much higher Y (e.g., `IVec3::new(0, 200, 0)`) since mountains can now reach ~200.
- `trees_only_on_surface`: this test checks wood is above surface height — still valid but may need the Y range expanded in the scan.
- `terrain_with_trees_has_wood` / `terrain_with_trees_has_leaves`: may need to scan more chunk Y levels since trees can now be at higher elevations.
- Add `terrain_caves_persistent_across_restart`: create two `TerrainGenerator` instances with the same seed, verify identical chunks (caves included). This is mostly covered by existing determinism tests but worth an explicit test name for the cave requirement.

# Trees

**Summary:** Add wood and leaves voxel types with procedural textures, and generate trees on the terrain surface using deterministic placement that works across chunk boundaries.
**Depends on:** noise-terrain-generation, procedural-textures

---

## Steps

### 1. Add wood texture generator

**Files:** `src/voxel_textures.rs`

Add a `generate_wood(seed: u32) -> Texture` function that produces a bark-like pixel art texture inspired by the reference image. The texture should have:

- Vertical grain streaks of varying width and brightness
- Color palette ranging from dark brown `Rgba::rgb(40, 30, 20)` through medium browns `Rgba::rgb(80, 65, 40)` and `Rgba::rgb(100, 90, 60)` to lighter tan `Rgba::rgb(150, 130, 90)`
- Use `sample_fbm_tiling` with high y-frequency and low x-frequency to create the vertical grain pattern (e.g. `x_scale: 0.3, y_scale: 2.0`)
- Layer a second noise pass for knot/variation detail
- Apply `posterize` and `palette_select` to keep the pixel-art feel
- Use `pixel_hash` for per-pixel brightness jitter

Follow the existing parameterized pattern: create `WoodParams` struct with `Default` impl, `generate_wood_parameterized(seed, params)`, and `generate_wood(seed)` wrapper.

Tests:
- `wood_correct_size_and_opaque` — 64x64, all alpha=255
- `wood_deterministic` — same seed produces same output
- `wood_different_seeds_differ` — different seeds produce different output
- `wood_tiles_seamlessly` — reuse `assert_texture_tiles` helper
- `wood_brown_dominant` — mean red > mean blue, mean green < mean red (brown tones)

### 2. Add leaves texture generator

**Files:** `src/voxel_textures.rs`

Add a `generate_leaves(seed: u32) -> Texture` function that produces a high-contrast stippled green leaf texture inspired by the reference image. The texture should have:

- High-contrast mix of bright greens `Rgba::rgb(30, 200, 30)` and dark greens `Rgba::rgb(10, 60, 10)`
- Use a high-frequency noise layer (`frequency: 12.0+`) with posterization to create the blocky stippled look
- Layer a lower-frequency cluster noise to create patches of light/dark
- Strong green saturation — green channel should dominate red and blue significantly
- More contrast than the grass texture (wider spread between light and dark)

Follow the existing parameterized pattern: create `LeavesParams` struct with `Default` impl, `generate_leaves_parameterized(seed, params)`, and `generate_leaves(seed)` wrapper.

Tests:
- `leaves_correct_size_and_opaque` — 64x64, all alpha=255
- `leaves_deterministic` — same seed produces same output
- `leaves_different_seeds_differ` — different seeds produce different output
- `leaves_tiles_seamlessly` — reuse `assert_texture_tiles` helper
- `leaves_green_dominant` — >95% of pixels have green > red and green > blue
- `leaves_high_contrast` — green channel stddev > 25

### 3. Register wood and leaves in TILE_DEFS

**Files:** `src/voxel_textures.rs`

Add two entries to `TILE_DEFS`:
- `TileDef { id: 5, name: "wood", generator: generate_wood }`
- `TileDef { id: 6, name: "leaves", generator: generate_leaves }`

This automatically integrates them into the texture atlas via `build_voxel_atlas`.

Tests:
- `atlas_has_valid_uv_for_ids_1_to_6` — update the existing `atlas_has_valid_uv_for_ids_1_to_4` test to check IDs 1–6

### 4. Add tree structure generation

**Files:** `src/terrain.rs`

Add a `TreePlacer` struct (or integrate into `TerrainGenerator`) that deterministically decides where trees go and generates their voxel structure. Key design:

- **Tree position selection:** Use a separate `Perlin` noise or hash-based approach seeded from the world seed. For each XZ column, compute a "tree score" — if it exceeds a threshold, a tree is placed there. This must be deterministic based only on world (x, z) coordinates so it works identically regardless of which chunk is being generated.
- **Tree parameters per position:** Use `pixel_hash(wx, wz, tree_seed)` to derive per-tree randomness:
  - Trunk height: 4–8 blocks
  - Number of branches: 2–5
  - Branch direction offsets and arc shapes
- **Tree structure:** A tree rooted at surface position `(wx, surface_y, wz)` consists of:
  - A vertical column of wood (voxel ID 5) from `surface_y + 1` to `surface_y + trunk_height`
  - 2–5 branches starting near the top third of the trunk, arcing outward in diagonal directions (combinations of +x/-x/+z/-z with +y), each 3–5 blocks long, made of wood
  - A roughly spherical/ellipsoidal canopy of leaves (voxel ID 6) centered around the top of the trunk, radius ~3–4 blocks, filling air voxels only
- **Cross-chunk placement:** When generating a chunk, check all potential tree positions whose structure could overlap with the chunk's bounding box. This means scanning tree roots within ~5 blocks outside the chunk's XZ range and considering the vertical extent. For each candidate tree, compute its full structure and write any voxels that fall within the current chunk.
- **Surface detection:** Reuse the same height noise from `TerrainGenerator` to compute `surface_y` for any world (x, z) without needing the chunk data.
- **Placement constraints:** Only place trees where the surface block is grass (voxel ID 3). Skip positions on steep slopes (optional) or where trees would overlap.

Modify `TerrainGenerator::generate_chunk` to call tree placement after the base terrain fill. Trees overwrite air voxels with wood/leaves but do not replace existing solid terrain.

Tests:
- `terrain_with_trees_has_wood` — generate chunks around y=0..3 at the origin and verify at least some wood voxels (ID 5) exist
- `terrain_with_trees_has_leaves` — same, verify leaves voxels (ID 6) exist
- `tree_placement_deterministic` — generating the same chunk twice produces identical results
- `tree_placement_cross_chunk_consistent` — a tree trunk that starts in one chunk and extends into the chunk above should appear in both chunks
- `trees_only_on_surface` — wood voxels at the base of a tree should be at `surface_y + 1`, not underground

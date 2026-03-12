# Procedural Noise Textures

**Summary:** Replace the placeholder texture generators for grass, dirt, stone, and brick in `voxel_textures.rs` with noise-based procedural generators at 64×64 resolution. The existing `VoxelTextureAtlas`, `TileDef` table, and `build_voxel_atlas()` function stay in place — only the generators and tile size change.
**Depends on:** texture-atlas (existing), voxel_textures.rs (existing)

---

## Steps

### 7.1 Add noise crate and create noise helpers

**Files:** `Cargo.toml`, `src/voxel_textures.rs`

- Add the `noise` crate dependency (`noise = "0.9"`) to `Cargo.toml`. This provides Perlin, Simplex, Worley (cell), and fBm combinators.
- In `src/voxel_textures.rs`, add noise-related helper functions (these can live as private helpers in the same module):
  - `fn sample_fbm(x: f64, y: f64, octaves: u32, frequency: f64, lacunarity: f64, persistence: f64, seed: u32) -> f64` — computes fractal Brownian motion by summing multiple octaves of Perlin noise. Returns a value in approximately [-1, 1].
  - `fn noise_to_u8(value: f64, min: f64, max: f64) -> u8` — maps a noise value from [min, max] to [0, 255], clamping.
  - `fn lerp_color(a: Rgba, b: Rgba, t: f64) -> Rgba` — linearly interpolates between two colors.
- Change `TILE_SIZE` from `16` to `64`.
- Update `TileDef.generator` signature from `fn(&mut Texture)` to `fn(u32) -> Texture` (takes a seed, returns a new `Texture` at `TILE_SIZE`). Update `build_voxel_atlas()` accordingly: instead of creating a blank texture and calling the generator, just call the generator with the seed and pass the result to the builder.
- Add a `seed` parameter to `build_voxel_atlas(seed: u32)` and update the call site in `main.rs`.
- **Tests:**
  - `sample_fbm` returns values in a reasonable range (sample 100 points, all within [-2, 2]).
  - `noise_to_u8(0.0, -1.0, 1.0)` returns 128 (midpoint).
  - `lerp_color` at t=0 returns `a`, at t=1 returns `b`, at t=0.5 returns midpoint.

### 7.2 Grass texture generator

**Files:** `src/voxel_textures.rs`

- Replace the grass `TileDef` generator (currently `gen_checkerboard`) with a new `fn generate_grass(seed: u32) -> Texture`.
- Use 2–3 octaves of fBm to create base variation. Map the noise output to a green palette ranging from dark green `Rgba::rgb(34, 100, 34)` to bright green `Rgba::rgb(68, 160, 50)`.
- Add a second low-amplitude noise layer with a different seed to introduce yellow-brown patches: blend toward `Rgba::rgb(120, 120, 40)` where the second noise exceeds a threshold.
- Scatter sparse single-pixel highlights using a hash function on pixel coordinates (pseudo-random, deterministic): ~5% of pixels get a slight brightness boost to simulate individual blade tips.
- **Tests:**
  - Output texture is `TILE_SIZE × TILE_SIZE` with all pixels having `a == 255`.
  - Same seed produces identical textures (determinism).
  - Different seeds produce different textures.

### 7.3 Dirt texture generator

**Files:** `src/voxel_textures.rs`

- Replace the dirt `TileDef` generator (currently `gen_solid`) with a new `fn generate_dirt(seed: u32) -> Texture`.
- Use fBm with 3–4 octaves for the base, mapping to a brown palette from dark brown `Rgba::rgb(80, 50, 30)` to lighter brown `Rgba::rgb(140, 100, 60)`.
- Add salt-and-pepper noise: ~3% of pixels are darker (small rocks) and ~2% are lighter (grains/sand). Use a simple hash on `(x, y, seed)` to determine which pixels get these spots.
- **Tests:**
  - Output texture is `TILE_SIZE × TILE_SIZE` with all pixels having `a == 255`.
  - Deterministic for same seed.

### 7.4 Stone texture generator with Worley noise

**Files:** `src/voxel_textures.rs`

- Replace the stone `TileDef` generator (currently `gen_solid`) with a new `fn generate_stone(seed: u32) -> Texture`.
- Use the `noise` crate's Worley noise (or implement a simple Worley: scatter ~20 seed points in the 64×64 field using a seeded RNG, compute F1 and F2 distances per pixel). Compute `F2 - F1` to get pebble edge contrast.
- Map F2-F1 values to a gray palette: base stone color `Rgba::rgb(128, 128, 128)` with darker edges where F2-F1 is small (cell boundaries) and lighter interiors.
- Blend with a low-amplitude Perlin layer for subtle background color variation (slight warm/cool shifts across the surface).
- **Tests:**
  - Output texture is `TILE_SIZE × TILE_SIZE` with all pixels having `a == 255`.
  - Deterministic for same seed.
  - Pixel values span a reasonable range (not all one color) — check that the standard deviation of brightness across all pixels is > 5.

### 7.5 Brick texture generator

**Files:** `src/voxel_textures.rs`

- Replace the brick `TileDef` generator (currently `gen_border`) with a new `fn generate_brick(seed: u32) -> Texture`.
- Divide the texture into a grid of brick rows. At 64×64, use 8 rows of 8px height each, with bricks 16px wide. Offset every other row by half a brick width (running bond pattern).
- Mortar lines: 2px wide gaps between bricks, filled with mortar color `Rgba::rgb(180, 175, 165)`. Determine mortar vs brick by checking if the pixel falls within the 2px border zone of any grid cell.
- Per-brick coloring: use the brick's grid coordinates `(brick_col, brick_row)` as a seed for a small fBm sample. Map to a brick-red palette ranging from `Rgba::rgb(140, 55, 40)` to `Rgba::rgb(180, 80, 55)`. Add a per-pixel low-frequency noise within each brick for surface texture.
- **Tests:**
  - Output texture is `TILE_SIZE × TILE_SIZE` with all pixels having `a == 255`.
  - Deterministic for same seed.
  - Mortar pixels exist: at least some pixels match the mortar color range (gray-ish, R > 150, G > 150).
  - Brick pixels exist: at least some pixels are in the red-brown range (R > 100, G < 100).

### 7.6 Remove unused tiles and clean up

**Files:** `src/voxel_textures.rs`, `src/chunk.rs`

- Remove the wood, sand, and water entries from `TILE_DEFS`. Renumber brick from id 5 to id 4. The final tile IDs are: stone=1, dirt=2, grass=3, brick=4.
- Remove the old `gen_solid`, `gen_checkerboard`, `gen_stripes_h`, `gen_stripes_v`, and `gen_border` helper functions.
- In `generate_test_chunk()`, the interior voxels currently use ids 4 (wood) and 5 (brick). Update them to use ids from the remaining set (e.g. both groups use brick=4, or split between grass=3 and brick=4).
- **Tests:**
  - `build_voxel_atlas(42)` succeeds.
  - The returned atlas has valid UV rects for IDs 1–4 (stone, dirt, grass, brick).
  - Atlas texture dimensions are at least 128×128 (fits four 64×64 tiles).
  - ID 0 UV rect is still zeroed out.

# Improved Pixel-Art Textures

**Summary:** Overhaul the grass and dirt procedural texture generators in `voxel_textures.rs` to produce convincing low-res pixel art. Add noise composition helpers (posterization, palette selection, directional stretching, dithering, layered combination) and rewrite both generators to use palette-based hard thresholds instead of smooth interpolation.
**Depends on:** procedural-textures (existing)

---

## Steps

### 1.1 Add noise composition helpers

**Files:** `src/voxel_textures.rs`

Add the following private helper functions for building layered noise textures. These compose on top of the existing `sample_fbm`, `noise_to_u8`, `lerp_color`, and `pixel_hash` helpers.

- `fn posterize(t: f64, levels: u32) -> f64` — quantizes a `[0, 1]` value to discrete steps: `(t * levels as f64).floor() / (levels - 1).max(1) as f64)`, clamped to `[0, 1]`. This is the core operation for pixel-art noise — it replaces smooth gradients with hard color bands.
- `fn palette_select(palette: &[Rgba], t: f64) -> Rgba` — given a `[0, 1]` value, selects the palette entry at index `floor(t * len)`, clamped. No interpolation — hard jumps between colors. This replaces `lerp_color` for pixel-art use.
- `fn sample_fbm_stretched(x: f64, y: f64, x_scale: f64, y_scale: f64, octaves: u32, frequency: f64, lacunarity: f64, persistence: f64, seed: u32) -> f64` — like `sample_fbm` but applies separate x/y scaling before sampling, enabling directional stretching (e.g. vertical grass streaks via `y_scale: 0.3`).
- `fn noise_layer(x: u32, y: u32, tile_size: u32, config: &NoiseLayerConfig) -> f64` — a convenience wrapper that normalizes pixel coords to `[0,1]`, calls `sample_fbm_stretched` with config parameters, and returns a `[0, 1]` value. `NoiseLayerConfig` is a struct:
  ```rust
  struct NoiseLayerConfig {
      octaves: u32,
      frequency: f64,
      lacunarity: f64,
      persistence: f64,
      x_scale: f64,  // 1.0 = no stretch
      y_scale: f64,  // 1.0 = no stretch
      seed: u32,
  }
  ```
- `fn combine_layers(layers: &[f64], weights: &[f64]) -> f64` — weighted sum of multiple noise layer outputs, normalized to `[0, 1]`. Sum of `layers[i] * weights[i]` divided by sum of `weights`, clamped.
- `fn bayer_dither_2x2(x: u32, y: u32, t: f64, levels: u32) -> u32` — ordered dithering using the 2x2 Bayer matrix `[[0,2],[3,1]] / 4.0`. Given a `[0, 1]` value and desired number of output levels, returns the dithered level index. For transitions between two palette colors, this produces the classic pixel-art dither pattern instead of blending.
- `fn darken(color: Rgba, amount: u8) -> Rgba` — subtracts `amount` from each RGB channel with saturating subtraction. Convenience for crevice/shadow effects.
- `fn brighten(color: Rgba, amount: u8) -> Rgba` — adds `amount` to each RGB channel with saturating addition.

**Tests:**
- `posterize(0.0, 4)` returns `0.0`, `posterize(0.99, 4)` returns `1.0`, `posterize(0.3, 4)` returns `0.333...` (level 1 of 4).
- `palette_select` with a 4-color palette: `t=0.0` returns color 0, `t=0.99` returns color 3, `t=0.5` returns color 2.
- `sample_fbm_stretched` with `x_scale: 1.0, y_scale: 1.0` matches `sample_fbm` output.
- `combine_layers(&[0.5, 1.0], &[1.0, 1.0])` returns `0.75`.
- `bayer_dither_2x2` at the four 2x2 positions with `t=0.5, levels=2` returns a mix of 0s and 1s (not all the same).
- `darken(Rgba::rgb(100, 50, 20), 30)` returns `Rgba::rgb(70, 20, 0)`.
- `brighten(Rgba::rgb(200, 230, 250), 30)` returns `Rgba::rgb(230, 255, 255)`.

### 1.2 Rewrite dirt texture generator

**Files:** `src/voxel_textures.rs`

Replace `generate_dirt` with a palette-based, layered approach:

- Define a 4-color dirt palette:
  - Dark soil: `Rgba::rgb(60, 35, 20)`
  - Mid brown: `Rgba::rgb(100, 65, 40)`
  - Sandy brown: `Rgba::rgb(130, 95, 55)`
  - Reddish-brown: `Rgba::rgb(110, 55, 30)`
- **Base layer:** Use `noise_layer` with 3 octaves, frequency 4.0. Posterize to 3 levels. Use `palette_select` with the first 3 palette colors.
- **Hue variation layer:** Second `noise_layer` with 2 octaves, frequency 3.0, different seed. Where this exceeds 0.6 (hard threshold), replace the base color with the reddish-brown palette entry. No blending — hard swap.
- **Pebble spots:** Use `pixel_hash` to scatter 5-8 small circles (2-3px radius). For each pixel, check distance to each pebble center. If within radius, use `darken(base_color, 25)` to create a distinct darker spot. Pebble centers are determined by hashing index values with the seed.
- **Grit layer:** Use `noise_layer` with 2 octaves at high frequency (12.0-16.0), low amplitude. Apply as brightness offset via `brighten`/`darken` with small amounts (5-10). This replaces the old salt-and-pepper with clustered grain.
- **Crevice darkening:** A separate low-frequency `noise_layer` (1 octave, frequency 2.0). Where it dips below 0.3, apply `darken(color, 20)`. Only darkens, never lightens.

**Tests:**
- Output texture is `TILE_SIZE × TILE_SIZE` with all pixels `a == 255`.
- Same seed produces identical textures (determinism).
- Different seeds produce different textures.
- At least 3 distinct color values in output (not just two tones).
- Standard deviation of red channel > 10 (ensures visible variation).

### 1.3 Rewrite grass texture generator

**Files:** `src/voxel_textures.rs`

Replace `generate_grass` with a directional, palette-based approach:

- Define a 4-color grass palette:
  - Dark green (shadow): `Rgba::rgb(25, 75, 25)`
  - Mid green: `Rgba::rgb(45, 120, 40)`
  - Bright green: `Rgba::rgb(70, 165, 55)`
  - Dead grass (yellow-brown): `Rgba::rgb(120, 115, 45)`
- **Base layer:** Use `sample_fbm_stretched` with `y_scale: 0.3` to create vertical streaks that read as grass blade direction. 3 octaves, frequency 4.0. Posterize to 3 levels, select from the first 3 green palette entries.
- **Dead grass patches:** Second `noise_layer` with 2 octaves, frequency 2.5, different seed. Hard threshold at 0.45 — above this, replace color with the dead grass palette entry. No gradual blend.
- **Blade tip highlights:** Use `pixel_hash` to select ~5% of pixels. For selected pixels, also select the pixel directly above (if it exists) to form 2px vertical dashes. Apply `brighten(color, 25)` to these pixels.
- **Crevice/shadow darkening:** Low-frequency `noise_layer` (1 octave, frequency 2.0). Below threshold 0.35, apply `darken(color, 18)`. Creates depth between clumps.
- **Dithered transitions:** At boundaries where the posterized base level changes, use `bayer_dither_2x2` to create a 1-pixel dither band instead of a hard edge. Detect boundaries by checking if the un-posterized noise value is within 0.05 of a level boundary.

**Tests:**
- Output texture is `TILE_SIZE × TILE_SIZE` with all pixels `a == 255`.
- Same seed produces identical textures (determinism).
- Different seeds produce different textures.
- Green channel mean is higher than red channel mean (it's actually green).
- Has some pixels matching dead grass hue (red channel > green channel or close).
- Standard deviation of green channel > 15 (visible variation, more than dirt).

### 1.4 Clean up old helpers and verify integration

**Files:** `src/voxel_textures.rs`, `src/main.rs`

- Remove any helper functions that are no longer called after the rewrite (check if `lerp_color` is still used by stone/brick generators — if so, keep it).
- Verify that `build_voxel_atlas` still works correctly with the new generators. The atlas packing, UV mapping, and GPU upload path should be unchanged.
- Run the application and visually verify the textures render correctly on voxel faces.
- Ensure existing tests for stone and brick generators still pass unchanged.

**Tests:**
- All existing tests in `voxel_textures.rs` pass (stone, brick, atlas tests).
- `build_voxel_atlas(42)` succeeds and returns valid UV rects for IDs 1-4.
- No compiler warnings about unused functions.

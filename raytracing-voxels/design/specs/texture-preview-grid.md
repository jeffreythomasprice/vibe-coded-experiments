# Texture Preview Grid

**Summary:** Add a texture preview mode that displays an 8x8 grid of procedurally generated grass texture variants across the window, varying key parameters (palette colors, noise frequencies, blend weights, posterization levels) so the user can visually compare and pick the best-looking combination. Later extends to dirt and stone.
**Depends on:** improved-pixel-art-textures

---

## Steps

### 1.1 Extract grass parameters into `GrassParams` struct

**Files:** `src/voxel_textures.rs`

Create a `GrassParams` struct that captures all tunable parameters currently hardcoded in `generate_grass`:

```rust
pub struct GrassParams {
    pub palette: Vec<Rgba>,
    pub cluster_frequency: f64,      // currently 3.0
    pub cluster_octaves: u32,        // currently 3
    pub detail_frequency: f64,       // currently 12.0
    pub detail_octaves: u32,         // currently 2
    pub cluster_weight: f64,         // currently 0.65
    pub detail_weight: f64,          // currently 0.25
    pub hash_jitter_range: f64,      // currently 0.075
    pub posterize_levels: u32,       // currently palette.len()
    pub brightness_shift_range: i16, // currently 5
}
```

Add `GrassParams::default()` returning the current hardcoded values. Add `generate_grass_parameterized(seed: u32, params: &GrassParams) -> Texture` that uses these params. Change `generate_grass` to call `generate_grass_parameterized` with `GrassParams::default()`.

No tests needed — this is throwaway tooling.

### 1.2 Build the 8x8 preview texture atlas

**Files:** `src/voxel_textures.rs`

Add `pub fn build_grass_preview_atlas(seed: u32) -> Result<TextureAtlas>` that:

1. Creates 64 `GrassParams` variants by systematically varying 2-3 key parameters across the grid (e.g., rows vary `cluster_frequency` from 1.0 to 8.0, columns vary `detail_frequency` from 4.0 to 20.0 — or palette hue shifts, blend weights, etc.).
2. Generates a 64px tile for each via `generate_grass_parameterized`.
3. Packs all 64 tiles into a `TextureAtlas` with names like `"grass_0_0"` through `"grass_7_7"`.

The exact parameter axes can be refined later; the important thing is that each cell is visibly different.


### 1.3 Add preview mode toggle to `App`

**Files:** `src/main.rs`

Add a `preview_mode: bool` field to `App` (default `false`). Toggle it with the `P` key in `window_event`. When entering preview mode:

- Release the cursor (call `release_cursor`).
- Build the grass preview atlas via `build_grass_preview_atlas`.
- Upload it as an overlay texture via `renderer.overlay().create_texture(...)`.
- Store the resulting `wgpu::BindGroup` and `TextureAtlas` in new `App` fields (`preview_atlas: Option<TextureAtlas>`, `preview_bind_group: Option<wgpu::BindGroup>`).

When exiting preview mode, drop the stored atlas/bind group (set to `None`).

### 1.4 Render the 8x8 grid in overlay pass

**Files:** `src/main.rs`

In `RedrawRequested`, when `preview_mode` is true:

1. Skip voxel camera movement (freeze camera controls).
2. Clear the draw list, then fill it with 64 `atlas_rect` quads arranged in an 8x8 grid that fills the window. Compute cell size from the window dimensions (`width / 8`, `height / 8`). Use the UV rects from the stored `TextureAtlas` for each cell.
3. Render the overlay pass using the preview bind group instead of the font bind group.
4. Optionally draw a label in each cell showing the parameter values (using the existing font system) — this can be deferred to a follow-up step if it adds too much complexity.


### 1.5 Add parameter labels to each grid cell

**Files:** `src/main.rs`

When in preview mode, after drawing the 64 texture quads, draw text labels in each cell showing the varied parameter values (e.g., `"f=3.0 d=12.0"`). This requires two overlay render passes per frame in preview mode: one with the preview texture bind group for the tile quads, and one with the font bind group for the text labels.

Update `Renderer::render_overlay` or call it twice — once for tiles, once for labels — building separate `DrawList`s or clearing and refilling between calls.

### 1.6 Parameterize dirt and stone generators

**Files:** `src/voxel_textures.rs`

Follow the same pattern as step 1.1 for `generate_dirt` and `generate_stone`:

- `DirtParams` struct with fields for palette colors, noise configs (base/hue/grit/crevice frequencies, octaves), pebble count range, darkening amounts, and thresholds.
- `StoneParams` struct with fields for point count, edge divisor, fbm frequency/octaves, warm shift scale, base gray range.
- `generate_dirt_parameterized(seed, &DirtParams)` and `generate_stone_parameterized(seed, &StoneParams)`.
- Wire `generate_dirt` / `generate_stone` to call through with `Default` params.

No tests needed — throwaway tooling.

### 1.7 Cycle between texture types in preview mode

**Files:** `src/main.rs`, `src/voxel_textures.rs`

Add `build_dirt_preview_atlas` and `build_stone_preview_atlas` functions analogous to `build_grass_preview_atlas`. Add a `preview_texture_type: usize` field to `App` that cycles through grass/dirt/stone when pressing `N` (next) or `B` (back) while in preview mode. Rebuild the preview atlas when the type changes.

# Transparent Voxel Textures

**Summary:** Add alpha-tested transparency to the voxel raytracer so certain voxel types (starting with leaves) can have see-through holes in their textures. The ray continues through transparent texels, naturally crossing chunk boundaries via the existing BVH traversal.
**Depends on:** None (builds on existing texture generation and raymarching)

---

## Approach

Alpha testing (binary: alpha < 0.5 = skip, else opaque) rather than alpha blending. This is simple, performant, fits the voxel aesthetic, and requires no changes to chunk data, BVH traversal, renderer blend state, or any buffer layouts.

## Steps

### 1. Add alpha cutoff to leaves texture generation

**Files:** `src/voxel_textures.rs`

Add `alpha_cutoff: f64` field to `LeavesParams` with default ~0.22. In `generate_leaves_parameterized`, after computing the combined noise value `t` (line 741-744), use `t` to decide transparency: if `t < alpha_cutoff`, set alpha to 0 instead of 255.

Currently the final pixel is set with `Rgba::rgb(...)` which produces alpha=255. Change to use alpha=0 when `t < alpha_cutoff`, otherwise alpha=255. The `Rgba` type already supports an alpha channel.

Update the three existing tests that assume all-opaque pixels:
- `leaves_correct_size_and_opaque` — rename or adjust to account for transparent pixels (some pixels will have alpha=0)
- `leaves_green_dominant` — filter to only count opaque pixels when checking green dominance
- `leaves_high_contrast` — filter to only use opaque pixels for brightness variance calculation

Add two new tests:
- `leaves_alpha_binary` — verify all alpha values are either 0 or 255, nothing in between
- `leaves_transparency_ratio` — verify that roughly 15-35% of pixels are transparent (alpha=0)

### 2. Alpha test in shader DDA raymarching loop

**Files:** `src/voxels.wgsl`

Restructure the hit block in `march_chunk` (lines 137-180). Currently when `v != 0u`, it samples the texture and returns immediately. Change so that after sampling, it checks `tex_color.a >= 0.5`:

- If alpha >= 0.5: existing lighting calculation + return (opaque hit, no behavior change)
- If alpha < 0.5: alpha test failed, fall through to the DDA stepping code below

The DDA stepping code (lines 182-198) already runs when `v == 0u`. After this change it also runs when the alpha test fails. The key structural change is removing the early `return result` from inside the `if v != 0u` block and instead making it conditional on passing the alpha test. The DDA step and bounds check must execute for both air voxels and transparent-texel hits.

No structural changes to the DDA algorithm or BVH traversal — when `march_chunk` finds no opaque hit after traversing the chunk, it returns `hit = false` and the BVH naturally tries the next chunk.

No changes needed to: `voxel_renderer.rs`, `chunk.rs`, `world.rs`, `terrain.rs`, `bvh.rs`, `overlay.rs`, or any buffer layouts.

## Verification

```
cargo test && cargo clippy && cargo build
```
Then run the app and look at trees — leaves should have visible holes showing sky/terrain behind them.

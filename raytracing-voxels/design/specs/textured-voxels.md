# Textured Voxels

**Summary:** Render each voxel face using a texture sampled from a texture atlas on the GPU, where the voxel ID (1–255) maps to a specific tile in the atlas. Voxel ID 0 remains empty air.
**Depends on:** voxel-chunk-raytracing (Phase 3)

---

## Steps

### 6.1 Voxel texture atlas builder on the CPU

**Files:** `src/voxel_textures.rs`, `src/main.rs`

Create the CPU-side voxel texture atlas:

- Create a new module `voxel_textures.rs` with a function `fn build_voxel_atlas() -> Result<VoxelTextureAtlas>` that programmatically generates a set of distinct 16x16 tile images (one per voxel type) and packs them into a texture atlas.
- `VoxelTextureAtlas` wraps a `Texture` (the packed atlas image) and a mapping from voxel ID (`u8`, 1–255) to UV rectangles `[u_min, v_min, u_max, v_max]`.
- For now, generate simple procedural textures to distinguish voxel types — e.g., solid colors, checkerboard patterns, stripe patterns. Start with at least 4–5 distinct tiles.
- Store the UV mapping as a flat array: `[[f32; 4]; 256]` where index 0 is unused (air). This array will be uploaded to the GPU as a storage buffer.
- Add `mod voxel_textures;` to `main.rs`.
- **Tests:**
  - Building the atlas succeeds and produces a non-empty texture.
  - UV rects for IDs 1–N are valid (min < max, within 0..1 range).
  - ID 0 UV rect is zeroed or ignored.

### 6.2 Upload voxel texture atlas to the GPU

**Files:** `src/voxel_renderer.rs`

Add the atlas texture and UV mapping buffer to the voxel render pipeline:

- Add a new `wgpu::Texture` + `TextureView` + `Sampler` for the voxel atlas image. Upload the `Texture` pixel data from step 6.1 using `queue.write_texture()`.
- Add a new `wgpu::Buffer` (storage, read-only) of size `256 * 4 * sizeof(f32)` = 4096 bytes to hold the UV mapping array. Upload the `[[f32; 4]; 256]` data.
- Create a new bind group (`@group(2)`) with three entries:
  - binding 0: the atlas texture (texture_2d<f32>)
  - binding 1: the sampler
  - binding 2: the UV mapping storage buffer
- Update the `PipelineLayout` to include the new bind group layout: `[&camera_bgl, &chunk_bgl, &atlas_bgl]`.
- Add a method `fn upload_voxel_atlas(&self, atlas_texture: &Texture, uv_map: &[[f32; 4]; 256])` to `Renderer` (where `Texture` is the overlay `Texture` type — the raw pixel data).
- Store the atlas bind group in `Renderer` and set it during `render_voxels`.

### 6.3 Shader: compute face UVs from ray hit position

**Files:** `src/voxels.wgsl`

Modify the fragment shader to compute per-face texture coordinates when a voxel is hit:

- Add bind group 2 declarations:
  ```wgsl
  @group(2) @binding(0) var atlas_tex: texture_2d<f32>;
  @group(2) @binding(1) var atlas_sampler: sampler;
  @group(2) @binding(2) var<storage, read> uv_map: array<vec4<f32>>;
  ```
- When the DDA hits a solid voxel (value `v != 0`), compute the hit point in local voxel space: `hit = ro + rd * t_hit - (chunk_min + vec3<f32>(voxel))`. The hit point's two non-normal components give the face UV (0..1 within that voxel face).
- Determine which face was hit from the `normal` vector: if normal is along X, use (y,z) fract; if Y, use (x,z) fract; if Z, use (x,y) fract. Take the fractional part of the hit position in those axes to get `face_uv` in `[0, 1]`.
- Look up the voxel's UV rect from the mapping buffer: `let uv_rect = uv_map[v]`. Remap `face_uv` into the atlas UV range: `atlas_uv = uv_rect.xy + face_uv * (uv_rect.zw - uv_rect.xy)`.
- Sample the atlas texture: `let tex_color = textureSample(atlas_tex, atlas_sampler, atlas_uv)`.
- Apply existing lighting (ambient + diffuse) to `tex_color` instead of the hardcoded `vec3<f32>(0.9, 0.3, 0.2)`.

### 6.4 Update test chunk to use multiple voxel types

**Files:** `src/chunk.rs`

Update `generate_test_chunk()` to assign different voxel IDs so that the textured rendering is visually testable:

- Instead of setting all edge voxels to `1`, assign different IDs to different edges or faces. For example: edges along the X axis get ID 1, edges along Y get ID 2, edges along Z get ID 3. Or assign IDs based on position hash.
- Add a few solid interior voxels with other IDs (e.g., ID 4, 5) so that multiple textures are visible.
- Update existing tests if the exact voxel values change (tests check `!= 0`, so this should be minimal).

### 6.5 Wire up atlas creation and upload in App

**Files:** `src/main.rs`

Connect the voxel texture atlas to the renderer during initialization:

- In `try_resume()`, call `build_voxel_atlas()` to create the atlas.
- Call `renderer.upload_voxel_atlas(atlas.texture(), atlas.uv_map())` to upload both the atlas texture and UV mapping to the GPU.
- Verify that the textured voxels render correctly by running the app: each voxel type should display its distinct texture on all visible faces, with correct orientation and lighting.

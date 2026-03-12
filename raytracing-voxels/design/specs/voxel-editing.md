# Voxel Editing Tools

**Summary:** Add voxel placement and removal via mouse clicks, with a selectable active voxel type (1-4), a HUD showing the active type with texture preview, and a crosshair overlay.
**Depends on:** multi-chunk-world, textured-voxels, immediate-mode-2d-overlay

---

## Steps

### 1. CPU raycast through the world

**Files:** `src/world.rs`

Implement a `raycast` method on `World` that performs DDA ray marching through loaded chunks on the CPU side, mirroring the GPU shader logic.

```rust
pub struct RaycastHit {
    pub chunk_pos: IVec3,      // chunk coordinate in world grid
    pub local_pos: [usize; 3], // voxel position within the chunk (0..15)
    pub normal: IVec3,         // face normal of the hit (-1/0/+1 per axis)
    pub voxel_id: u8,
}

impl World {
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<RaycastHit>;
}
```

Algorithm:
- Compute the starting world-space voxel coordinate from `origin`.
- Use standard 3D DDA stepping: compute `t_max` and `t_delta` per axis, step through voxels.
- At each step, convert world voxel coords to `(chunk_pos, local_pos)` via `divmod 16`. Look up the chunk in the world hashmap. If the chunk is not loaded, skip (treat as air). If the voxel is non-zero, return `RaycastHit`.
- Track the face normal by recording which axis was last stepped.
- Stop after `max_distance` units.

Tests:
- Ray hitting a known solid voxel returns correct chunk_pos, local_pos, voxel_id.
- Ray hitting a face returns the correct normal.
- Ray missing all voxels returns `None`.
- Ray starting inside a solid voxel returns that voxel immediately.
- Ray crossing chunk boundaries finds voxels in adjacent chunks.

### 2. World mutation by global position

**Files:** `src/world.rs`

Add methods to `World` for getting/setting voxels by world-space integer coordinates (not chunk-relative):

```rust
impl World {
    pub fn get_mut(&mut self, pos: &IVec3) -> Option<&mut Chunk> { ... }

    /// Convert world voxel coords to (chunk_pos, local_pos).
    pub fn world_to_chunk(voxel_pos: IVec3) -> (IVec3, [usize; 3]) { ... }

    /// Set a voxel at global coordinates. Returns false if the chunk is not loaded.
    pub fn set_voxel(&mut self, voxel_pos: IVec3, value: u8) -> bool { ... }

    /// Get a voxel at global coordinates. Returns 0 if the chunk is not loaded.
    pub fn get_voxel(&self, voxel_pos: IVec3) -> u8 { ... }
}
```

`world_to_chunk` should use Euclidean division (`div_euclid` / `rem_euclid`) so negative coordinates map correctly. `set_voxel` calls `get_mut`, sets the voxel, and marks the world dirty.

Tests:
- `world_to_chunk` with positive coords (e.g. `(17, 5, 33)` -> chunk `(1,0,2)`, local `(1,5,1)`).
- `world_to_chunk` with negative coords (e.g. `(-1, 0, 0)` -> chunk `(-1,0,0)`, local `(15,0,0)`).
- `set_voxel` on a loaded chunk succeeds and marks dirty.
- `set_voxel` on an unloaded chunk returns false.
- `get_voxel` on an unloaded chunk returns 0.

### 3. Active voxel type selection and state

**Files:** `src/main.rs`

Add state and input handling:

- Add `active_voxel_type: u8` field to `App`, default `1` (grass).
- Define a mapping constant: `VOXEL_TYPE_NAMES: &[&str] = &["", "grass", "dirt", "stone", "brick"]` (index 0 unused, IDs 1-4 match the user-facing numbers 1-4 but note that the user's desired mapping is 1=grass, 2=dirt, 3=stone, 4=brick, while the internal voxel IDs from `TILE_DEFS` are 1=stone, 2=dirt, 3=grass, 4=brick). Create a mapping array `VOXEL_KEY_TO_ID: [u8; 5] = [0, 3, 2, 1, 4]` so key 1 -> voxel ID 3 (grass), key 2 -> voxel ID 2 (dirt), key 3 -> voxel ID 1 (stone), key 4 -> voxel ID 4 (brick).
- In `window_event`, handle `KeyCode::Digit1` through `Digit4` on press: set `active_voxel_type = VOXEL_KEY_TO_ID[digit]`.

No tests needed (pure input wiring).

### 4. Crosshair overlay texture

**Files:** `src/main.rs`

Generate a small procedural crosshair texture at startup (e.g. 32x32 pixels):
- Transparent background (alpha 0).
- A thin cross shape (2px wide arms, ~12px long) centered, drawn in white with ~60% alpha (Rgba(255, 255, 255, 153)).
- Center 2x2 pixels slightly brighter or fully opaque.

Create a bind group for this texture using `overlay_renderer.create_texture()`. Store it as `crosshair_bind_group: Option<wgpu::BindGroup>` on `App`.

In the render loop, draw the crosshair centered on screen using `draw_list.rect()` with the crosshair texture UV (full 0..1 range), at position `(screen_width/2 - 16, screen_height/2 - 16)` with size 32x32.

This requires rendering the overlay pass twice (once with font texture, once with crosshair texture) since the overlay currently supports only one texture per draw call. Alternatively, both the crosshair and the active voxel preview can share a separate draw list + render pass.

Add the crosshair texture bytes to the GPU memory estimate display (32 * 32 * 4 = 4096 bytes).

### 5. Active voxel HUD display

**Files:** `src/main.rs`, `src/voxel_textures.rs`

Display the active voxel type in the lower-left corner of the screen:

1. **Text label:** Use the existing font to draw the voxel type name (e.g. "Active: grass") at a position like `(10, screen_height - line_height - 50)`.

2. **Texture preview:** Show a small square (e.g. 48x48 pixels) of the active voxel's texture next to the text. This requires access to the voxel texture atlas UV rects.

Since the voxel atlas is a separate texture from the font atlas, and the overlay renderer only supports one texture per render pass, we need to either:
- (a) Create a combined overlay atlas that includes both font glyphs and voxel tile previews, or
- (b) Do a second overlay render pass with the voxel atlas texture.

Approach (b) is simpler: add a second `DrawList` for voxel-atlas-textured quads, create a bind group for the voxel atlas as an overlay texture, and call `render_overlay` a second time.

To support this:
- In `voxel_textures.rs`, make `VoxelTextureAtlas` publicly expose the atlas texture so it can be used for overlay rendering too (it already does via `texture()`).
- Store `voxel_atlas_overlay_bind_group: Option<wgpu::BindGroup>` on `App`, created during `try_resume` after building the voxel atlas.
- Store the `VoxelTextureAtlas` on `App` (or at least its `uv_map`) so we can look up UV rects per voxel ID at draw time.
- Use a second `DrawList` (`hud_draw_list`) for the voxel preview quad. Draw a rect using the active voxel's UV rect from the `uv_map`.

### 6. Voxel placement (left click)

**Files:** `src/main.rs`

When the cursor is grabbed and the user left-clicks:
1. Cast a ray from the camera using `world.raycast(camera.position, camera.forward(), 50.0)`.
2. If a hit is found, compute the adjacent voxel position: `hit_world_pos + hit.normal` where `hit_world_pos = hit.chunk_pos * 16 + IVec3::from(hit.local_pos)`.
3. Call `world.set_voxel(adjacent_pos, active_voxel_type)`. This returns false if the target chunk is unloaded (do nothing in that case).
4. The existing dirty-check logic in `RedrawRequested` will re-upload world data on the next frame.

Update the left-click handler: currently left click only grabs the cursor. Change to: if cursor is already grabbed, do voxel placement. If not grabbed, grab cursor (existing behavior).

### 7. Voxel removal (right click)

**Files:** `src/main.rs`

When the cursor is grabbed and the user right-clicks:
1. Cast a ray from the camera using `world.raycast(camera.position, camera.forward(), 50.0)`.
2. If a hit is found, compute the world position of the hit voxel: `hit.chunk_pos * 16 + IVec3::from(hit.local_pos)`.
3. Call `world.set_voxel(hit_pos, 0)` to replace with air.

Add a `WindowEvent::MouseInput` handler for `MouseButton::Right` with `ElementState::Pressed`, gated on `self.cursor_grabbed`.

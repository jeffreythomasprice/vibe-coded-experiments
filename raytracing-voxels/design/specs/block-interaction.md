# Improved Block Placing and Breaking Interface

**Summary:** Replace current left/right click voxel editing with a Minecraft-style interface: wireframe placement preview, right-click to place, hold left-click to break with a visual progress indicator.
**Depends on:** voxel-editing, tile-picker

---

## Steps

### 1. Swap mouse button bindings (right-click place, left-click break stub)

**Files:** `src/main.rs`

Swap the current mouse button behaviors:

- **Right-click** (`MouseButton::Right`): place the active voxel (move the existing left-click placement logic here). When `active_voxel_type` is `Some(id)`, raycast and place at `hit_world + hit.normal`. When `None`, do nothing.
- **Left-click** (`MouseButton::Left`): for now, keep the existing instant-remove behavior as a placeholder (will be replaced with hold-to-break in step 4). Still grab cursor if not grabbed.

No new state needed yet. This is a pure rebinding of existing logic.

### 2. Add highlight block uniform to the voxel shader

**Files:** `src/voxels.wgsl`, `src/voxel_renderer.rs`, `src/main.rs`

Pass the placement preview position to the GPU so the shader can render a wireframe outline.

**Shader changes (`voxels.wgsl`):**

Add a new uniform struct for interaction state in a new bind group (group 3):

```wgsl
struct InteractionState {
    highlight_pos: vec3<f32>,
    highlight_active: u32,  // 0 = no highlight, 1 = show wireframe
    break_pos: vec3<f32>,
    break_progress: f32,    // 0.0 to 1.0, used in step 5
};

@group(3) @binding(0) var<uniform> interaction: InteractionState;
```

**Renderer changes (`voxel_renderer.rs`):**

- Define a `#[repr(C)] #[derive(Pod, Zeroable)] pub struct GpuInteractionState` matching the shader layout (32 bytes: 3 floats + 1 u32 + 3 floats + 1 float).
- Create a uniform buffer `interaction_buffer` (32 bytes) and bind group for group 3.
- Add `pub fn upload_interaction(&self, state: &GpuInteractionState)` that writes to the buffer.
- Set the bind group in `render_voxels`.
- Update the pipeline layout to include the new bind group layout.

**Main loop (`src/main.rs`):**

Each frame in `RedrawRequested`, before rendering:
1. Perform a raycast from the camera: `world.raycast(camera.position, camera.forward(), 50.0)`.
2. If a hit is found and `active_voxel_type.is_some()`, compute placement position (`hit_world + hit.normal`) and set `highlight_pos` to the placement voxel's world position (as `Vec3`), `highlight_active = 1`.
3. If no hit or no active type, set `highlight_active = 0`.
4. Upload the interaction state to the GPU.

### 3. Render wireframe outline in the shader

**Files:** `src/voxels.wgsl`

After the main BVH traversal and hit determination in `fs_main`, add wireframe rendering:

1. If `interaction.highlight_active != 0`, intersect the ray with the AABB `[highlight_pos, highlight_pos + vec3(1)]`.
2. If the ray hits this box AND the box's entry `t` is less than or equal to `closest_t` (i.e., the wireframe block is visible, not occluded):
   - Compute the hit point on the face.
   - Compute the 2D face coordinates (0..1 on each axis of the face).
   - Check if the face coordinates are within a small margin of the edges (e.g., `< 0.04` or `> 0.96` on either face axis).
   - If on an edge, blend a wireframe color (e.g., white at ~60% alpha) over the current pixel color (whether it's a voxel hit or sky).

This gives a clean wireframe cube outline at the placement position. The wireframe should be visible even against the sky (no voxel behind it), so check the box intersection regardless of whether a voxel was hit.

Helper function:

```wgsl
fn is_wireframe_edge(face_uv: vec2<f32>, thickness: f32) -> bool {
    return face_uv.x < thickness || face_uv.x > (1.0 - thickness)
        || face_uv.y < thickness || face_uv.y > (1.0 - thickness);
}
```

### 4. Hold-to-break mechanic with state tracking

**Files:** `src/main.rs`

Add state for tracking a block being broken:

```rust
struct BreakState {
    world_pos: IVec3,       // the voxel being broken
    elapsed: f32,           // seconds held so far
}

// On App:
break_state: Option<BreakState>,
```

Constants:

```rust
const BREAK_TIME: f32 = 0.4; // seconds to hold before block breaks
```

**Input handling:**

- `MouseButton::Left` + `Pressed`: if cursor is grabbed, start a raycast. If a non-air voxel is hit, begin breaking: set `break_state = Some(BreakState { world_pos: hit_world, elapsed: 0.0 })`.
- `MouseButton::Left` + `Released`: cancel breaking: set `break_state = None`.

**Frame update (in `RedrawRequested`, before rendering):**

If `break_state.is_some()`:
1. Re-raycast from the camera. Check if the ray still hits the same voxel (`hit_world == break_state.world_pos`).
2. If still aiming at the same block, increment `elapsed += dt`.
3. If `elapsed >= BREAK_TIME`, remove the voxel (`world.set_voxel(world_pos, 0)`) and set `break_state = None`.
4. If aiming at a different block or no hit, cancel: set `break_state = None`.

Upload `break_pos` and `break_progress` (elapsed / BREAK_TIME, clamped 0..1) to the interaction uniform. Set `break_progress = 0.0` when not breaking.

### 5. Break progress visual indicator in the shader

**Files:** `src/voxels.wgsl`

When `interaction.break_progress > 0.0`, overlay a visual indicator on the voxel being broken:

After the main BVH traversal hit:
1. Compute the world position of the hit voxel and check if it matches `interaction.break_pos` (compare floor of world coordinates).
2. If it matches, overlay a crack/progress pattern on the face.

**Progress pattern design:**

Use a grid-based darkening pattern that grows with progress. Divide the face into an N×N grid (e.g., 4×4). As progress increases from 0 to 1, darken more cells of the grid:

```wgsl
fn break_overlay(face_uv: vec2<f32>, progress: f32) -> f32 {
    // Returns a darkening factor (0.0 = no change, 1.0 = fully dark)
    let grid = vec2<u32>(u32(face_uv.x * 4.0), u32(face_uv.y * 4.0));
    let cell_id = grid.x + grid.y * 4u;
    // Use a scrambled order so cells darken pseudo-randomly
    let scramble = array<u32, 16>(0u,10u,5u,15u, 8u,2u,13u,7u, 12u,6u,3u,9u, 4u,14u,1u,11u);
    let threshold = u32(progress * 16.0);
    if scramble[cell_id] < threshold {
        return 0.5; // darken by 50%
    }
    return 0.0;
}
```

Apply the darkening to the hit color: `result_color.rgb *= (1.0 - overlay_factor)`.

Also draw a subtle wireframe outline around the block being broken (similar to the placement wireframe but in a different color, e.g., red-tinted) so the player can see which block is targeted.

### 6. Cancel break on cursor release or escape

**Files:** `src/main.rs`

Ensure `break_state` is properly cancelled in edge cases:

- When cursor is released (escape key): set `break_state = None`.
- When `release_cursor()` is called: set `break_state = None`.
- The `MouseButton::Left` `Released` handler from step 4 already covers the normal case.

This step is about auditing all paths that should cancel an in-progress break to avoid stale state.

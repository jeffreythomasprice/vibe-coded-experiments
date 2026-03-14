# Water

**Summary:** Add water blocks that fill low-lying terrain, render as semi-transparent surfaces slightly below voxel top, apply underwater physics (slow sinking, swimming with space), and tint the screen blue when submerged.
**Depends on:** None (builds on existing terrain generation, raymarching, player physics, and overlay)

---

## Design Decisions

### Water level
Terrain surface heights range from ~24 (deepest valleys) to ~200 (mountain peaks), with `BASE_HEIGHT = 40` and `MIN_AMPLITUDE = 16`. A water level of **36** fills the lowest valleys while keeping most terrain dry, producing natural-looking lakes.

### Water voxel ID
Water uses voxel ID **7** (next after leaves = 6). Water voxels are non-solid for collision purposes.

### Rendering approach
Water is rendered in the raymarcher with semi-transparent blending. When a ray hits a water voxel, it renders a surface plane at `y_top - 0.15` (slightly below voxel top), records the water color with partial alpha, then continues marching. The final color blends the water tint over whatever is behind it. This avoids needing a separate render pass or blend state changes.

### Underwater detection
The shader receives an `is_submerged` flag via an existing or new uniform. When set, a blue color filter is applied to the final fragment color. The CPU detects submersion by checking if the voxel at the player's eye position is water.

### Physics
Water voxels are excluded from AABB collision (`get_voxel` returns 0-like for collision purposes, or a separate `is_solid` check). While in water: gravity is replaced with slow sinking, space key swims up at a slow rate, horizontal movement is slightly slower.

---

## Steps

### 1. Add water voxel constant and texture

**Files:** `src/voxel_textures.rs`, `src/terrain.rs`

Add `const WATER: u8 = 7` to `terrain.rs` (alongside existing STONE, GRASS, etc.). In `voxel_textures.rs`, add a `TileDef` entry for water (ID 7) with a procedural texture generator that produces a blue semi-transparent texture. The texture should use noise-based patterns to simulate water surface ripples with colors in the blue range (e.g., `Rgba::new(30, 80, 180, 140)` with variations). The alpha should be ~140 (semi-transparent) rather than 255.

Add the water tile to `TILE_DEFS` array. Add a test `water_correct_size` and `water_is_semitransparent` (verify alpha values are between 100-200, not fully opaque).

### 2. Fill water in terrain generation

**Files:** `src/terrain.rs`

Add `const WATER_LEVEL: i32 = 36` to `terrain.rs` and make it `pub` so other modules can reference it.

In `generate_chunk`, after the terrain + cave pass but before the tree pass, add a water fill pass: for each column (lx, lz), for each local y where the world y is <= `WATER_LEVEL` and the voxel is currently air (0), set it to `WATER`. This fills empty space below the water level — caves below water level will flood, valleys will have lakes.

Don't place trees on water: in `has_tree_at`, add a check that `surface_height >= WATER_LEVEL` (trees shouldn't grow from underwater surfaces). Actually, trees already only grow on grass, and grass only appears at the surface, so if the surface is below water level the surface voxel will still be grass but the space above will be water. Add a guard: skip tree placement if `surface_height < WATER_LEVEL`.

Add tests:
- `water_fills_below_level` — generate chunks at y=2 (world y 32-47 includes water level 36) and verify water voxels exist where expected
- `water_does_not_fill_solid` — verify no water voxels replace solid terrain
- `no_trees_underwater` — verify no wood/leaves below water level

### 3. Water rendering in shader (semi-transparent surface)

**Files:** `src/voxels.wgsl`

Modify `march_chunk` to handle water specially. Define water voxel ID as a constant `const WATER_ID: u32 = 7u;`.

When the DDA hits a water voxel (`v == WATER_ID`):
1. Compute the hit position as a plane at `voxel_y_top - 0.15` (only for rays hitting from above via the Y face; for side faces, use the normal face intersection).
2. Sample the water texture from the atlas for the surface color.
3. Instead of returning immediately, store the water color and t value, then continue the DDA to find what's behind the water.
4. After the march completes, if water was hit, blend: `final = water_color * water_alpha + behind_color * (1 - water_alpha)`.

To implement this efficiently, add fields to `MarchResult`: `water_hit: bool`, `water_color: vec4<f32>`, `water_t: f32`. In `fs_main`, after getting the march result, if `water_hit` is true, blend the water color over the result (or sky if nothing was behind it).

For the water surface offset: when the ray enters a water voxel from the top face (normal.y < 0 means ray going down hitting top), adjust the hit t so the surface appears at `voxel_y + 0.85` instead of `voxel_y + 1.0`.

### 4. Exclude water from player collision

**Files:** `src/player.rs`, `src/world.rs`

Add a method `World::is_solid_voxel(voxel_pos: IVec3) -> bool` that returns true if the voxel is non-zero AND not water (ID 7). Use `pub const WATER_VOXEL_ID: u8 = 7` in `world.rs` (or import from terrain).

In `Player::aabb_collides`, change the check from `world.get_voxel(voxel_pos) != 0` to `world.is_solid_voxel(voxel_pos)`. This allows the player to walk into water.

Also update `World::raycast` to skip water voxels for block interaction (the player shouldn't be able to "break" or "place" against water). Add a condition: when a non-mesh, non-zero voxel is hit, also check it's not water before returning a hit.

Add tests:
- `aabb_water_no_collision` — place a water voxel and verify `aabb_collides` returns false
- `raycast_skips_water` — cast a ray through a water voxel to a solid block behind, verify the solid block is hit

### 5. Player water physics

**Files:** `src/player.rs`

Add `pub in_water: bool` field to `Player` struct (default false).

Add a method `check_water_state(&mut self, world: &World)` that checks if any part of the player's AABB overlaps a water voxel. Set `self.in_water` based on this.

In `tick_physics`, call `check_water_state` at the start. When `in_water`:
- Replace gravity with a slow sink: `self.velocity.y -= WATER_SINK_SPEED * dt` (where `WATER_SINK_SPEED ≈ 3.0`)
- Clamp downward velocity to `MAX_WATER_SINK ≈ 2.0`
- If `input.up` (space), apply upward swim velocity: `self.velocity.y = SWIM_UP_SPEED` (≈ 3.5)
- Reduce horizontal move speed to `MOVE_SPEED * 0.6`
- Disable jumping (no `JUMP_VELOCITY` in water, space is swim-up instead)

Add tests:
- `player_sinks_in_water` — player in water column sinks slowly
- `player_swims_up` — player in water with space pressed moves upward
- `player_water_reduces_speed` — horizontal speed is reduced in water

### 6. Underwater screen tint

**Files:** `src/voxels.wgsl`, `src/voxel_renderer.rs`, `src/main.rs`

Add an `is_submerged: u32` field to the `InteractionState` struct in both the WGSL shader and `GpuInteractionState` in `voxel_renderer.rs`. This reuses the existing bind group 3 — just extend the struct (add after `break_progress`, respecting alignment).

Update `GpuInteractionState` to include `pub is_submerged: u32` and add a padding field if needed for alignment.

In `main.rs`, when building the interaction state each frame, check if the voxel at the player's eye position is water. If so, set `is_submerged = 1`.

In the shader's `fs_main`, after computing `final_color` (after all lighting, break overlay, and wireframe), if `interaction.is_submerged != 0u`, apply a blue tint:
```
let underwater_tint = vec3<f32>(0.1, 0.3, 0.7);
final_color = vec4<f32>(mix(final_color.rgb, underwater_tint, 0.4), 1.0);
```

Also reduce the view distance when underwater by adding fog: blend toward the tint color based on distance.

### 7. Update main.rs voxel type arrays and tile picker

**Files:** `src/main.rs`

Add water to `VOXEL_TYPE_NAMES`: extend the array to include `"water"` at index 7.

Optionally add water to the tile picker so the player can place water blocks. Add key 7 mapping to `VOXEL_KEY_TO_ID` and extend `TILE_PICKER_TILE_IDS`. If not desired, at minimum ensure the arrays don't panic if a water voxel ID is encountered.

## Verification

```
cargo test && cargo clippy && cargo build
```
Then run the app:
- Lakes should be visible in low-lying terrain areas
- Water surface should appear slightly below voxel top and be semi-transparent (see terrain through it)
- Walking into water should cause slow sinking
- Holding space in water should swim upward
- When eyes are below water level, screen should have blue tint
- Water blocks should not be breakable via normal interaction

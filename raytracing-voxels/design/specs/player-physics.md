# Player Physics

**Summary:** Add gravity, AABB collision detection, wall sliding, 1-voxel step-up, and jumping to a new `Player` struct that owns position/velocity and drives the camera. Includes a fly-mode toggle (F1) to switch between physics and the existing free-fly camera.
**Depends on:** None (uses existing `World::get_voxel`)

---

## Steps

### 1. Create Player struct with fly-mode passthrough

**Files:** `src/player.rs`, `src/main.rs`

Create `src/player.rs` with:

```rust
pub const PLAYER_WIDTH: f32 = 0.6;
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const EYE_HEIGHT: f32 = 1.62;
pub const MOVE_SPEED: f32 = 7.5;

pub struct Player {
    pub feet_pos: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub fly_mode: bool,
}
```

Methods:
- `new(eye_pos: Vec3) -> Self` — derives `feet_pos` from eye position: `eye_pos - Vec3(0, EYE_HEIGHT, 0)`. Starts in fly mode so existing behavior is preserved.
- `eye_position() -> Vec3` — returns `feet_pos + Vec3(0, EYE_HEIGHT, 0)`.
- `toggle_fly_mode()` — flips `fly_mode`. When switching from fly to physics, zero out velocity.
- `tick(&mut self, dt: f32, input: &InputState, yaw: f32, world: &World)` — in this step, implement **fly mode only**: compute wish direction from WASD using `yaw` (same sin/cos math as `Camera::move_forward`), apply vertical from space/shift, move `feet_pos` directly by `wish_dir * MOVE_SPEED * dt`. No collision in fly mode.

In `main.rs`:
- Add `mod player;` and `player: Player` field to `App`.
- Initialize: `Player::new(camera.position)`.
- Move `MOVE_SPEED` constant to `player.rs` (remove from `main.rs`).
- In `RedrawRequested`, replace the 6 `camera.move_*` calls with: `self.player.tick(dt, &self.input, self.camera.yaw, &self.world);` then `self.camera.position = self.player.eye_position();`.
- Add F1 key handler in the `KeyboardInput` match: on press, call `self.player.toggle_fly_mode()`.
- `InputState` needs to be accessible from `player.rs` — either move it to `player.rs` or make it `pub` in `main.rs` and import. Moving to `player.rs` is cleaner.

Tests:
- `Player::new` computes correct `feet_pos` from eye position.
- `eye_position()` round-trips: `Player::new(eye).eye_position() ≈ eye`.
- Fly-mode tick moves position by expected amount for forward/strafe/vertical input.

### 2. Add `is_chunk_loaded` to World and AABB collision helper

**Files:** `src/world.rs`, `src/player.rs`

Add to `World`:
```rust
pub fn is_chunk_loaded(&self, chunk_pos: &IVec3) -> bool {
    self.chunks.contains_key(chunk_pos)
}
```

In `player.rs`, add a collision query function:

```rust
fn aabb_collides(feet_pos: Vec3, world: &World) -> bool
```

Computes AABB from `feet_pos`:
- `min = feet_pos - Vec3(PLAYER_WIDTH/2, 0.0, PLAYER_WIDTH/2)`
- `max = feet_pos + Vec3(PLAYER_WIDTH/2, PLAYER_HEIGHT, PLAYER_WIDTH/2)`

Iterates integer voxel coordinates from `floor(min.x)..=floor(max.x - EPSILON)` (and same for Y, Z) where `EPSILON = 1e-4`. For each voxel position:
1. Compute chunk position via `World::world_to_chunk`.
2. If chunk is **not loaded**, treat as solid (return true).
3. If `world.get_voxel(pos) != 0`, return true.

Returns false if no collisions found.

Tests:
- AABB in open air returns false.
- AABB overlapping a solid voxel returns true.
- AABB in unloaded chunk returns true (treated as solid).
- AABB exactly on boundary (e.g., feet_pos.y = 5.0 standing on voxel at y=4) does NOT collide with the block below (epsilon prevents it).

### 3. Add gravity and ground detection

**Files:** `src/player.rs`

Add constants:
```rust
pub const GRAVITY: f32 = 28.0;
pub const MAX_FALL_SPEED: f32 = 50.0;
```

In `Player::tick`, when `fly_mode` is false:
1. Compute horizontal wish direction from WASD + yaw. Set `velocity.x` and `velocity.z` directly (instant, no acceleration).
2. Apply gravity: `velocity.y -= GRAVITY * dt`. Clamp to `-MAX_FALL_SPEED`.
3. **Resolve Y axis first:**
   - Compute `new_feet_y = feet_pos.y + velocity.y * dt`.
   - Check `aabb_collides` at the new Y position (keep X/Z unchanged).
   - If colliding and moving **down** (`velocity.y < 0`): snap `feet_pos.y` to `floor(min.y) + 1.0` (top of the block below), set `velocity.y = 0`, set `on_ground = true`.
   - If colliding and moving **up**: snap `feet_pos.y` to `ceil(max.y) - PLAYER_HEIGHT` (bottom of the block above), set `velocity.y = 0`.
   - If not colliding: accept new Y, set `on_ground = false`.

The snapping logic for downward collision: `min.y` at the new position is `new_feet_y`. The solid block's top face is at `floor(new_feet_y) + 1`. So snap `feet_pos.y = floor(new_feet_y) + 1.0`. For upward: `max.y` at new position is `new_feet_y + PLAYER_HEIGHT`. The solid block's bottom face is at `ceil(new_feet_y + PLAYER_HEIGHT - EPSILON)`. Snap `feet_pos.y` so `max.y` aligns: `feet_pos.y = ceil(new_feet_y + PLAYER_HEIGHT - EPSILON) - PLAYER_HEIGHT`.

Tests:
- Player above ground falls over multiple ticks and lands on_ground.
- Player standing on ground stays put (velocity.y stays 0, on_ground stays true).
- Player hitting ceiling from below stops and velocity.y zeroes.

### 4. Add jumping

**Files:** `src/player.rs`, `src/main.rs`

Add constant:
```rust
pub const JUMP_VELOCITY: f32 = 10.6; // sqrt(2 * GRAVITY * 2.0) — reaches ~2 voxels
```

In `Player::tick` (physics mode), after computing wish velocity but before applying gravity:
- If `on_ground` and `input.up` (space): set `velocity.y = JUMP_VELOCITY`.

Note: `input.up` (space) serves dual purpose — jump in physics mode, fly up in fly mode. This is already handled because fly mode and physics mode are separate branches in `tick()`.

Tests:
- Player on ground with jump input gets `velocity.y = JUMP_VELOCITY`.
- Player in air with jump input does NOT change velocity (no air jump).
- After jumping, player reaches approximately 2 voxels above starting position (within tolerance, since discrete tick resolution causes slight variation).

### 5. Add horizontal collision and wall sliding

**Files:** `src/player.rs`

After Y-axis resolution, resolve X and Z axes independently:

**X-axis:**
1. `new_feet_x = feet_pos.x + velocity.x * dt`
2. Check `aabb_collides` at `(new_feet_x, feet_pos.y, feet_pos.z)`.
3. If colliding: snap X. If moving in +X (`velocity.x > 0`): find the leftmost solid voxel's X and snap `feet_pos.x` so `max.x` doesn't overlap, i.e., `feet_pos.x = floor(new_feet_x + PLAYER_WIDTH/2) - PLAYER_WIDTH/2`. If moving in -X: `feet_pos.x = ceil(new_feet_x - PLAYER_WIDTH/2) + PLAYER_WIDTH/2`. Zero `velocity.x`.
4. If not colliding: accept `new_feet_x`.

**Z-axis:** Same logic, substituting Z for X.

Wall sliding emerges naturally: if diagonal movement into a wall blocks X but not Z, the Z component still applies.

The snap formulas assume the collision is with the nearest voxel boundary. Using `floor()` for positive movement and `ceil()` for negative movement finds the face of the blocking voxel.

Tests:
- Walking into a wall along X: X stops, Z continues (wall slide).
- Walking into a wall along Z: Z stops, X continues.
- Walking diagonally into a corner: both axes stop.
- Walking parallel to a wall (not colliding): no sliding interference.

### 6. Add step-up logic

**Files:** `src/player.rs`

Add constant:
```rust
pub const STEP_HEIGHT: f32 = 1.0;
```

Modify the X-axis and Z-axis collision resolution from step 5. When a horizontal collision is detected **and** `on_ground` is true:

1. Tentatively compute `stepped_feet_pos.y = feet_pos.y + STEP_HEIGHT`.
2. Check that the player can exist at the stepped-up position (no collision at `(feet_pos.x, stepped_feet_pos.y, feet_pos.z)` — headroom check).
3. Check that the horizontal move succeeds at the stepped-up height: no collision at `(new_feet_x, stepped_feet_pos.y, feet_pos.z)` for X-axis.
4. Find the actual ground at the stepped+moved position: scan downward from `stepped_feet_pos.y` to `feet_pos.y` checking `aabb_collides`. The highest non-colliding Y is the landing spot.
5. If all checks pass, accept the stepped position (new X + landed Y). Otherwise, fall back to the normal collision snap.

This allows walking up stairs (1-voxel height differences) smoothly while preventing step-up when there's no headroom or no ground at the destination.

Tests:
- Player walks into a 1-voxel step: position rises by 1 voxel, horizontal movement continues.
- Player walks into a 2-voxel wall: no step-up (too tall), normal wall collision.
- Player airborne walks into a 1-voxel step: no step-up (must be on_ground).
- Player walks into a 1-voxel step with low ceiling (< PLAYER_HEIGHT + STEP_HEIGHT headroom): no step-up.

### 7. Add step-down snapping

**Files:** `src/player.rs`

After all axis resolution, if `on_ground` was true at the start of the tick but `on_ground` is now false (player walked off a small ledge), attempt step-down:

1. Probe downward up to `STEP_HEIGHT` below current `feet_pos.y`.
2. Check `aabb_collides` at `(feet_pos.x, feet_pos.y - probe, feet_pos.z)` for increasing probe values.
3. If ground is found within `STEP_HEIGHT`, snap `feet_pos.y` down to land on it and set `on_ground = true`.

This prevents the bobbing effect of constant micro-falls when walking on slightly uneven terrain or descending 1-voxel steps.

Tests:
- Player walks off a 1-voxel ledge: snaps down instead of brief free-fall.
- Player walks off a 2-voxel ledge: enters free-fall (too high for step-down).
- Step-down only activates when `was_on_ground` is true (no snap-down during jumps).

### 8. Add HUD indicator for physics mode

**Files:** `src/main.rs`

In the overlay text rendering section of `RedrawRequested`, add a line showing the current mode:
- "Mode: FLY" or "Mode: WALK"
- Optionally show velocity or on_ground state for debugging.

Display near the existing HUD text (below the active voxel indicator or in a corner).

Tests: None (visual only).

# Async Chunk Loading

**Summary:** Migrate ChunkManager to run chunk generation on a background tokio task with a time-budgeted update loop, replacing the fixed-interval single-chunk-per-tick approach. The main thread receives completed chunks via a channel and applies them to the World for GPU upload.
**Depends on:** chunk-manager

---

## Steps

### 1.1 Add tokio dependency

**Files:** `Cargo.toml`

Add `tokio = { version = "1", features = ["rt", "sync", "time"] }` to `[dependencies]`. The `rt` feature provides the runtime, `sync` provides `mpsc` channels, and `time` provides `Instant`/`sleep` for the background task's timing.

### 1.2 Refactor ChunkManager API to use `updates_per_second`

**Files:** `src/chunk_manager.rs`, `src/main.rs`

Replace the `update_interval: Duration` field with `updates_per_second: f32`. The constructor becomes:

```rust
pub fn new(load_radius: i32, max_loaded: usize, updates_per_second: f32) -> Self
```

Internally compute the time budget per cycle as `Duration::from_secs_f32(1.0 / updates_per_second)`. Store this as `cycle_budget: Duration`.

Update `App::new()` call site: replace `Duration::from_millis(200)` with `5.0` (5 updates per second, equivalent to the old 200ms interval).

Keep all existing tests passing by updating their constructor calls. Tests that used `Duration::ZERO` should use `f32::INFINITY` (infinite updates per second = zero budget doesn't make sense for time-budgeted loading, so these tests will be restructured in step 1.3).

Tests:
- Existing unit tests still compile and pass with the new constructor signature.

### 1.3 Implement time-budgeted `update` — load multiple chunks per cycle

**Files:** `src/chunk_manager.rs`

Change the `update` method to load as many chunks as possible within the time budget instead of exactly one:

```rust
pub fn update(&mut self, camera_pos: Vec3, world: &mut World)
```

Logic:
1. Check if `cycle_budget` has elapsed since `last_update`. If not, return.
2. Reset `last_update = Instant::now()`.
3. Compute `camera_chunk` and `desired` set once.
4. Loop: call `load_one(camera_pos, world)` repeatedly until either:
   - `load_one` returns `false` (nothing left to load/swap), or
   - `last_update.elapsed() >= cycle_budget` (time budget exhausted).

The existing `load_one` method stays as-is — it handles the single-chunk load/swap logic.

For tests that need deterministic behavior, add a `load_batch(camera_pos, world, max_ops: usize)` method that loads up to `max_ops` chunks without any time check (used by tests and `load_initial`). Rewrite `load_initial` to delegate to `load_batch`.

Tests:
- `load_batch` with max_ops=27, radius 1: loads all 27 chunks.
- `load_batch` with max_ops=5, radius 1: loads exactly 5 nearest chunks.
- `update` with a generous budget (e.g., 1 second) and radius 1: loads all 27 chunks in a single call.
- Camera move test: after `load_batch` fills world, calling `load_batch` again at new position swaps all distant chunks.

### 1.4 Define chunk loading messages and channel types

**Files:** `src/chunk_manager.rs`

Define the message types for communication between the main thread and the background task:

```rust
pub enum ChunkCommand {
    UpdateCamera(Vec3),
    Shutdown,
}

pub struct ChunkResult {
    pub pos: IVec3,
    pub chunk: Chunk,
    pub evict: Option<IVec3>,  // chunk position to remove, if swapping
}
```

`ChunkCommand::UpdateCamera` tells the background task where the camera is. `ChunkCommand::Shutdown` tells it to exit.

`ChunkResult` carries a generated chunk and its target position, plus optionally a position to evict (for swap operations when at max capacity).

No tests needed — these are plain data types.

### 1.5 Extract chunk loading logic into a pure function

**Files:** `src/chunk_manager.rs`

Extract the decision-making logic from `load_one` into a pure function that doesn't need `&mut World`:

```rust
fn compute_next_load(
    camera_chunk: IVec3,
    loaded_positions: &HashSet<IVec3>,
    desired: &[IVec3],
    max_loaded: usize,
) -> Option<LoadAction>

enum LoadAction {
    Load(IVec3),
    Swap { evict: IVec3, load: IVec3 },
}
```

This function contains the same logic as `load_one` but operates on a set of positions rather than the World directly. This makes it usable from the background task which only has a shadow copy of loaded positions.

Rewrite `load_one` to delegate to `compute_next_load` + apply the result to World.

Tests:
- Empty loaded set, desired has chunks: returns `Load(nearest)`.
- Loaded set == desired set: returns `None`.
- At capacity, farthest loaded is farther than nearest unloaded: returns `Swap { evict, load }`.
- At capacity, all loaded are nearer than unloaded: returns `None`.

### 1.6 Implement the background chunk loading task

**Files:** `src/chunk_manager.rs`

Create an async function that runs on a tokio task:

```rust
async fn chunk_loader_task(
    load_radius: i32,
    max_loaded: usize,
    cycle_budget: Duration,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<ChunkCommand>,
    result_tx: tokio::sync::mpsc::UnboundedSender<ChunkResult>,
)
```

Logic:
1. Maintain local state: `camera_pos: Vec3`, `loaded_positions: HashSet<IVec3>`.
2. Main loop: drain all pending `ChunkCommand`s from `cmd_rx` (non-blocking `try_recv`). Apply camera position updates. On `Shutdown`, return.
3. Compute `camera_chunk` and `desired` set.
4. Time-budgeted inner loop: call `compute_next_load` repeatedly. For each `LoadAction`:
   - Generate the chunk via `generate_test_chunk()`.
   - Update local `loaded_positions` (add new, remove evicted).
   - Send `ChunkResult` via `result_tx`.
   - Check elapsed time against `cycle_budget`; break if exceeded.
5. If nothing to load, sleep until next cycle (`tokio::time::sleep`).
6. After inner loop, sleep for remaining cycle time if any.

The task runs continuously, loading chunks whenever the camera moves into a new region.

Tests:
- Unit test the loop body logic by calling `compute_next_load` directly (tested in 1.5).
- Integration test deferred to step 1.8.

### 1.7 Refactor ChunkManager into a handle for the background task

**Files:** `src/chunk_manager.rs`

Restructure `ChunkManager` to be a handle that owns the channel endpoints and communicates with the background task:

```rust
pub struct ChunkManager {
    cmd_tx: tokio::sync::mpsc::UnboundedSender<ChunkCommand>,
    result_rx: tokio::sync::mpsc::UnboundedReceiver<ChunkResult>,
    runtime: tokio::runtime::Runtime,
}
```

New constructor:

```rust
pub fn new(load_radius: i32, max_loaded: usize, updates_per_second: f32) -> Self
```

Creates a tokio `Runtime` (multi-thread, 1 worker), creates channels, spawns `chunk_loader_task`, returns the handle.

New methods:

```rust
pub fn update_camera(&self, camera_pos: Vec3)
```
Sends `ChunkCommand::UpdateCamera(camera_pos)` on the command channel.

```rust
pub fn drain_results(&mut self, world: &mut World)
```
Drains all pending `ChunkResult`s from the result channel using `try_recv()`. For each result:
- If `evict` is Some, call `world.remove(&evict)`.
- Call `world.insert(pos, chunk)`.

This replaces the old `update()` method. The main thread calls `update_camera` each frame, then `drain_results` to apply any chunks the background task has generated.

Implement `Drop` for `ChunkManager` to send `Shutdown` and drop the runtime.

### 1.8 Refactor `load_initial` to block on background task

**Files:** `src/chunk_manager.rs`

Replace the old synchronous `load_initial` with a method that waits for the background task to finish its initial load:

```rust
pub fn load_initial(&mut self, camera_pos: Vec3, world: &mut World)
```

Logic:
1. Send `UpdateCamera(camera_pos)`.
2. Block-wait on `result_rx` in a loop, applying each `ChunkResult` to the world, until either:
   - `max_loaded` chunks have been inserted, or
   - the channel yields no results for a short timeout (indicating the background task has loaded everything it can).

This ensures the world is populated before the first frame, just like the old synchronous version.

Tests:
- `load_initial` with radius 2, max_loaded 125: all 125 chunks loaded.
- `load_initial` with radius 2, max_loaded 50: exactly 50 chunks loaded.

### 1.9 Integrate async ChunkManager into App

**Files:** `src/main.rs`

Update `App` to use the new async `ChunkManager` API:

1. In `App::new()`:
   - Create `ChunkManager::new(5, 500, 5.0)`.
   - Call `chunk_manager.load_initial(cam_pos, &mut world)`.

2. In `RedrawRequested`:
   - Replace `self.chunk_manager.update(self.camera.position, &mut self.world)` with:
     ```rust
     self.chunk_manager.update_camera(self.camera.position);
     self.chunk_manager.drain_results(&mut self.world);
     ```
   - The existing dirty-check → GPU upload flow handles the rest automatically.

3. Remove `use std::time::Duration` if no longer needed (ChunkManager no longer takes a Duration).

The `ChunkManager` is dropped when `App` is dropped, which sends `Shutdown` to the background task.

### 1.10 Update and clean up tests

**Files:** `src/chunk_manager.rs`

Update all existing chunk_manager tests to work with the new architecture:

- Pure-logic tests (`camera_chunk_coord`, `chebyshev_distance`, `desired_chunks`, `compute_next_load`) remain synchronous unit tests.
- Integration tests that used the old synchronous `update()` method should be rewritten to either:
  - Test `compute_next_load` + `LoadAction` directly (preferred for determinism).
  - Use `load_initial` + `drain_results` for end-to-end tests that verify the async pipeline.
- Remove tests that are no longer applicable (e.g., tests that relied on the timer-gated single-chunk-per-update behavior).

Ensure `cargo test` passes and `cargo clippy` is clean.

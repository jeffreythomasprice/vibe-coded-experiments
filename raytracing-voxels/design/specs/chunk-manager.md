# Chunk Manager

**Summary:** Add a ChunkManager that dynamically loads/unloads chunks around the camera using a Chebyshev distance budget, replacing the static 3x3 grid with a radius-5 loading region and 500-chunk cap, with periodic updates decoupled from frame rate.
**Depends on:** multi-chunk-world

---

## Steps

### 1.1 Create `ChunkManager` struct with core fields

**Files:** `src/chunk_manager.rs`, `src/main.rs`

Create `src/chunk_manager.rs` with the `ChunkManager` struct:

```rust
pub struct ChunkManager {
    load_radius: i32,
    max_loaded: usize,
    update_interval: Duration,
    last_update: Instant,
}
```

Add a constructor:

```rust
pub fn new(load_radius: i32, max_loaded: usize, update_interval: Duration) -> Self
```

Initialize `last_update` to `Instant::now()`.

Add helper function:

```rust
fn camera_chunk_coord(camera_pos: Vec3) -> IVec3
```

Computes `(camera_pos / 16.0).floor().as_ivec3()`.

Add helper function:

```rust
fn chebyshev_distance(a: IVec3, b: IVec3) -> i32
```

Returns `(a - b).abs().max_element()`.

Add `mod chunk_manager;` to `src/main.rs`.

Tests:
- `camera_chunk_coord(Vec3::ZERO)` returns `IVec3::ZERO`.
- `camera_chunk_coord(Vec3::new(24.0, 20.0, 40.0))` returns `IVec3::new(1, 1, 2)`.
- `camera_chunk_coord(Vec3::new(-1.0, 0.0, 0.0))` returns `IVec3::new(-1, 0, 0)`.
- `chebyshev_distance(IVec3::ZERO, IVec3::new(3, 1, 2))` returns `3`.
- `chebyshev_distance(IVec3::ZERO, IVec3::ZERO)` returns `0`.

### 1.2 Implement desired chunk set computation

**Files:** `src/chunk_manager.rs`

Add a method:

```rust
fn desired_chunks(&self, camera_chunk: IVec3) -> Vec<IVec3>
```

Returns all chunk coordinates where `chebyshev_distance(camera_chunk, coord) <= self.load_radius`. For radius 5 this is an 11x11x11 cube (up to 1331 positions).

Tests:
- Radius 0 returns exactly 1 chunk (the camera chunk).
- Radius 1 returns 27 chunks (3x3x3).
- All returned chunks have Chebyshev distance <= radius from camera chunk.

### 1.3 Implement the `update` method — load/unload logic

**Files:** `src/chunk_manager.rs`

Add the core update method:

```rust
pub fn update(&mut self, camera_pos: Vec3, world: &mut World)
```

Logic:
1. Check if `self.update_interval` has elapsed since `self.last_update`. If not, return.
2. Reset `self.last_update = Instant::now()`.
3. Compute `camera_chunk = camera_chunk_coord(camera_pos)`.
4. Compute desired set via `desired_chunks(camera_chunk)`.
5. Find the nearest unloaded desired chunk (smallest Chebyshev distance to `camera_chunk` among desired chunks not present in `world`).
6. If no unloaded desired chunk exists, return (world is fully loaded for this region).
7. If `world.chunk_count() < self.max_loaded`: insert the nearest chunk using `generate_test_chunk()`.
8. If `world.chunk_count() >= self.max_loaded`: find the farthest loaded chunk (largest Chebyshev distance from `camera_chunk`). If the farthest is farther than the nearest unloaded, remove farthest and insert nearest. Otherwise, return (all loaded chunks are closer than any unloaded desired chunk).
9. Only one load/unload operation per update call.

Use `World::iter()` to scan loaded chunks for the farthest. Use `World::insert()` and `World::remove()` which already set the dirty flag.

Import `generate_test_chunk` from `crate::chunk`.

Tests:
- With radius 1, max_loaded 27, and a zero-duration interval: after enough `update()` calls from a fixed camera position, all 27 desired chunks are loaded.
- With max_loaded 5 and radius 1: only 5 chunks get loaded, they are the 5 nearest to camera.
- Moving camera to a new chunk coord causes distant chunks to be swapped for nearer ones after enough updates.

### 1.4 Integrate ChunkManager into App

**Files:** `src/main.rs`

1. Add `use chunk_manager::ChunkManager;` and `use std::time::Duration;`.
2. Add `chunk_manager: ChunkManager` field to `App`.
3. In `App::new()`: remove the static `for x in -1..=1 { for z in -1..=1 { ... } }` chunk insertion loop. Create `ChunkManager::new(5, 500, Duration::from_millis(200))`.
4. In `RedrawRequested`, before the `if self.world.is_dirty()` check, add:
   ```rust
   self.chunk_manager.update(self.camera.position, &mut self.world);
   ```
5. The existing dirty-check → `pack_gpu_data()` → `upload_world()` → `clear_dirty()` flow handles GPU upload automatically.

Note: On first frame, world starts empty. The chunk manager will load one chunk per update tick (every 200ms). To avoid a blank screen on startup, consider calling `self.chunk_manager.update()` in a loop (e.g., `max_loaded` times) during `App::new()` or `try_resume` with a zero-elapsed override, or add a `force_update` method that skips the timer check. This is addressed in step 1.5.

### 1.5 Add bulk initial load

**Files:** `src/chunk_manager.rs`, `src/main.rs`

Add a method to `ChunkManager`:

```rust
pub fn load_initial(&mut self, camera_pos: Vec3, world: &mut World)
```

This calls the core load logic (without the timer check) in a loop up to `max_loaded` times, loading one chunk per iteration until either the world is at capacity or all desired chunks are loaded. This ensures the world is populated before the first frame.

Call `self.chunk_manager.load_initial(self.camera.position, &mut self.world)` in `App::new()` after constructing the ChunkManager.

Tests:
- After `load_initial` with radius 2 and max_loaded 125: all 125 chunks in the 5x5x5 region are loaded.
- After `load_initial` with radius 2 and max_loaded 50: exactly 50 chunks loaded, all within desired set, prioritized by nearness.

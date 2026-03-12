# Chunk Persistence

**Summary:** Save modified chunks to disk as binary files and load them back when re-entering a region, with a config file (`voxels.toml`) controlling the storage directory. Guarantees writes via debounced periodic saves and save-on-eviction.
**Depends on:** chunk-manager, voxel-editing

---

## Steps

### 1. Add `toml` dependency and create config module

**Files:** `Cargo.toml`, `src/config.rs`, `src/main.rs`

Add `toml` and `serde` (with derive) to `Cargo.toml`. Create `src/config.rs` with:

```rust
pub struct Config {
    pub chunk_storage_dir: PathBuf,
}
```

Implement `Config::load()` which reads `voxels.toml` from the current directory. If the file doesn't exist, use defaults. The TOML structure:

```toml
chunk_storage_dir = "/tmp/voxels"
```

Use `serde::Deserialize` for the TOML structure. `Config::load()` returns `anyhow::Result<Config>`. Create the storage directory if it doesn't exist.

Create `voxels.toml` in the project root with `chunk_storage_dir = "/tmp/voxels"`.

Load config in `main()` and pass `config.chunk_storage_dir` into `App::new()`.

### 2. Add chunk serialization and deserialization

**Files:** `src/chunk.rs`

Add methods to `Chunk`:

- `pub fn save_to_file(path: &Path) -> anyhow::Result<()>` — writes `self.voxels` (4096 bytes) to the file using `std::fs::write`.
- `pub fn load_from_file(path: &Path) -> anyhow::Result<Chunk>` — reads exactly 4096 bytes from the file using `std::fs::read`, converts to `[u8; 4096]` via `TryInto`, and constructs a `Chunk`.

Add a helper function:

- `pub fn chunk_file_path(dir: &Path, pos: IVec3) -> PathBuf` — returns `dir.join(format!("chunk_{}_{}_{}", pos.x, pos.y, pos.z))`.

Tests:
- Round-trip: save a chunk, load it back, verify data matches.
- `load_from_file` with wrong-size file returns an error.
- `chunk_file_path` produces expected paths for positive/negative coordinates.

### 3. Track per-chunk modifications in World

**Files:** `src/world.rs`

Add a `modified_chunks: HashSet<IVec3>` field to `World`. When `set_voxel()` succeeds, insert the chunk position into `modified_chunks`. Add methods:

- `pub fn take_modified(&mut self) -> HashSet<IVec3>` — returns and clears the modified set.
- `pub fn is_chunk_modified(&self, pos: &IVec3) -> bool` — checks membership.
- `pub fn mark_chunk_saved(&mut self, pos: &IVec3)` — removes from modified set.

When `remove()` is called, also remove from `modified_chunks` (the caller is responsible for saving before removing if needed).

Tests:
- `set_voxel` adds chunk position to modified set.
- `take_modified` returns positions and clears.
- `remove` clears chunk from modified set.

### 4. Save modified chunks on eviction

**Files:** `src/chunk_manager.rs`, `src/main.rs`

Change `ChunkManager::drain_results()` so that before evicting a chunk, if the chunk is in `world.modified_chunks`, it saves the chunk to disk using `Chunk::save_to_file`. The storage dir `PathBuf` is stored on `ChunkManager`.

Update `ChunkManager::new()` to accept a `storage_dir: PathBuf` parameter and store it.

In `drain_results`, when processing an eviction:
1. Check if the evicted chunk is modified (`world.is_chunk_modified(&evict_pos)`).
2. If so, get the chunk data from world, save to file, mark saved.
3. Then remove from world as before.

Update `App::new()` to pass the storage dir from config.

### 5. Load chunks from disk before generating

**Files:** `src/chunk_manager.rs`

Pass `storage_dir: PathBuf` into the `chunk_loader_task` function. When loading a chunk at position `pos`:

1. Compute `chunk_file_path(&storage_dir, pos)`.
2. If the file exists, use `Chunk::load_from_file`. On error, log a warning and fall back to generation.
3. Otherwise, call `generate_test_chunk()`.

This applies to both `LoadAction::Load` and `LoadAction::Swap`.

### 6. Debounced periodic save of modified chunks

**Files:** `src/chunk_manager.rs`, `src/main.rs`

Add a save timer to the main loop. In `App`, add a `last_save: Instant` field. Each frame in `RedrawRequested`, check if enough time has elapsed since the last save (e.g. 5 seconds). If so, call a new method `ChunkManager::save_modified(&self, world: &mut World)` which iterates `world.take_modified()`, and for each position, gets the chunk from the world, saves to disk.

This ensures modified chunks are persisted within a bounded time even if they aren't evicted.

### 7. Save all modified chunks on shutdown

**Files:** `src/chunk_manager.rs`, `src/main.rs`

Add a `pub fn save_all_modified(&self, world: &mut World)` method to `ChunkManager` that saves every chunk in the modified set. Call this in `App`'s `CloseRequested` handler before exiting, and also in `ChunkManager::drop` isn't sufficient since we need world access.

In `window_event` for `CloseRequested`:
1. Call `self.chunk_manager.save_all_modified(&mut self.world)`.
2. Then `event_loop.exit()`.

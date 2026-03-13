# Deterministic Seed Persistence

**Summary:** Replace the hardcoded default seed with a time-based seed on first run, persist it to the storage directory, and reload it on subsequent runs so the same world is always regenerated. Ensure all noise-based generation (terrain, textures) is fully deterministic given a seed.

**Depends on:** None (builds on existing `Config`, `TerrainGenerator`, and `voxel_textures` infrastructure)

---

## Steps

### 1. Add `rand` dependency for time-based seed generation

**Files:** `Cargo.toml`

Add the `rand` crate as a dependency. Only the basic RNG functionality is needed — `rand::rng()` and `rand::Rng::random()` — to generate a `u32` seed from system entropy when no persisted seed exists.

### 2. Create seed file read/write utilities

**Files:** `src/seed.rs`

Create a new module responsible for loading and saving seed data to a JSON file in the storage directory.

- Define `SeedData` struct with a `seed: u32` field (and `serde::Serialize`/`Deserialize`).
- `pub fn load_seed(storage_dir: &Path) -> Result<Option<SeedData>>` — reads `<storage_dir>/seed.json`, returns `Ok(None)` if the file doesn't exist.
- `pub fn save_seed(storage_dir: &Path, data: &SeedData) -> Result<()>` — writes `seed.json` atomically (write to temp file, then rename).
- `pub fn resolve_seed(storage_dir: &Path, config_seed: Option<u32>) -> Result<u32>` — the main entry point:
  1. If `config_seed` is `Some(s)` (user explicitly set `[world] seed` in `voxels.toml`), use that value and persist it.
  2. Else if `seed.json` exists in storage dir, load and use the persisted seed.
  3. Else generate a new seed via `rand::rng().random::<u32>()`, persist it, and return it.
- Use JSON format (`serde_json`) so the file is human-readable and extensible for future noise parameters.
- Log the seed source (config override / loaded from file / newly generated) at `info` level.

**Tests:**
- `resolve_seed` with no file and no config seed generates and persists a new seed.
- `resolve_seed` with existing file returns the persisted seed.
- `resolve_seed` with config seed overrides the file.
- Round-trip: `save_seed` then `load_seed` returns the same data.
- `resolve_seed` called twice without config seed returns the same value (persistence works).

### 3. Add `serde_json` dependency

**Files:** `Cargo.toml`

Add `serde_json` for reading/writing the seed file. Also add `rand` if not done in step 1.

### 4. Integrate seed resolution into Config

**Files:** `src/config.rs`, `src/main.rs`

- Change `Config::load()` to return the `seed` field as `Option<u32>` (reflecting whether the user explicitly configured it).
- In `main.rs`, after loading config, call `resolve_seed(&config.chunk_storage_dir, config.seed)` to get the final seed value. Pass this resolved seed to `App::new()` and `build_voxel_atlas()`.
- Remove the `DEFAULT_SEED` constant from `config.rs`.

### 5. Register the seed module

**Files:** `src/main.rs`

Add `mod seed;` to the module declarations in `main.rs`.

### 6. Verify determinism across generation orders

**Files:** `src/terrain.rs` (tests only)

Add a test that generates the same set of chunks in different orders and asserts identical output. This validates the user's requirement that "chunks generated should be the same for a given seed no matter what order they are generated."

- `terrain_deterministic_regardless_of_order`: generate chunks at positions `[(0,0,0), (1,0,0), (0,0,1)]` in forward and reverse order, assert all chunk data matches.

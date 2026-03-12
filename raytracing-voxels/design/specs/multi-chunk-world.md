# Multi-Chunk World Rendering

**Summary:** Extend the renderer to support multiple 16³ chunks in a single scene by packing all voxel data into a mega-buffer with per-chunk metadata, and looping over chunks in the fragment shader with front-to-back ordering.
**Depends on:** voxel-chunk-raytracing, textured-voxels

---

## Steps

### 1.1 Add `World` struct to manage multiple chunks

**Files:** `src/world.rs`, `src/main.rs`

Create a `World` struct that holds chunks keyed by integer grid coordinates:

```rust
pub struct World {
    chunks: HashMap<[i32; 3], Chunk>,
}
```

Methods:
- `new() -> Self` — empty world.
- `insert(&mut self, pos: [i32; 3], chunk: Chunk)` — add/replace a chunk.
- `remove(&mut self, pos: &[i32; 3]) -> Option<Chunk>` — remove a chunk.
- `get(&self, pos: &[i32; 3]) -> Option<&Chunk>` — lookup.
- `iter(&self) -> impl Iterator<Item = (&[i32; 3], &Chunk)>` — iterate all.
- `chunk_count(&self) -> usize`

Add `mod world;` to `main.rs`. Replace the single `chunk: Chunk` field in `App` with `world: World`. In `try_resume`, insert the test chunk at `[0, 0, 0]` (and optionally a second chunk at `[1, 0, 0]` for testing).

Tests:
- Insert and retrieve a chunk by position.
- Remove returns the chunk and subsequent get returns None.
- Iterate yields all inserted chunks.

### 1.2 Define `ChunkInfo` GPU struct and pack world data

**Files:** `src/world.rs`

Add a `#[repr(C)] Pod/Zeroable` struct for GPU upload:

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuChunkInfo {
    pub world_offset: [f32; 3],  // world-space origin of chunk (pos * 16.0)
    pub data_offset: u32,        // byte offset into mega voxel buffer (in u32 units: chunk_index * 1024)
}
```

Add a method to `World`:

```rust
pub fn pack_gpu_data(&self) -> (Vec<u8>, Vec<GpuChunkInfo>)
```

This iterates all chunks in a deterministic order (sorted by key), concatenates their 4096-byte voxel arrays into a single `Vec<u8>`, and builds a parallel `Vec<GpuChunkInfo>` with each chunk's world offset (`pos * 16.0` so chunks tile seamlessly) and its u32 offset into the mega-buffer (`chunk_index * 1024` since 4096 bytes = 1024 u32s).

Tests:
- Single chunk: mega-buffer is 4096 bytes, info has offset 0 and world_offset `[0,0,0]`.
- Two chunks at `[0,0,0]` and `[1,0,0]`: mega-buffer is 8192 bytes, second info has data_offset 1024 and world_offset `[16,0,0]`.

### 1.3 Update renderer bind groups for multi-chunk buffers

**Files:** `src/voxel_renderer.rs`

Replace the single `chunk_buffer: wgpu::Buffer` and `chunk_bind_group` with:

```rust
voxel_mega_buffer: wgpu::Buffer,
chunk_info_buffer: wgpu::Buffer,
chunk_count_buffer: wgpu::Buffer,   // uniform, single u32 (padded to 16 bytes)
chunk_bind_group: wgpu::BindGroup,
chunk_bgl: wgpu::BindGroupLayout,   // store layout for re-creating bind group on resize
```

Update the chunk bind group layout (group 1) to have three bindings:
- `binding(0)`: `Storage { read_only: true }` — mega voxel buffer
- `binding(1)`: `Storage { read_only: true }` — chunk info array
- `binding(2)`: `Uniform` — chunk count (u32, padded to 16 bytes for alignment)

Replace `upload_chunk(&self, data: &[u8; 4096])` with:

```rust
pub fn upload_world(&mut self, voxel_data: &[u8], chunk_infos: &[GpuChunkInfo], chunk_count: u32)
```

This method should:
1. Check if existing buffers are large enough; if not, recreate them and rebuild the bind group.
2. Write voxel data, chunk info data, and chunk count to their respective buffers.

### 1.4 Update the WGSL shader for multi-chunk raymarching

**Files:** `src/voxels.wgsl`

Replace the single `voxels` storage buffer binding with:

```wgsl
struct ChunkInfo {
    world_offset: vec3<f32>,
    data_offset: u32,
};

@group(1) @binding(0) var<storage, read> voxels: array<u32>;
@group(1) @binding(1) var<storage, read> chunks: array<ChunkInfo>;
@group(1) @binding(2) var<uniform> chunk_count: u32;
```

Modify `get_voxel` to accept a data offset:

```wgsl
fn get_voxel(x: i32, y: i32, z: i32, data_offset: u32) -> u32 {
    // same bounds check
    let idx = u32(x) + u32(y) * 16u + u32(z) * 256u;
    let byte_idx = data_offset + idx / 4u;
    let shift = (idx % 4u) * 8u;
    return (voxels[byte_idx] >> shift) & 0xFFu;
}
```

Modify `fs_main`:
1. Loop over `chunk_count` chunks.
2. For each chunk, compute `chunk_min = chunks[i].world_offset` and `chunk_max = chunk_min + vec3(16.0)`.
3. `ray_box` test; if miss, continue.
4. Collect hits into a small fixed-size array (e.g., max 8 or 16 entries) of `(entry_t, chunk_index)`.
5. Sort hits by `entry_t` (simple insertion sort on the small array).
6. DDA march through each chunk in order; on first solid voxel hit, return the color. The existing DDA code moves mostly unchanged — just parameterized by `chunk_min`/`chunk_max`/`data_offset`.

Key changes to the DDA section:
- Replace hardcoded `chunk_min = vec3(-8.0)` with the chunk's `world_offset`.
- Replace hardcoded `chunk_max = vec3(8.0)` with `world_offset + vec3(16.0)`.
- Pass `chunks[i].data_offset` to `get_voxel`.

### 1.5 Wire up World in App and generate a multi-chunk test scene

**Files:** `src/main.rs`

Update `App`:
- Replace `chunk: Chunk` with `world: World`.
- In `App::new()`, create a `World` and insert multiple test chunks (e.g., a 3x1x3 grid of 9 chunks using `generate_test_chunk()`).
- In `try_resume`, call `world.pack_gpu_data()` and `renderer.upload_world(...)`.
- Remove the old `renderer.upload_chunk(...)` call.
- The camera default position may need adjusting since chunks now span a larger area (e.g., 48x16x48 for a 3x1x3 grid). Consider pulling the camera back to `[24, 20, 40]`.

### 1.6 Support dirty-tracking and incremental upload

**Files:** `src/world.rs`, `src/main.rs`

Add a dirty flag to `World`:

```rust
dirty: bool,
```

Set `dirty = true` in `insert` and `remove`. Add `fn is_dirty(&self) -> bool` and `fn clear_dirty(&mut self)`.

In the `RedrawRequested` handler, before rendering:
```rust
if self.world.is_dirty() {
    let (voxel_data, chunk_infos) = self.world.pack_gpu_data();
    renderer.upload_world(&voxel_data, &chunk_infos, chunk_infos.len() as u32);
    self.world.clear_dirty();
}
```

This avoids re-uploading every frame when nothing changed.

Tests:
- New world is not dirty.
- Insert sets dirty.
- clear_dirty resets it.
- Remove sets dirty.

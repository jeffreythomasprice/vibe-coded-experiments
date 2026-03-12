# Voxel Chunk Raytracing

**Summary:** Replace the test unit cube with a 16x16x16 voxel chunk backed by a GPU storage buffer, and implement DDA ray marching in the fragment shader to raytrace the voxels.
**Depends on:** wgpu-raytracing-cube (Phase 1), camera-controls (Phase 2)

---

## Steps

### 3.1 Chunk data structure on the Rust side

**Files:** `src/chunk.rs`, `src/main.rs`

Create the CPU-side chunk representation:

- Define a `Chunk` struct containing a `[u8; 4096]` array (16×16×16, 1 byte per voxel). Value `0` = air, nonzero = solid.
- Add helper methods:
  - `fn new() -> Self` — all air (zeroed).
  - `fn get(&self, x: usize, y: usize, z: usize) -> u8` — returns the voxel value at (x,y,z). Returns `0` if out of bounds.
  - `fn set(&mut self, x: usize, y: usize, z: usize, value: u8)` — sets the voxel. No-op if out of bounds.
  - `fn data(&self) -> &[u8; 4096]` — returns a reference to the raw data for GPU upload.
- Index mapping: `index = x + y * 16 + z * 16 * 16` (x varies fastest, then y, then z).
- Add a `fn generate_test_chunk() -> Chunk` function that fills the 12 edges and 8 corners of the 16×16×16 cube with solid voxels. A voxel is on an edge when at least two of its coordinates are at the boundary (0 or 15). This produces a wireframe cube outline — easy to visually verify orientation and face normals from any angle.
- Add `mod chunk;` to `main.rs`.
- **Tests:**
  - `set` then `get` round-trips correctly.
  - Out-of-bounds `get` returns 0, out-of-bounds `set` is a no-op.
  - `new()` is all zeros.
  - `generate_test_chunk()` has at least one solid voxel.

### 3.2 GPU storage buffer for chunk data

**Files:** `src/renderer.rs`

Create the wgpu buffer and bind group for the chunk voxel data:

- Create a `wgpu::Buffer` of size 4096 bytes with `BufferUsages::STORAGE | BufferUsages::COPY_DST`, label `"chunk_voxels"`.
- Create a new `BindGroupLayout` (binding 0, fragment visibility, storage buffer, read-only) for the chunk data. This will be `@group(1)`.
- Create a corresponding `BindGroup`.
- Update the `PipelineLayout` to include both bind group layouts: `[&camera_bgl, &chunk_bgl]`.
- Store the new buffer and bind group in the `Renderer` struct: `chunk_buffer: wgpu::Buffer`, `chunk_bind_group: wgpu::BindGroup`.
- Add a method `pub fn upload_chunk(&self, data: &[u8; 4096])` that calls `self.queue.write_buffer(&self.chunk_buffer, 0, data)`.
- In `render()`, set the chunk bind group: `pass.set_bind_group(1, &self.chunk_bind_group, &[])`.

### 3.3 Upload test chunk at startup

**Files:** `src/main.rs`

Wire the chunk data into the app lifecycle:

- In `App`, add a `chunk: Chunk` field, initialized with `generate_test_chunk()`.
- In `try_resume`, after creating the `Renderer`, call `renderer.upload_chunk(self.chunk.data())`.
- Move the default camera position further back (e.g., `(12, 10, 20)`) so the 16×16×16 chunk is visible. The chunk spans from `(-8,-8,-8)` to `(8,8,8)` — the camera needs to be outside this volume.

### 3.4 DDA ray marching in the shader

**Files:** `src/shader.wgsl`

Replace the unit-cube ray-box intersection with a DDA voxel traversal:

- **Storage buffer binding**: Declare `@group(1) @binding(0) var<storage, read> voxels: array<u32>;` — the 4096 bytes are accessed as 1024 `u32` values. To read voxel `(x,y,z)`: compute `idx = x + y*16 + z*256`, then `byte_idx = idx / 4`, `shift = (idx % 4) * 8`, `value = (voxels[byte_idx] >> shift) & 0xFF`.
- **Chunk AABB**: The chunk occupies world space `(-8,-8,-8)` to `(8,8,8)`. Define `chunk_min = vec3(-8.0)`, `chunk_max = vec3(8.0)`.
- **Ray-AABB intersection**: Reuse the existing `ray_box` function to intersect the ray with the chunk AABB. If miss, render sky.
- **DDA traversal** (Amanatides & Woo):
  1. Compute entry point: `entry = ro + rd * max(t_min, 0.0)`.
  2. Convert to chunk-local coordinates: `local = entry - chunk_min`.
  3. Determine starting voxel: `voxel = ivec3(floor(local))`, clamped to `[0, 15]`.
  4. Compute `step`, `t_delta`, and `t_max_axis` per axis from the ray direction (as in the pseudocode).
  5. Walk through voxels. For each voxel, sample from the storage buffer. If solid, return the hit with face normal derived from the last-stepped axis.
  6. Terminate when the voxel exits the `[0,15]` range on any axis.
  7. Max iteration cap (e.g., 128) as a safety guard against infinite loops.
- **Shading**: Same diffuse + ambient lighting as before, using the face normal from the DDA step.
- **Sky**: Same gradient for misses.
- Remove the old `box_normal` function (no longer needed — normal comes from DDA step direction).

### 3.5 Verify and tune

**Files:** `src/chunk.rs`

Verify the rendering works end-to-end and refine the test chunk:

- Run the app and confirm voxels render correctly from multiple angles.
- Adjust `generate_test_chunk()` if the initial pattern doesn't look right or is hard to visually verify (e.g., add a checkerboard floor, or L-shaped walls, to confirm face normals and orientation are correct).
- Ensure the camera default position gives a good initial view of the chunk.
- Verify that empty space inside the chunk is correctly transparent (rays pass through air voxels).
- Check that moving around the chunk with WASD/mouse works — the voxel geometry should appear solid from all directions.

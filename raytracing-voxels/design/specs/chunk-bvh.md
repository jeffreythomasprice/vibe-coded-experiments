# Chunk-Level Bounding Volume Hierarchy

**Summary:** Replace the O(n) linear scan of all chunks per ray in the fragment shader with a BVH (Bounding Volume Hierarchy) built on the CPU and traversed iteratively on the GPU, reducing chunk lookup to O(log n) per ray.
**Depends on:** multi-chunk-world, chunk-manager

---

## Steps

### 1.1 Create `GpuBvhNode` struct and BVH construction module

**Files:** `src/bvh.rs`, `src/main.rs`

Create a new module `bvh` with a GPU-friendly BVH node type:

```rust
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuBvhNode {
    pub aabb_min: Vec3,        // 12 bytes
    pub right_or_chunk: u32,   // 4 bytes — bit 31 set = leaf (lower 31 = chunk_infos index)
                               //           bit 31 clear = internal (value = right child index)
    pub aabb_max: Vec3,        // 12 bytes
    pub _padding: u32,         // 4 bytes — total 32 bytes per node
}
```

Implement `build_bvh(chunk_infos: &[GpuChunkInfo]) -> Vec<GpuBvhNode>` using top-down object median split:

1. Create a working list of `(chunk_index, centroid)` for each chunk. Each chunk's AABB is `[world_offset, world_offset + 16.0]`.
2. Allocate a `Vec<GpuBvhNode>` (capacity `2*N - 1` for N > 0 leaves).
3. Recursively build in depth-first order:
   - **Base case** (1 primitive): emit a leaf node with `right_or_chunk = chunk_index | 0x80000000`.
   - **Recursive case**: compute the bounding box of all primitives in range. Find the longest axis of the centroid bounds. Sort primitives by centroid along that axis. Split at the median index. Emit an internal node (placeholder `right_or_chunk = 0`). Recurse left half (appends at `parent + 1`). Record current array length as the right child index. Recurse right half. Patch the internal node's `right_or_chunk` with the right child index.
4. Return the flat array.

Add `mod bvh;` to `main.rs`.

Edge cases:
- 0 chunks → return empty `Vec`
- 1 chunk → single leaf node

Tests:
- Empty input produces empty node array.
- Single chunk produces one leaf node with correct flag and index.
- Two chunks produce 3 nodes (1 internal + 2 leaves).
- Root node's AABB encloses all chunk AABBs.
- Node count equals `2*N - 1` for N chunks (test with 5, 10, 50 chunks).
- Left child is always at `parent_index + 1`.
- All leaf `right_or_chunk` values have bit 31 set; all internal nodes don't.

### 1.2 Integrate BVH construction into `World::pack_gpu_data`

**Files:** `src/world.rs`

Change the return type of `pack_gpu_data`:

```rust
pub fn pack_gpu_data(&self) -> (Vec<u8>, Vec<GpuChunkInfo>, Vec<GpuBvhNode>)
```

After building the chunk info array (existing code), call `bvh::build_bvh(&chunk_infos)` and return the BVH nodes as the third tuple element.

Update existing tests that destructure the return value (`pack_single_chunk`, `pack_two_chunks`) to handle the 3-tuple.

### 1.3 Add BVH GPU buffers and bind group entries to the renderer

**Files:** `src/voxel_renderer.rs`

Add two new fields to `Renderer`:

```rust
bvh_node_buffer: wgpu::Buffer,    // storage buffer for BVH nodes
bvh_count_buffer: wgpu::Buffer,   // uniform buffer (u32 padded to 16 bytes)
```

Extend the `chunk_bgl` bind group layout with two new entries:
- `binding(3)`: `Storage { read_only: true }` — BVH node array
- `binding(4)`: `Uniform` — BVH node count

Update `Renderer::new()`:
- Create the two new buffers with initial minimum sizes (32 bytes for node buffer, 16 bytes for count buffer).
- Include them in the initial `chunk_bind_group`.

Update `upload_world` signature:

```rust
pub fn upload_world(&mut self, voxel_data: &[u8], chunk_infos: &[GpuChunkInfo], chunk_count: u32, bvh_nodes: &[GpuBvhNode])
```

In `upload_world`:
- Check if `bvh_node_buffer` is large enough for the new BVH data; recreate if needed (same pattern as `voxel_mega_buffer`).
- Write BVH node data via `queue.write_buffer`.
- Write BVH node count as `[count, 0, 0, 0]: [u32; 4]` to `bvh_count_buffer`.
- Include both new buffers in the bind group recreation.

### 1.4 Update call sites in `main.rs`

**Files:** `src/main.rs`

Update both call sites that invoke `pack_gpu_data` and `upload_world` (in `try_resume` and in the `RedrawRequested` handler):

```rust
let (voxel_data, chunk_infos, bvh_nodes) = self.world.pack_gpu_data();
renderer.upload_world(&voxel_data, &chunk_infos, chunk_infos.len() as u32, &bvh_nodes);
```

At this point the BVH data is uploaded to the GPU but the shader still uses the old linear scan. The scene should render identically — this step is a safe checkpoint.

### 1.5 Extract DDA march into a WGSL helper function

**Files:** `src/voxels.wgsl`

Extract the DDA raymarching loop (current lines ~126–233) into a reusable function:

```wgsl
struct MarchResult {
    hit: bool,
    color: vec4<f32>,
    t: f32,
};

fn march_chunk(ro: vec3<f32>, rd: vec3<f32>, chunk_min: vec3<f32>,
               data_offset: u32, max_t: f32) -> MarchResult
```

This function contains the existing DDA logic: entry point computation, voxel stepping, normal tracking, texture lookup, and lighting. It returns early with `hit = true` if a solid voxel is found at distance `< max_t`, otherwise returns `hit = false`.

Replace the inline DDA code in the existing sorted-march loop with a call to `march_chunk`. The scene should still render identically after this refactor — this is a pure extraction with no behavior change.

### 1.6 Replace linear chunk scan with BVH traversal in the shader

**Files:** `src/voxels.wgsl`

Add the new BVH bindings and struct:

```wgsl
struct BvhNode {
    aabb_min: vec3<f32>,
    right_or_chunk: u32,
    aabb_max: vec3<f32>,
    _padding: u32,
};

@group(1) @binding(3) var<storage, read> bvh_nodes: array<BvhNode>;
@group(1) @binding(4) var<uniform> bvh_node_count: u32;
```

Replace the linear chunk collection loop (lines ~83–112), the insertion sort (lines ~114–123), and the sorted march loop (lines ~126–234) with iterative BVH traversal:

```wgsl
var stack: array<u32, 32>;
var stack_ptr: i32 = 0;
var closest_t: f32 = 1e30;
var result_color: vec4<f32>;
var hit_anything: bool = false;

if bvh_node_count > 0u {
    stack[0] = 0u;
    stack_ptr = 1;
}

while stack_ptr > 0 {
    stack_ptr -= 1;
    let node_idx = stack[stack_ptr];
    let node = bvh_nodes[node_idx];

    let t = ray_box(ro, rd, node.aabb_min, node.aabb_max);
    if t.x > t.y || t.y < 0.0 || t.x > closest_t {
        continue;
    }

    let is_leaf = (node.right_or_chunk & 0x80000000u) != 0u;
    if is_leaf {
        let chunk_idx = node.right_or_chunk & 0x7FFFFFFFu;
        let result = march_chunk(ro, rd, chunks[chunk_idx].world_offset,
                                 chunks[chunk_idx].data_offset, closest_t);
        if result.hit {
            closest_t = result.t;
            result_color = result.color;
            hit_anything = true;
        }
    } else {
        let left_idx = node_idx + 1u;
        let right_idx = node.right_or_chunk;

        // Push farther child first so nearer child is popped first
        let t_left = ray_box(ro, rd, bvh_nodes[left_idx].aabb_min, bvh_nodes[left_idx].aabb_max);
        let t_right = ray_box(ro, rd, bvh_nodes[right_idx].aabb_min, bvh_nodes[right_idx].aabb_max);

        if t_left.x <= t_left.y && t_left.y >= 0.0 && t_left.x <= closest_t {
            if t_right.x <= t_right.y && t_right.y >= 0.0 && t_right.x <= closest_t {
                if t_left.x < t_right.x {
                    stack[stack_ptr] = right_idx; stack_ptr += 1;
                    stack[stack_ptr] = left_idx; stack_ptr += 1;
                } else {
                    stack[stack_ptr] = left_idx; stack_ptr += 1;
                    stack[stack_ptr] = right_idx; stack_ptr += 1;
                }
            } else {
                stack[stack_ptr] = left_idx; stack_ptr += 1;
            }
        } else if t_right.x <= t_right.y && t_right.y >= 0.0 && t_right.x <= closest_t {
            stack[stack_ptr] = right_idx; stack_ptr += 1;
        }
    }
}

if hit_anything {
    return result_color;
}
```

Remove the `ChunkHit` struct (no longer needed).

Key correctness properties:
- Chunks are non-overlapping, so `closest_t` from one chunk correctly prunes farther chunks.
- The `t.x > closest_t` check on internal nodes prunes entire subtrees.
- Near-to-far child ordering minimizes unnecessary marches.
- Stack depth 32 supports over 4 billion chunks theoretically.

### 1.7 Clean up and verify

**Files:** `src/voxels.wgsl`

Remove any dead code left from the old linear scan path (the `ChunkHit` struct, the hit collection array, the insertion sort). Verify with `cargo clippy` and `cargo test`. Run the application and confirm:

- Scene renders identically to before the BVH changes.
- No visual artifacts at chunk boundaries.
- FPS is equal or improved, especially with many chunks loaded.

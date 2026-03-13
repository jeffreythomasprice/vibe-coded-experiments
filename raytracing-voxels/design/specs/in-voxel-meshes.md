# In-Voxel Triangle Mesh Rendering

**Summary:** Place arbitrary triangle meshes inside voxels by extending the GPU raymarcher to do ray-triangle intersection for mesh-type voxel IDs, with per-triangle colors and precise CPU-side raycasting for interaction.
**Depends on:** None (builds on existing voxel rendering, lighting, and block interaction systems)

---

## Steps

### 1.1 Create mesh catalog with GPU types and torch mesh

**Files:** `src/mesh_catalog.rs`, `src/main.rs`

Create a new module `mesh_catalog` with the GPU-compatible types and a static mesh catalog:

**Types:**
```rust
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuTriangle {
    pub v0: [f32; 3],
    pub color_r: f32,
    pub v1: [f32; 3],
    pub color_g: f32,
    pub v2: [f32; 3],
    pub color_b: f32,
}
// 48 bytes, 16-byte aligned (3 × vec4<f32>)

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuMeshInfo {
    pub tri_offset: u32,
    pub tri_count: u32,
    pub _pad0: u32,
    pub _pad1: u32,
}
// 16 bytes

pub struct MeshCatalog {
    pub triangles: Vec<GpuTriangle>,
    pub mesh_infos: Vec<GpuMeshInfo>,
}
```

**Constants:**
```rust
pub const MESH_VOXEL_BASE: u8 = 128;
pub const MESH_TORCH: u8 = MESH_VOXEL_BASE; // = 128
```

**`MeshCatalog::build() -> Self`:** Constructs the catalog with an initial torch mesh.

Add a helper `fn tri(v0: [f32;3], v1: [f32;3], v2: [f32;3], color: [f32;3]) -> GpuTriangle` to simplify mesh construction.

Add `fn build_torch() -> Vec<GpuTriangle>` that builds the torch geometry in voxel-local [0,1]³ space. Reference: Minecraft-style blocky torch — a square wooden stick with a chunky pixelated flame on top.

**Torch geometry (all coordinates in [0,1]³ voxel-local space):**

*Stick* — a square prism, centered on X/Z, sitting on the voxel floor:
- Cross-section: 0.25 wide (x: 0.375–0.625, z: 0.375–0.625)
- Height: bottom of voxel to 60% up (y: 0.0–0.6)
- 4 side faces + 1 top cap + 1 bottom cap = 6 faces × 2 tris = 12 triangles
- Colors: sides use two alternating browns for visual depth:
  - Light brown `[0.55, 0.35, 0.15]` for front/back faces
  - Dark brown `[0.40, 0.25, 0.10]` for left/right faces
  - Medium brown `[0.48, 0.30, 0.12]` for top and bottom caps

*Flame* — a jagged cluster of triangles sitting on top of the stick, slightly wider than the stick:
- Base flame block: a smaller prism from y 0.6 to y 0.8, slightly wider than the stick (x: 0.35–0.65, z: 0.35–0.65). 4 side faces × 2 tris = 8 triangles. Color: bright yellow `[1.0, 0.85, 0.2]`.
- Flame tip: 4 upward-pointing triangles (one per side face of the flame block), each a single triangle with base at y=0.8 and apex at y=0.95, slightly narrower. Color: orange-red `[1.0, 0.45, 0.05]`.
- Flame peak: 1 small triangle on top pointing up from ~y=0.9 to y=1.0, offset slightly off-center for a natural look. Color: red `[0.9, 0.2, 0.0]`.

Total: 12 (stick) + 8 (flame base) + 4 (flame tips) + 1 (flame peak) = **25 triangles**.

Register `mod mesh_catalog;` in `main.rs`.

**Tests:**
- `mesh_catalog_builds_without_panic` — call `MeshCatalog::build()` and assert `triangles.len() == 25` and `mesh_infos.len() >= 1`
- `gpu_triangle_size` — assert `std::mem::size_of::<GpuTriangle>() == 48`
- `gpu_mesh_info_size` — assert `std::mem::size_of::<GpuMeshInfo>() == 16`
- `torch_triangles_within_unit_cube` — verify all triangle vertices have coordinates in [0.0, 1.0]

### 1.2 Add mesh buffers to the renderer

**Files:** `src/voxel_renderer.rs`

Add two new fields to the `Renderer` struct:
```rust
mesh_triangle_buffer: wgpu::Buffer,
mesh_info_buffer: wgpu::Buffer,
```

Initialize them in `Renderer::new()` with minimum sizes (48 bytes for triangles, 16 bytes for infos), same pattern as `bvh_node_buffer`.

Add two new entries to `chunk_bgl` (the bind group layout created at line 200):
- Binding 5: `Storage { read_only: true }` for mesh triangles
- Binding 6: `Storage { read_only: true }` for mesh infos

Add corresponding `BindGroupEntry` items (bindings 5 and 6) to every place that creates a `chunk_bind_group` — both in `Renderer::new()` (line 256) and in `upload_world()` (line 534).

Add a new method:
```rust
pub fn upload_mesh_catalog(&mut self, catalog: &MeshCatalog) {
    // Resize mesh_triangle_buffer and mesh_info_buffer if needed
    // Write triangle and mesh_info data
    // Recreate chunk_bind_group with all 7 bindings
}
```

Follow the existing resize-and-recreate pattern from `upload_world()`.

### 1.3 Add mesh structs and ray-triangle intersection to shader

**Files:** `src/voxels.wgsl`

Add new WGSL structs and bindings after the existing BVH bindings (after line 49):

```wgsl
struct MeshTriangle {
    v0: vec3<f32>,
    color_r: f32,
    v1: vec3<f32>,
    color_g: f32,
    v2: vec3<f32>,
    color_b: f32,
};

struct MeshInfo {
    tri_offset: u32,
    tri_count: u32,
    _pad0: u32,
    _pad1: u32,
};

@group(1) @binding(5) var<storage, read> mesh_triangles: array<MeshTriangle>;
@group(1) @binding(6) var<storage, read> mesh_infos: array<MeshInfo>;
```

Add a Moller-Trumbore `ray_triangle()` function:
```wgsl
struct TriHitResult {
    hit: bool,
    t: f32,
    normal: vec3<f32>,
};

fn ray_triangle(ro: vec3<f32>, rd: vec3<f32>, v0: vec3<f32>, v1: vec3<f32>, v2: vec3<f32>, max_t: f32) -> TriHitResult
```
Returns `hit=true` with the `t` distance and geometric normal (oriented to face the ray) when the ray intersects the triangle within `(0.001, max_t)`.

Add `intersect_mesh_voxel()`:
```wgsl
fn intersect_mesh_voxel(ro: vec3<f32>, rd: vec3<f32>, voxel_world_min: vec3<f32>, mesh_type: u32, max_t: f32) -> MarchResult
```
Transforms `ro` into voxel-local space by subtracting `voxel_world_min`, then loops over triangles from `mesh_infos[mesh_type].tri_offset` to `tri_offset + tri_count`, calling `ray_triangle()` for each. Returns the closest hit with per-triangle color as `result.color`.

### 1.4 Modify DDA loop to handle mesh voxels

**Files:** `src/voxels.wgsl`

In `march_chunk()` (DDA loop starting at line 275), modify the `if v != 0u` block. After the `t_hit >= max_t` early-out (line 289-291), add a branch:

```wgsl
if v >= 128u {
    // Mesh voxel: ray-trace triangles
    let mesh_type = v - 128u;
    let voxel_world = chunk_min + vec3<f32>(f32(voxel.x), f32(voxel.y), f32(voxel.z));
    let mesh_result = intersect_mesh_voxel(ro, rd, voxel_world, mesh_type, max_t);
    if mesh_result.hit {
        result.hit = true;
        result.color = mesh_result.color;
        result.t = mesh_result.t;
        result.normal = mesh_result.normal;
        return result;
    }
    // No triangle hit — ray passes through, continue DDA
} else {
    // Existing solid voxel texture sampling code (lines 293-317)
}
```

In `march_chunk_occlusion()` (line 379), change `if v != 0u` to `if v != 0u && v < 128u` so mesh voxels don't block shadow rays. This is the simple initial behavior — mesh shadow support can be added later.

### 1.5 Wire up mesh catalog initialization

**Files:** `src/main.rs`

In the `try_resume()` method (or wherever `upload_voxel_atlas()` is called), after atlas upload:
```rust
let mesh_catalog = mesh_catalog::MeshCatalog::build();
renderer.upload_mesh_catalog(&mesh_catalog);
```

Store the `MeshCatalog` in the `App` struct so it's available for CPU-side raycasting and hotbar rendering later.

Verify: `cargo build && cargo clippy && cargo test`.

### 1.7 Add torch to hotbar and rework placement/removal controls

**Files:** `src/main.rs`

This step changes how torches and point lights are placed and removed, and adds the torch as a selectable hotbar item.

**Hotbar changes:**

Add torch as slot 9 on the hotbar. Currently the hotbar uses `TILE_PICKER_TILE_IDS: [u8; 6]` for display (IDs `[3, 2, 1, 4, 5, 6]`) and `VOXEL_KEY_TO_ID: [u8; 7]` for key mapping, with keys 0-6. Extend these:

- Change `active_voxel_type: Option<u8>` — this already supports any u8 value, so `MESH_TORCH` (128) works as-is.
- Add `KeyCode::Digit9` handler: `self.active_voxel_type = Some(mesh_catalog::MESH_TORCH)`
- Extend the hotbar rendering to show a 9th slot for the torch:
  - Draw a small torch icon in the slot. Since we can't easily render the 3D mesh in the 2D overlay, draw a simple stylized torch using `DrawList::solid_rect()` calls — a brown rectangle for the stick and an orange/yellow rectangle for the flame. Or use a few colored quads to approximate the torch silhouette.
  - Draw "9" as the key label below the torch slot, using the same `TextureAtlasFont::draw_text()` pattern as existing labels (lines 824-858).
  - Draw "torch" or "🔥" as label text if there's room, or just rely on the icon + number.
  - Apply the same `TILE_PICKER_HIGHLIGHT_COLOR` yellow highlight when `active_voxel_type == Some(MESH_TORCH)`.
- Widen the hotbar background to accommodate the extra slot. The hotbar currently calculates its width from `num_tiles` (line 733) — increase this or add the torch slot with a small gap/separator after the block tiles.
- Also update the "0: none" label rendering to account for the wider bar.

**Torch placement (right-click when torch selected):**

Modify the right-click handler (lines 335-357). Currently it checks `active_voxel_type.is_some()` and places a block. Change to:
```rust
if let Some(voxel_id) = self.active_voxel_type {
    // Raycast, compute place_pos (same as current code)
    if voxel_id >= mesh_catalog::MESH_VOXEL_BASE {
        // Mesh placement: place torch voxel + create point light
        self.world.set_voxel(place_pos, voxel_id);
        self.point_lights.push(GpuPointLight {
            position: [place_pos.x as f32 + 0.5, place_pos.y as f32 + 0.85, place_pos.z as f32 + 0.5],
            radius: 8.0,
            color: [1.0, 0.9, 0.7],
            _padding: 0.0,
        });
        self.point_lights_dirty = true;
    } else {
        // Existing block placement (unchanged)
        self.world.set_voxel(place_pos, voxel_id);
    }
}
```

The point light position is at `(+0.5, +0.85, +0.5)` relative to the voxel origin, placing it inside the flame geometry.

**Torch removal (left-click, instant, regardless of selected hotbar item):**

Modify the left-click handler (lines 301-326). Currently it starts `break_state` for hold-to-break. Change to:

```rust
// Raycast from camera
if let Some(hit) = raycast_result {
    if hit.voxel_id >= mesh_catalog::MESH_VOXEL_BASE {
        // Mesh voxel: instant removal, no hold-to-break
        let world_pos = hit.chunk_pos * 16 + IVec3::new(...);
        self.world.set_voxel(world_pos, 0);
        // Find and remove the associated point light
        let voxel_center = Vec3::new(world_pos.x as f32 + 0.5, world_pos.y as f32 + 0.5, world_pos.z as f32 + 0.5);
        if let Some(idx) = self.point_lights.iter().position(|l| {
            let lp = Vec3::from(l.position);
            (lp - voxel_center).length() < 1.0  // light is within the same voxel
        }) {
            self.point_lights.remove(idx);
            self.point_lights_dirty = true;
        }
    } else {
        // Existing hold-to-break for solid blocks (unchanged)
        self.break_state = Some(BreakState { ... });
    }
}
```

Key behaviors:
- Left-clicking a torch removes it instantly (no break progress, no hold required)
- The associated point light is also removed by finding the light whose position is within the same voxel
- This works regardless of what's currently selected on the hotbar
- Left-clicking a regular block still uses the existing hold-to-break mechanic

**Remove old middle-click point light controls:**

Remove the middle-click place light handler (lines 383-407) and the shift+middle-click remove handler (lines 365-381). These are replaced by the right-click/left-click torch controls above. Middle-click becomes unused (or can be kept as a no-op for now).

Verify: `cargo build && cargo clippy && cargo test`, then `cargo run`:
- Press 9 to select torch on hotbar (should highlight)
- Right-click to place torch + point light
- Press 1 to switch to grass, left-click a torch to instantly remove it + its light
- Press 0 for "nothing" mode, verify left-click still removes torches

### 1.6 Add precise CPU-side mesh raycasting

**Files:** `src/mesh_catalog.rs`, `src/world.rs`

Add a CPU-side Moller-Trumbore implementation to `MeshCatalog`:
```rust
pub fn raycast(&self, mesh_type: u8, local_origin: Vec3, direction: Vec3) -> Option<(f32, Vec3)>
```
Takes a ray in voxel-local [0,1]³ space, tests all triangles for the given mesh type, returns `(t, normal)` of the closest hit.

Modify `World::raycast()` to accept an optional `&MeshCatalog` parameter (or store a reference/arc). In the DDA loop (line 226-235), when `id >= MESH_VOXEL_BASE`:
1. Compute the voxel-local ray origin: `origin + dir * t_entry - voxel_world_min`
2. Call `mesh_catalog.raycast(id - MESH_VOXEL_BASE, local_origin, dir)`
3. If hit, return `RaycastHit` with the mesh voxel's position and the triangle normal
4. If no hit, continue the DDA loop (ray passes through)

Also handle the starting-inside-mesh-voxel case (line 138-147) — if `id >= MESH_VOXEL_BASE`, do a mesh raycast from the origin position instead of immediately returning.

**Tests:**
- `raycast_hits_mesh_triangle` — place a `MESH_TORCH` voxel, cast a ray that should hit the torch geometry, verify hit
- `raycast_misses_mesh_triangle` — cast a ray through a `MESH_TORCH` voxel that passes through empty space (e.g., corner of the voxel), verify no hit / hits something behind it
- `raycast_mesh_continues_past_miss` — place a `MESH_TORCH` voxel in front of a solid block, cast a ray that misses the mesh triangles, verify it hits the solid block behind

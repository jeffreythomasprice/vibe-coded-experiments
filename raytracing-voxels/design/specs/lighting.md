# Lighting System

**Summary:** Add a configurable directional sun light with shadow rays and placeable point lights with radius-based falloff to the voxel raytracer. The sun casts real shadows via secondary ray marches through the BVH. Point lights are unshadowed initially.
**Depends on:** chunk-bvh, textured-voxels

---

## Steps

### 1.1 Refactor MarchResult to carry normal and unlit color

**Files:** `src/voxels.wgsl`

Add a `normal` field to the `MarchResult` struct (currently at line 192):

```wgsl
struct MarchResult {
    hit: bool,
    color: vec4<f32>,  // raw texture color, unlit
    t: f32,
    normal: vec3<f32>,
};
```

In `march_chunk()`, stop computing lighting inline. Remove the current lighting code (lines 289-292 where `light_dir`, `diffuse`, `ambient`, and `color` are computed). Instead, store the raw `tex_color` and current `normal` in the result:

```wgsl
result.hit = true;
result.color = tex_color;      // was: vec4(color, 1.0)
result.t = t_hit;
result.normal = normal;
return result;
```

In `fs_main()`, add a `result_normal` variable tracked alongside `result_color` and `closest_t` (line 339 area):

```wgsl
var result_normal: vec3<f32> = vec3<f32>(0.0);
```

In the BVH leaf hit block (line 362-366), save the normal:

```wgsl
if mr.hit {
    closest_t = mr.t;
    result_color = mr.color;
    result_normal = mr.normal;
    hit_anything = true;
}
```

After the BVH traversal, in the `if hit_anything` block (line 397-398), apply the existing hardcoded lighting so the output is visually identical:

```wgsl
if hit_anything {
    let light_dir = normalize(vec3<f32>(1.0, 2.0, 3.0));
    let diffuse = max(dot(result_normal, light_dir), 0.0);
    let ambient = 0.15;
    let lit = result_color.rgb * (ambient + diffuse * 0.85);
    final_color = vec4<f32>(lit, 1.0);
}
```

This step is a pure refactor. The scene must render identically to before.

### 1.2 Create lighting GPU structs and bind group (Rust side)

**Files:** `src/voxel_renderer.rs`

Add two new `#[repr(C)]` Pod/Zeroable structs following the same pattern as `GpuInteractionState` (line 15-22):

```rust
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuLightingUniforms {
    pub sun_dir: [f32; 3],
    pub ambient: f32,
    pub sun_color: [f32; 3],
    pub sun_intensity: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct GpuPointLight {
    pub position: [f32; 3],
    pub radius: f32,
    pub color: [f32; 3],
    pub _padding: f32,
}
```

Add new fields to the `Renderer` struct (after `interaction_bind_group` at line 44):

```rust
lighting_buffer: wgpu::Buffer,
point_light_buffer: wgpu::Buffer,
point_light_count_buffer: wgpu::Buffer,
lighting_bind_group: wgpu::BindGroup,
lighting_bgl: wgpu::BindGroupLayout,
```

In `Renderer::new()`, create the bind group layout with 3 entries following the pattern of `interaction_bgl` (lines 289-301):

- Binding 0: `Uniform` buffer — `GpuLightingUniforms` (32 bytes)
- Binding 1: `Storage { read_only: true }` buffer — point light array (initial size: `64 * size_of::<GpuPointLight>()`, minimum 32 bytes)
- Binding 2: `Uniform` buffer — point light count (16 bytes, padded u32)

All entries use `ShaderStages::FRAGMENT` visibility.

Create the three buffers and the initial bind group. Add `&lighting_bgl` to the pipeline layout's `bind_group_layouts` array (line 314) as the 5th entry (index 4).

Add `upload_lighting(&self, uniforms: &GpuLightingUniforms)` method:
- Writes `bytemuck::bytes_of(uniforms)` to `lighting_buffer` via `queue.write_buffer`.

Add `upload_point_lights(&mut self, lights: &[GpuPointLight])` method:
- Writes `point_light_count` as `[count, 0, 0, 0]: [u32; 4]` to `point_light_count_buffer`.
- If lights is empty, just write count. Otherwise write the light data to `point_light_buffer`, recreating the buffer and bind group if it needs to grow (same pattern as voxel buffer growth in `upload_world`, lines 389-457).

In `render_voxels()` (line 599), add `pass.set_bind_group(4, &self.lighting_bind_group, &[]);` after the interaction bind group.

### 1.3 Add lighting uniform declarations in shader and read from uniforms

**Files:** `src/voxels.wgsl`

Add WGSL struct declarations and bind group 4 bindings after the existing interaction group declarations:

```wgsl
struct LightingUniforms {
    sun_dir: vec3<f32>,
    ambient: f32,
    sun_color: vec3<f32>,
    sun_intensity: f32,
};

struct PointLight {
    position: vec3<f32>,
    radius: f32,
    color: vec3<f32>,
    _padding: f32,
};

@group(4) @binding(0) var<uniform> lighting: LightingUniforms;
@group(4) @binding(1) var<storage, read> point_lights: array<PointLight>;
@group(4) @binding(2) var<uniform> point_light_count: u32;
```

Replace the hardcoded lighting in `fs_main()` (from step 1.1) with uniform reads:

```wgsl
if hit_anything {
    let sun_diffuse = max(dot(result_normal, lighting.sun_dir), 0.0);
    let lit = result_color.rgb * (lighting.ambient + lighting.sun_color * lighting.sun_intensity * sun_diffuse);
    final_color = vec4<f32>(lit, 1.0);
}
```

Upload initial lighting uniforms from `main.rs` with values matching the old hardcoded behavior: `sun_dir = normalize(1,2,3)`, `ambient = 0.15`, `sun_color = [1,1,1]`, `sun_intensity = 0.85`. Upload an empty point light array.

The scene must still render identically. This validates the full CPU→GPU uniform pipeline.

### 1.4 Implement shadow ray occlusion test

**Files:** `src/voxels.wgsl`

Add `march_chunk_occlusion()` — a simplified version of `march_chunk()` that returns `bool` instead of `MarchResult`. It uses the same DDA stepping logic but:
- Returns `true` immediately on hitting any non-air voxel (no texture sampling, UV computation, or color calculation)
- Returns `false` if the ray exits the chunk without hitting anything
- Uses the same 128-iteration loop limit
- Takes the same parameters: `(ro, rd, chunk_min, data_offset, max_t)`

Add `is_occluded(origin, direction, max_dist)` — a simplified version of the `fs_main()` BVH traversal loop that:
- Uses the same stack-based BVH traversal (stack of 32, same push/pop logic)
- Calls `march_chunk_occlusion()` instead of `march_chunk()` at leaf nodes
- Returns `true` on first hit from any chunk (early exit)
- Returns `false` if no chunk's voxels are hit
- Uses `max_dist` as the `closest_t` bound (cap shadow ray distance)

### 1.5 Wire up sun shadow rays

**Files:** `src/voxels.wgsl`

In `fs_main()`, update the lighting block to cast a shadow ray before applying sun diffuse:

```wgsl
if hit_anything {
    let hit_pos = ro + rd * closest_t;
    let shadow_origin = hit_pos + result_normal * 0.01;

    var total_light = vec3<f32>(lighting.ambient);

    let sun_diffuse = max(dot(result_normal, lighting.sun_dir), 0.0);
    if sun_diffuse > 0.0 {
        let in_shadow = is_occluded(shadow_origin, lighting.sun_dir, 128.0);
        if !in_shadow {
            total_light += lighting.sun_color * lighting.sun_intensity * sun_diffuse;
        }
    }

    final_color = vec4<f32>(result_color.rgb * total_light, 1.0);
}
```

Key details:
- `shadow_origin` is offset by `0.01` along the surface normal to prevent self-intersection (shadow acne)
- Back-face optimization: skip shadow ray entirely when `sun_diffuse <= 0.0` (surface faces away from sun)
- Shadow ray max distance of 128.0 world units (8 chunks) balances quality vs performance
- Shadows appear under overhangs, inside caves, and behind trees

### 1.6 Add point light loop in shader

**Files:** `src/voxels.wgsl`

After the sun shadow calculation in `fs_main()`, add the point light accumulation loop:

```wgsl
    let pc = point_light_count;
    for (var i = 0u; i < pc; i += 1u) {
        let light = point_lights[i];
        let to_light = light.position - hit_pos;
        let dist = length(to_light);
        if dist > light.radius { continue; }
        let dir = to_light / dist;
        let ndotl = max(dot(result_normal, dir), 0.0);
        if ndotl <= 0.0 { continue; }
        let attenuation = max(1.0 - (dist * dist) / (light.radius * light.radius), 0.0);
        total_light += light.color * ndotl * attenuation;
    }
```

The falloff formula `1 - (d/r)^2` provides smooth quadratic attenuation that reaches zero at the radius boundary. No shadow rays for point lights in this step.

### 1.7 Add point light placement and removal input handling

**Files:** `src/main.rs`

Add to the `App` struct:
```rust
point_lights: Vec<GpuPointLight>,
point_lights_dirty: bool,
```

Initialize both in `App` construction (empty vec, `false`).

Add a `MouseButton::Middle` handler in `window_event()` following the pattern of the existing `MouseButton::Left` (line 289-314) and `MouseButton::Right` (line 323-339) handlers:

**Middle click (place light):**
- Only when `cursor_grabbed` is true
- Call `self.world.raycast(camera.position, camera.forward(), INTERACT_REACH)`
- If hit, compute light position: `hit.position + hit.normal * 0.5` (center of adjacent air voxel)
- Push a `GpuPointLight { position: pos.to_array(), radius: 8.0, color: [1.0, 0.9, 0.7], _padding: 0.0 }`
- Set `point_lights_dirty = true`

**Shift + Middle click (remove nearest light):**
- Check if shift is held (via `InputState` or `modifiers`)
- Find the point light closest to the camera position within `INTERACT_REACH`
- Remove it from the vec if found
- Set `point_lights_dirty = true`

In the `RedrawRequested` handler, before rendering:
- Upload `GpuLightingUniforms` every frame (32 bytes, cheap)
- If `point_lights_dirty`, call `renderer.upload_point_lights(&self.point_lights)` and reset the flag

### 1.8 Add lighting configuration

**Files:** `src/config.rs`

Add a new `LightingConfig` struct and optional field on `ConfigFile`:

```rust
#[derive(Deserialize)]
struct LightingConfig {
    sun_angle: Option<f32>,
    ambient: Option<f32>,
    sun_intensity: Option<f32>,
}
```

Add `#[serde(default)] lighting: Option<LightingConfig>` to `ConfigFile`.

Expose on the public `Config` struct:
```rust
pub sun_angle: f32,      // default: ~0.6405 (atan2(2,3) to match normalize(1,2,3) initial direction)
pub ambient: f32,        // default: 0.15
pub sun_intensity: f32,  // default: 0.85
```

In `main.rs`, derive the sun direction from the config's `sun_angle`:
```rust
let sun_dir = Vec3::new(sun_angle.cos(), sun_angle.sin().abs().max(0.1), 0.3).normalize();
```

This makes the sun angle configurable via `voxels.toml`:
```toml
[lighting]
sun_angle = 1.1
ambient = 0.15
sun_intensity = 0.85
```

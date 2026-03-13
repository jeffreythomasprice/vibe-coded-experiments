# Sun Day/Night Cycle

**Summary:** Animate the sun across the sky on a configurable day-length cycle (default 10 minutes), render a visible sun disc in the sky, and disable directional light when the sun is below the horizon.
**Depends on:** lighting

---

## Steps

### 1.1 Animate sun_angle over time

**Files:** `src/main.rs`

Add a constant for day duration:

```rust
const DAY_DURATION_SECS: f32 = 600.0; // 10 minutes = one full day
```

Each frame in `RedrawRequested`, advance `self.sun_angle` based on `dt`:

```rust
self.sun_angle += (std::f32::consts::TAU / DAY_DURATION_SECS) * dt;
```

This wraps the angle through a full 2π rotation over `DAY_DURATION_SECS`. No modulo needed since `sin`/`cos` handle any angle.

Update the sun direction computation (currently in `RedrawRequested` around line 574) to derive the direction from the orbiting angle. The sun orbits in a vertical plane:

```rust
let sun_y = self.sun_angle.sin();
let sun_xz = self.sun_angle.cos();
let sun_dir = Vec3::new(sun_xz, sun_y, 0.3).normalize();
```

When `sun_y < 0.0`, the sun is below the horizon.

### 1.2 Disable directional light below the horizon

**Files:** `src/main.rs`

Before uploading lighting uniforms, check whether the sun is above the horizon. If the y-component of the sun direction is `<= 0.0`, set `sun_intensity` to `0.0` so the shader applies no directional light. The existing shader already skips the shadow ray when `sun_diffuse <= 0.0`, so surfaces facing away will be correct. Setting intensity to zero ensures no light leaks through for any surface.

```rust
let sun_y = self.sun_angle.sin();
let effective_intensity = if sun_y > 0.0 {
    self.sun_intensity
} else {
    0.0
};
```

Pass `effective_intensity` as `sun_intensity` in `GpuLightingUniforms`. Also pass the actual `sun_dir` (even below horizon) so the shader sun disc rendering (step 1.4) knows where to draw the sun near the horizon.

### 1.3 Pass sun direction to the shader for sky rendering

**Files:** `src/voxel_renderer.rs`, `src/voxels.wgsl`

The shader already receives `lighting.sun_dir` — no new uniforms needed. However, add a `time_of_day` field to `GpuLightingUniforms` so the shader can blend sky colors for sunrise/sunset:

Add `time_of_day: f32` to `GpuLightingUniforms` (after `sun_intensity`). This is a normalized value `0.0..1.0` representing position in the day cycle (0.0 = midnight, 0.25 = sunrise, 0.5 = noon, 0.75 = sunset).

```rust
pub struct GpuLightingUniforms {
    pub sun_dir: [f32; 3],
    pub ambient: f32,
    pub sun_color: [f32; 3],
    pub sun_intensity: f32,
    pub time_of_day: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
}
```

In `main.rs`, compute and pass `time_of_day`:

```rust
let time_of_day = (self.sun_angle / std::f32::consts::TAU).fract();
```

In the WGSL shader, add the field to the `LightingUniforms` struct:

```wgsl
struct LightingUniforms {
    sun_dir: vec3<f32>,
    ambient: f32,
    sun_color: vec3<f32>,
    sun_intensity: f32,
    time_of_day: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};
```

### 1.4 Render a visible sun disc in the shader

**Files:** `src/voxels.wgsl`

In `fs_main()`, in the sky rendering branch (the `else` block when `!hit_anything`), add a sun disc test before the sky gradient:

```wgsl
let sun_dot = dot(rd, lighting.sun_dir);
let sun_angular_size = 0.995; // ~5.7° angular radius
if sun_dot > sun_angular_size {
    // Bright sun disc
    let sun_edge = smoothstep(sun_angular_size, sun_angular_size + 0.003, sun_dot);
    let sun_glow = vec3<f32>(1.0, 0.95, 0.8) * sun_edge * 2.0;
    final_color = vec4<f32>(min(sun_glow, vec3<f32>(1.0)), 1.0);
} else if sun_dot > 0.98 {
    // Soft glow halo around the sun
    let glow_strength = smoothstep(0.98, sun_angular_size, sun_dot);
    let glow = vec3<f32>(1.0, 0.9, 0.7) * glow_strength * 0.3;
    final_color = vec4<f32>(sky + glow, 1.0);
} else {
    final_color = vec4<f32>(sky, 1.0);
}
```

The sun disc should only render when the sun is above (or near) the horizon. Use `lighting.sun_dir.y > -0.05` as the visibility threshold so the disc is visible as it sets.

### 1.5 Dynamic sky color based on sun position

**Files:** `src/voxels.wgsl`

Replace the current static sky gradient with one that varies based on sun elevation (`lighting.sun_dir.y`):

- **Daytime** (sun_dir.y > 0.2): Current blue sky gradient
- **Sunrise/sunset** (sun_dir.y between -0.1 and 0.2): Warm orange/pink gradient blended with the blue
- **Night** (sun_dir.y < -0.1): Dark blue/near-black sky

```wgsl
let sun_elev = lighting.sun_dir.y;

// Base sky colors
let day_top = vec3<f32>(0.3, 0.5, 0.8);
let day_bottom = vec3<f32>(0.8, 0.85, 0.9);
let sunset_top = vec3<f32>(0.2, 0.15, 0.4);
let sunset_bottom = vec3<f32>(0.9, 0.4, 0.2);
let night_top = vec3<f32>(0.02, 0.02, 0.06);
let night_bottom = vec3<f32>(0.05, 0.05, 0.1);

let sky_t = rd.y * 0.5 + 0.5;

if sun_elev > 0.2 {
    sky = mix(day_bottom, day_top, sky_t);
} else if sun_elev > -0.1 {
    let blend = smoothstep(-0.1, 0.2, sun_elev);
    let top = mix(sunset_top, day_top, blend);
    let bottom = mix(sunset_bottom, day_bottom, blend);
    sky = mix(bottom, top, sky_t);
} else {
    let blend = smoothstep(-0.3, -0.1, sun_elev);
    let top = mix(night_top, sunset_top, blend);
    let bottom = mix(night_bottom, sunset_bottom, blend);
    sky = mix(bottom, top, sky_t);
}
```

Also scale ambient light based on sun elevation to darken the scene at night:

In `main.rs`, compute effective ambient:

```rust
let effective_ambient = if sun_y > 0.1 {
    self.ambient
} else if sun_y > -0.1 {
    let t = (sun_y + 0.1) / 0.2; // 0..1 over transition range
    self.ambient * (0.3 + 0.7 * t) // fade from 30% to 100% ambient
} else {
    self.ambient * 0.3 // night: reduced ambient
};
```

### 1.6 Add day_duration_secs to config

**Files:** `src/config.rs`, `src/main.rs`

Add `day_duration_secs: Option<f32>` to `LightingConfig` and expose as `pub day_duration_secs: f32` on `Config` (default: 600.0).

In `main.rs`, replace the `DAY_DURATION_SECS` constant usage with `config.day_duration_secs`. Store the value on `App`:

```rust
day_duration_secs: f32,
```

This allows customizing via `voxels.toml`:

```toml
[lighting]
day_duration_secs = 300  # 5-minute days
```

### 1.7 Display time-of-day in HUD

**Files:** `src/main.rs`

Add a line to the HUD text (in the `RedrawRequested` font drawing block, after the fly mode line) showing the current time of day:

```rust
let hour = (time_of_day * 24.0) as u32;
let minute = ((time_of_day * 24.0).fract() * 60.0) as u32;
let time_text = format!("Time: {:02}:{:02}", hour, minute);
font.draw_text(&mut self.draw_list, &time_text, 10.0, y, Rgba::WHITE);
```

Where `time_of_day` is the `0.0..1.0` normalized day position already computed for the shader. Noon = 12:00, midnight = 00:00.

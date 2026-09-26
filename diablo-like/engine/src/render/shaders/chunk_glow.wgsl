struct Camera {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@group(1) @binding(0) var chunk_array: texture_2d_array<f32>;
@group(1) @binding(1) var chunk_sampler: sampler;

struct InstanceInput {
    @location(0) origin: vec2<f32>,
    @location(1) layer: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) layer: f32,
}

const CHUNK_WORLD_SIZE: f32 = 16.0;
// APRON / STORED_RES and (APRON + CORE_RES) / STORED_RES — the core
// sub-rect of each stored tile; the 2-texel apron on either side exists
// only so bilinear sampling near the edge doesn't need to know about
// neighbouring chunks, not to be part of what's actually drawn.
const UV_MIN: f32 = 2.0 / 132.0;
const UV_MAX: f32 = 130.0 / 132.0;

@vertex
fn vs_main(@builtin(vertex_index) vert_idx: u32, instance: InstanceInput) -> VertexOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let corner = corners[vert_idx];
    let world_pos = instance.origin + corner * CHUNK_WORLD_SIZE;

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    out.uv = mix(vec2<f32>(UV_MIN, UV_MIN), vec2<f32>(UV_MAX, UV_MAX), corner);
    out.layer = instance.layer;
    return out;
}

// Same Tron-style glow look as the M3/M4 single-shape proof (crisp core +
// symmetric exponential halo), now sampling one layer of the chunk array
// instead of a single fullscreen texture.
const GLOW_COLOR = vec3<f32>(0.15, 0.9, 1.0);
const GLOW_FALLOFF = 2.2;
const CORE_WIDTH = 0.035;
const FILL_COLOR = vec3<f32>(0.02, 0.05, 0.06);

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let d = textureSample(chunk_array, chunk_sampler, in.uv, i32(in.layer)).r;
    let halo = exp(-abs(d) * GLOW_FALLOFF);
    let core = smoothstep(CORE_WIDTH, 0.0, abs(d));
    let inside = smoothstep(0.0, -1.5, d);
    let glow = GLOW_COLOR * (halo * 0.9 + core * 1.6);
    let color = FILL_COLOR * inside + glow;
    return vec4<f32>(color, 1.0);
}

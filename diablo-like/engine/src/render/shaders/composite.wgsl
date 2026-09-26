@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0), vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 0.0),
    );
    var out: VertexOutput;
    out.clip_position = vec4<f32>(positions[idx], 0.0, 1.0);
    out.uv = uvs[idx];
    return out;
}

// Resolves the render-scale supersample (a single bilinear tap from a
// higher-resolution offscreen target — an approximation, not a true box
// filter) and tonemaps the linear HDR-or-fallback offscreen color before
// handing it to the swapchain. The swapchain's own sRGB format then
// hardware-encodes this linear result on write; nothing here applies a
// gamma curve itself.
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(scene_tex, scene_sampler, in.uv).rgb;
    let mapped = hdr / (vec3<f32>(1.0) + hdr);
    return vec4<f32>(mapped, 1.0);
}

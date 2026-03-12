struct Uniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let clip_x = (in.pos.x / uniforms.screen_size.x) * 2.0 - 1.0;
    let clip_y = 1.0 - (in.pos.y / uniforms.screen_size.y) * 2.0;
    out.position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.color = in.color;
    out.uv = in.uv;
    return out;
}

@group(1) @binding(0)
var t_texture: texture_2d<f32>;
@group(1) @binding(1)
var t_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_texture, t_sampler, in.uv);
    return tex_color * in.color;
}

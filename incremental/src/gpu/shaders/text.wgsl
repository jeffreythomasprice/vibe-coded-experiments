struct Globals {
    view: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32, inst: Instance) -> VertexOut {
    var order = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let corner = order[index];
    let cx = f32(corner & 1u);
    let cy = f32(corner >> 1u);
    let p = inst.rect.xy + vec2<f32>(cx, cy) * inst.rect.zw;
    var out: VertexOut;
    out.position = vec4<f32>(
        p.x / globals.view.x * 2.0 - 1.0,
        1.0 - p.y / globals.view.y * 2.0,
        0.0,
        1.0,
    );
    out.uv = mix(inst.uv.xy, inst.uv.zw, vec2<f32>(cx, cy));
    out.color = inst.color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let coverage = textureSample(atlas, atlas_sampler, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}

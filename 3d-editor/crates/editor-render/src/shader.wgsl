struct Globals {
    view_proj: mat4x4<f32>,
    clip: vec4<f32>,
};

struct Material {
    tint: vec4<f32>,
    emissive: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var<uniform> material: Material;
@group(1) @binding(1) var tex: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;

struct VertexInput {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) world_z: f32,
    @location(2) uv: vec2<f32>,
};

@vertex
fn vs_main(v: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip = globals.view_proj * vec4<f32>(v.pos, 1.0);
    out.color = v.color;
    out.world_z = v.pos.z;
    out.uv = v.uv;
    return out;
}

@fragment
fn fs_main(v: VertexOutput) -> @location(0) vec4<f32> {
    if (v.world_z > globals.clip.x) {
        discard;
    }
    return v.color;
}

fn surface(v: VertexOutput) -> vec4<f32> {
    let texel = textureSample(tex, samp, v.uv);
    let rgb = texel.rgb * v.color.rgb * material.tint.rgb + material.emissive.rgb;
    return vec4<f32>(rgb, texel.a * material.tint.a);
}

@fragment
fn fs_face(v: VertexOutput) -> @location(0) vec4<f32> {
    let c = surface(v);
    if (v.world_z > globals.clip.x) {
        discard;
    }
    return vec4<f32>(c.rgb, 1.0);
}

@fragment
fn fs_face_blend(v: VertexOutput) -> @location(0) vec4<f32> {
    let c = surface(v);
    if (v.world_z > globals.clip.x) {
        discard;
    }
    return c;
}

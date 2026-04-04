struct Uniforms {
    camera_min: vec2<f32>,
    camera_size: vec2<f32>,
}

struct MaterialUniforms {
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<uniform> material: MaterialUniforms;
@group(0) @binding(2) var t_diffuse: texture_2d<f32>;
@group(0) @binding(3) var s_diffuse: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) texcoord: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) texcoord: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let ndc_x = ((in.position.x - uniforms.camera_min.x) / uniforms.camera_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - ((in.position.y - uniforms.camera_min.y) / uniforms.camera_size.y) * 2.0;
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.texcoord = in.texcoord;
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_diffuse, s_diffuse, in.texcoord);
    return tex_color * in.color * material.color;
}

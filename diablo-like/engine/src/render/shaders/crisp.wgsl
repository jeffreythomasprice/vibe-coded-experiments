struct Camera {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 0.0, 1.0);
    return out;
}

@fragment
fn fs_fill() -> @location(0) vec4<f32> {
    return vec4<f32>(0.72, 0.75, 0.78, 1.0);
}

@fragment
fn fs_stroke() -> @location(0) vec4<f32> {
    return vec4<f32>(0.15, 0.9, 1.0, 1.0);
}

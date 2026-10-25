struct Globals {
    view: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;

struct Vertex {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(v: Vertex) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(
        v.position.x / globals.view.x * 2.0 - 1.0,
        1.0 - v.position.y / globals.view.y * 2.0,
        0.0,
        1.0,
    );
    out.color = v.color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return in.color;
}

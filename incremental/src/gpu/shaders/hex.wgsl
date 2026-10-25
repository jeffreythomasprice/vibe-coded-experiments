struct Globals {
    view: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;

struct Instance {
    @location(0) center: vec2<f32>,
    @location(1) radius_width: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) radius_width: vec2<f32>,
    @location(2) color: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32, inst: Instance) -> VertexOut {
    var order = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let corner = order[index];
    let sx = f32(i32(corner & 1u) * 2 - 1);
    let sy = f32(i32(corner >> 1u) * 2 - 1);
    let extent = inst.radius_width.x + inst.radius_width.y + 1.0;
    let local = vec2<f32>(sx, sy) * extent;
    let p = inst.center + local;
    var out: VertexOut;
    out.position = vec4<f32>(
        p.x / globals.view.x * 2.0 - 1.0,
        1.0 - p.y / globals.view.y * 2.0,
        0.0,
        1.0,
    );
    out.local = local;
    out.radius_width = inst.radius_width;
    out.color = inst.color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let scale = globals.view.z;
    let m = max(
        abs(in.local.x),
        max(
            abs(dot(in.local, vec2<f32>(0.5, 0.8660254))),
            abs(dot(in.local, vec2<f32>(-0.5, 0.8660254))),
        ),
    );
    let inside = in.radius_width.x * 0.8660254 - m;
    var coverage: f32;
    if (in.radius_width.y <= 0.0) {
        coverage = clamp(inside * scale + 0.5, 0.0, 1.0);
    } else {
        coverage = clamp((in.radius_width.y * 0.5 - abs(inside)) * scale + 0.5, 0.0, 1.0);
    }
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}

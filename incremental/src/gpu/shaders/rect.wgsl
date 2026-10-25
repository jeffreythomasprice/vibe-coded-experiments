struct Globals {
    view: vec4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) fill: vec4<f32>,
    @location(2) border: vec4<f32>,
    @location(3) params: vec4<f32>,
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) border: vec4<f32>,
    @location(4) params: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32, inst: Instance) -> VertexOut {
    var order = array<u32, 6>(0u, 1u, 2u, 2u, 1u, 3u);
    let corner = order[index];
    let sx = f32(i32(corner & 1u) * 2 - 1);
    let sy = f32(i32(corner >> 1u) * 2 - 1);
    let half_size = inst.rect.zw * 0.5;
    let local = vec2<f32>(sx, sy) * (half_size + vec2<f32>(1.0, 1.0));
    let p = inst.rect.xy + half_size + local;
    var out: VertexOut;
    out.position = vec4<f32>(
        p.x / globals.view.x * 2.0 - 1.0,
        1.0 - p.y / globals.view.y * 2.0,
        0.0,
        1.0,
    );
    out.local = local;
    out.half_size = half_size;
    out.fill = inst.fill;
    out.border = inst.border;
    out.params = inst.params.xy;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let scale = globals.view.z;
    let border_width = in.params.x;
    let radius = min(in.params.y, min(in.half_size.x, in.half_size.y));
    let q = abs(in.local) - (in.half_size - vec2<f32>(radius, radius));
    let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - radius;
    let outer = clamp(0.5 - d * scale, 0.0, 1.0);
    var border_mix = 0.0;
    if (border_width > 0.0) {
        border_mix = clamp(0.5 + (d + border_width) * scale, 0.0, 1.0);
    }
    let color = mix(in.fill, in.border, border_mix);
    return vec4<f32>(color.rgb, color.a * outer);
}

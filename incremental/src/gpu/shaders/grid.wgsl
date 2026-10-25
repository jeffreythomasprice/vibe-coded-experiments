struct Grid {
    view: vec4<f32>,
    params: vec4<f32>,
    fill: vec4<f32>,
    border: vec4<f32>,
}

@group(0) @binding(0) var<uniform> grid: Grid;

const SQRT3: f32 = 1.7320508;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(index & 1u) * 4 - 1);
    let y = f32(i32(index >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn cube_round(q: f32, r: f32) -> vec2<f32> {
    let s = -q - r;
    var rq = round(q);
    var rr = round(r);
    let rs = round(s);
    let dq = abs(rq - q);
    let dr = abs(rr - r);
    let ds = abs(rs - s);
    if (dq > dr && dq > ds) {
        rq = -rr - rs;
    } else if (dr > ds) {
        rr = -rq - rs;
    }
    return vec2<f32>(rq, rr);
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let viewport = grid.view.zw;
    let zoom = grid.params.x;
    let size = grid.params.y;
    let border_width = grid.params.z;
    let scale = grid.params.w;

    let logical = frag.xy / scale;
    let world = (logical - viewport * 0.5) / zoom + grid.view.xy;

    let r = world.y / (size * 1.5);
    let q = world.x / (size * SQRT3) - r * 0.5;
    let axial = cube_round(q, r);
    let center = vec2<f32>(
        size * SQRT3 * (axial.x + axial.y * 0.5),
        size * 1.5 * axial.y,
    );
    let d = world - center;
    let m = max(
        abs(d.x),
        max(abs(dot(d, vec2<f32>(0.5, 0.8660254))), abs(dot(d, vec2<f32>(-0.5, 0.8660254)))),
    );
    let edge_px = (size * 0.8660254 - m) * zoom;
    let coverage = clamp((border_width * 0.5 - edge_px) * scale + 0.5, 0.0, 1.0);
    return vec4<f32>(mix(grid.fill.rgb, grid.border.rgb, coverage), 1.0);
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    // Full-screen triangle trick: 3 vertices cover the entire screen
    let x = f32(i32(vi & 1u)) * 4.0 - 1.0;
    let y = f32(i32(vi >> 1u)) * 4.0 - 1.0;
    var out: VertexOutput;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

struct Camera {
    origin: vec3<f32>,
    _pad0: f32,
    forward: vec3<f32>,
    _pad1: f32,
    right: vec3<f32>,
    _pad2: f32,
    up: vec3<f32>,
    fov: f32,
    aspect: f32,
    _pad3a: f32,
    _pad3b: f32,
    _pad3c: f32,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<storage, read> voxels: array<u32>;

fn ray_box(ro: vec3<f32>, rd: vec3<f32>, box_min: vec3<f32>, box_max: vec3<f32>) -> vec2<f32> {
    let inv_rd = 1.0 / rd;
    let t1 = (box_min - ro) * inv_rd;
    let t2 = (box_max - ro) * inv_rd;
    let tmin_v = min(t1, t2);
    let tmax_v = max(t1, t2);
    let tmin = max(max(tmin_v.x, tmin_v.y), tmin_v.z);
    let tmax = min(min(tmax_v.x, tmax_v.y), tmax_v.z);
    return vec2<f32>(tmin, tmax);
}

fn get_voxel(x: i32, y: i32, z: i32) -> u32 {
    if x < 0 || x >= 16 || y < 0 || y >= 16 || z < 0 || z >= 16 {
        return 0u;
    }
    let idx = u32(x) + u32(y) * 16u + u32(z) * 256u;
    let byte_idx = idx / 4u;
    let shift = (idx % 4u) * 8u;
    return (voxels[byte_idx] >> shift) & 0xFFu;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let half_h = tan(camera.fov * 0.5);
    let half_w = half_h * camera.aspect;
    let ndc = vec2<f32>(
        (in.uv.x * 2.0 - 1.0) * half_w,
        (1.0 - in.uv.y * 2.0) * half_h,
    );

    let rd = normalize(camera.forward + ndc.x * camera.right + ndc.y * camera.up);
    let ro = camera.origin;

    let chunk_min = vec3<f32>(-8.0, -8.0, -8.0);
    let chunk_max = vec3<f32>(8.0, 8.0, 8.0);
    let t = ray_box(ro, rd, chunk_min, chunk_max);

    if t.x > t.y || t.y < 0.0 {
        let sky_t = rd.y * 0.5 + 0.5;
        let sky = mix(vec3<f32>(0.8, 0.85, 0.9), vec3<f32>(0.3, 0.5, 0.8), sky_t);
        return vec4<f32>(sky, 1.0);
    }

    // DDA ray marching
    let entry_t = max(t.x, 0.0) + 0.001;
    let entry = ro + rd * entry_t;
    let local = entry - chunk_min;

    var voxel = vec3<i32>(
        clamp(i32(floor(local.x)), 0, 15),
        clamp(i32(floor(local.y)), 0, 15),
        clamp(i32(floor(local.z)), 0, 15),
    );

    let step = vec3<i32>(
        select(-1, 1, rd.x >= 0.0),
        select(-1, 1, rd.y >= 0.0),
        select(-1, 1, rd.z >= 0.0),
    );

    let inv_rd = 1.0 / rd;

    let next_boundary = vec3<f32>(
        chunk_min.x + f32(select(voxel.x, voxel.x + 1, rd.x >= 0.0)),
        chunk_min.y + f32(select(voxel.y, voxel.y + 1, rd.y >= 0.0)),
        chunk_min.z + f32(select(voxel.z, voxel.z + 1, rd.z >= 0.0)),
    );
    var t_max = (next_boundary - ro) * inv_rd;

    let t_delta = abs(inv_rd);

    var normal = vec3<f32>(0.0, 0.0, 0.0);

    for (var i = 0; i < 128; i = i + 1) {
        let v = get_voxel(voxel.x, voxel.y, voxel.z);
        if v != 0u {
            // Hit a solid voxel
            let light_dir = normalize(vec3<f32>(1.0, 2.0, 3.0));
            let diffuse = max(dot(normal, light_dir), 0.0);
            let ambient = 0.15;
            let color = vec3<f32>(0.9, 0.3, 0.2) * (ambient + diffuse * 0.85);
            return vec4<f32>(color, 1.0);
        }

        // Step to the next voxel along the axis with smallest t_max
        if t_max.x < t_max.y && t_max.x < t_max.z {
            voxel.x = voxel.x + step.x;
            t_max.x = t_max.x + t_delta.x;
            normal = vec3<f32>(f32(-step.x), 0.0, 0.0);
        } else if t_max.y < t_max.z {
            voxel.y = voxel.y + step.y;
            t_max.y = t_max.y + t_delta.y;
            normal = vec3<f32>(0.0, f32(-step.y), 0.0);
        } else {
            voxel.z = voxel.z + step.z;
            t_max.z = t_max.z + t_delta.z;
            normal = vec3<f32>(0.0, 0.0, f32(-step.z));
        }

        if voxel.x < 0 || voxel.x >= 16 || voxel.y < 0 || voxel.y >= 16 || voxel.z < 0 || voxel.z >= 16 {
            break;
        }
    }

    // Miss — sky gradient
    let sky_t = rd.y * 0.5 + 0.5;
    let sky = mix(vec3<f32>(0.8, 0.85, 0.9), vec3<f32>(0.3, 0.5, 0.8), sky_t);
    return vec4<f32>(sky, 1.0);
}

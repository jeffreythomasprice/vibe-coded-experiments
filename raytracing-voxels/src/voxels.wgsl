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

struct ChunkInfo {
    world_offset: vec3<f32>,
    data_offset: u32,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<storage, read> voxels: array<u32>;
@group(1) @binding(1) var<storage, read> chunks: array<ChunkInfo>;
@group(1) @binding(2) var<uniform> chunk_count: u32;
struct BvhNode {
    aabb_min: vec3<f32>,
    right_or_chunk: u32,
    aabb_max: vec3<f32>,
    _padding: u32,
};

@group(1) @binding(3) var<storage, read> bvh_nodes: array<BvhNode>;
@group(1) @binding(4) var<uniform> bvh_node_count: u32;
@group(2) @binding(0) var atlas_tex: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;
@group(2) @binding(2) var<storage, read> uv_map: array<vec4<f32>>;

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

fn get_voxel(x: i32, y: i32, z: i32, data_offset: u32) -> u32 {
    if x < 0 || x >= 16 || y < 0 || y >= 16 || z < 0 || z >= 16 {
        return 0u;
    }
    let idx = u32(x) + u32(y) * 16u + u32(z) * 256u;
    let byte_idx = data_offset + idx / 4u;
    let shift = (idx % 4u) * 8u;
    return (voxels[byte_idx] >> shift) & 0xFFu;
}

struct MarchResult {
    hit: bool,
    color: vec4<f32>,
    t: f32,
};

fn march_chunk(ro: vec3<f32>, rd: vec3<f32>, chunk_min: vec3<f32>,
               data_offset: u32, max_t: f32) -> MarchResult {
    var result: MarchResult;
    result.hit = false;
    result.color = vec4<f32>(0.0);
    result.t = max_t;

    let chunk_max = chunk_min + vec3<f32>(16.0, 16.0, 16.0);

    let t = ray_box(ro, rd, chunk_min, chunk_max);
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
    var t_max_dda = (next_boundary - ro) * inv_rd;

    let t_delta = abs(inv_rd);

    // Compute entry face normal
    var normal: vec3<f32>;
    if t.x > 0.0 {
        let t1 = (chunk_min - ro) * inv_rd;
        let t2 = (chunk_max - ro) * inv_rd;
        let tmin_v = min(t1, t2);
        if tmin_v.x >= tmin_v.y && tmin_v.x >= tmin_v.z {
            normal = vec3<f32>(select(1.0, -1.0, rd.x >= 0.0), 0.0, 0.0);
        } else if tmin_v.y >= tmin_v.z {
            normal = vec3<f32>(0.0, select(1.0, -1.0, rd.y >= 0.0), 0.0);
        } else {
            normal = vec3<f32>(0.0, 0.0, select(1.0, -1.0, rd.z >= 0.0));
        }
    } else {
        normal = vec3<f32>(0.0, 0.0, 0.0);
    }

    for (var i = 0; i < 128; i = i + 1) {
        let v = get_voxel(voxel.x, voxel.y, voxel.z, data_offset);
        if v != 0u {
            var t_hit: f32;
            if abs(normal.x) > 0.5 {
                t_hit = t_max_dda.x - t_delta.x;
            } else if abs(normal.y) > 0.5 {
                t_hit = t_max_dda.y - t_delta.y;
            } else if abs(normal.z) > 0.5 {
                t_hit = t_max_dda.z - t_delta.z;
            } else {
                t_hit = entry_t;
            }

            if t_hit >= max_t {
                break;
            }

            let hit_pos = ro + rd * t_hit;
            let local_hit = hit_pos - chunk_min - vec3<f32>(f32(voxel.x), f32(voxel.y), f32(voxel.z));

            var face_uv: vec2<f32>;
            if abs(normal.x) > 0.5 {
                face_uv = vec2<f32>(local_hit.z, local_hit.y);
            } else if abs(normal.y) > 0.5 {
                face_uv = vec2<f32>(local_hit.x, local_hit.z);
            } else {
                face_uv = vec2<f32>(local_hit.x, local_hit.y);
            }
            face_uv = clamp(face_uv, vec2<f32>(0.0), vec2<f32>(1.0));

            let uv_rect = uv_map[v];
            let atlas_uv = uv_rect.xy + face_uv * (uv_rect.zw - uv_rect.xy);

            let tex_color = textureSampleLevel(atlas_tex, atlas_sampler, atlas_uv, 0.0);

            if tex_color.a >= 0.5 {
                let light_dir = normalize(vec3<f32>(1.0, 2.0, 3.0));
                let diffuse = max(dot(normal, light_dir), 0.0);
                let ambient = 0.15;
                let color = tex_color.rgb * (ambient + diffuse * 0.85);

                result.hit = true;
                result.color = vec4<f32>(color, 1.0);
                result.t = t_hit;
                return result;
            }
        }

        if t_max_dda.x < t_max_dda.y && t_max_dda.x < t_max_dda.z {
            voxel.x = voxel.x + step.x;
            t_max_dda.x = t_max_dda.x + t_delta.x;
            normal = vec3<f32>(f32(-step.x), 0.0, 0.0);
        } else if t_max_dda.y < t_max_dda.z {
            voxel.y = voxel.y + step.y;
            t_max_dda.y = t_max_dda.y + t_delta.y;
            normal = vec3<f32>(0.0, f32(-step.y), 0.0);
        } else {
            voxel.z = voxel.z + step.z;
            t_max_dda.z = t_max_dda.z + t_delta.z;
            normal = vec3<f32>(0.0, 0.0, f32(-step.z));
        }

        if voxel.x < 0 || voxel.x >= 16 || voxel.y < 0 || voxel.y >= 16 || voxel.z < 0 || voxel.z >= 16 {
            break;
        }
    }

    return result;
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

    // BVH traversal
    var stack: array<u32, 32>;
    var stack_ptr: i32 = 0;
    var closest_t: f32 = 1e30;
    var result_color: vec4<f32>;
    var hit_anything: bool = false;

    if bvh_node_count > 0u {
        stack[0] = 0u;
        stack_ptr = 1;
    }

    while stack_ptr > 0 {
        stack_ptr -= 1;
        let node_idx = stack[stack_ptr];
        let node = bvh_nodes[node_idx];

        let t = ray_box(ro, rd, node.aabb_min, node.aabb_max);
        if t.x > t.y || t.y < 0.0 || t.x > closest_t {
            continue;
        }

        let is_leaf = (node.right_or_chunk & 0x80000000u) != 0u;
        if is_leaf {
            let chunk_idx = node.right_or_chunk & 0x7FFFFFFFu;
            let mr = march_chunk(ro, rd, chunks[chunk_idx].world_offset,
                                     chunks[chunk_idx].data_offset, closest_t);
            if mr.hit {
                closest_t = mr.t;
                result_color = mr.color;
                hit_anything = true;
            }
        } else {
            let left_idx = node_idx + 1u;
            let right_idx = node.right_or_chunk;

            // Push farther child first so nearer child is popped first
            let t_left = ray_box(ro, rd, bvh_nodes[left_idx].aabb_min, bvh_nodes[left_idx].aabb_max);
            let t_right = ray_box(ro, rd, bvh_nodes[right_idx].aabb_min, bvh_nodes[right_idx].aabb_max);

            let left_valid = t_left.x <= t_left.y && t_left.y >= 0.0 && t_left.x <= closest_t;
            let right_valid = t_right.x <= t_right.y && t_right.y >= 0.0 && t_right.x <= closest_t;

            if left_valid {
                if right_valid {
                    if t_left.x < t_right.x {
                        stack[stack_ptr] = right_idx; stack_ptr += 1;
                        stack[stack_ptr] = left_idx; stack_ptr += 1;
                    } else {
                        stack[stack_ptr] = left_idx; stack_ptr += 1;
                        stack[stack_ptr] = right_idx; stack_ptr += 1;
                    }
                } else {
                    stack[stack_ptr] = left_idx; stack_ptr += 1;
                }
            } else if right_valid {
                stack[stack_ptr] = right_idx; stack_ptr += 1;
            }
        }
    }

    if hit_anything {
        return result_color;
    }

    // Miss — sky gradient
    let sky_t = rd.y * 0.5 + 0.5;
    let sky = mix(vec3<f32>(0.8, 0.85, 0.9), vec3<f32>(0.3, 0.5, 0.8), sky_t);
    return vec4<f32>(sky, 1.0);
}

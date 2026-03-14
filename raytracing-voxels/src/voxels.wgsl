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

struct MeshTriangle {
    v0: vec3<f32>,
    color_r: f32,
    v1: vec3<f32>,
    color_g: f32,
    v2: vec3<f32>,
    color_b: f32,
};

struct MeshInfo {
    tri_offset: u32,
    tri_count: u32,
    _pad0: u32,
    _pad1: u32,
};

@group(1) @binding(5) var<storage, read> mesh_triangles: array<MeshTriangle>;
@group(1) @binding(6) var<storage, read> mesh_infos: array<MeshInfo>;

@group(2) @binding(0) var atlas_tex: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;
@group(2) @binding(2) var<storage, read> uv_map: array<vec4<f32>>;

struct InteractionState {
    highlight_pos: vec3<f32>,
    highlight_active: u32,
    break_pos: vec3<f32>,
    break_progress: f32,
    is_submerged: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(3) @binding(0) var<uniform> interaction: InteractionState;

struct LightingUniforms {
    sun_dir: vec3<f32>,
    ambient: f32,
    sun_color: vec3<f32>,
    sun_intensity: f32,
    time_of_day: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

struct PointLight {
    position: vec3<f32>,
    radius: f32,
    color: vec3<f32>,
    _padding: f32,
};

@group(4) @binding(0) var<uniform> lighting: LightingUniforms;
@group(4) @binding(1) var<storage, read> point_lights: array<PointLight>;
@group(4) @binding(2) var<uniform> point_light_count: u32;

const WATER_ID: u32 = 7u;

fn is_wireframe_edge(face_uv: vec2<f32>, thickness: f32) -> bool {
    return face_uv.x < thickness || face_uv.x > (1.0 - thickness)
        || face_uv.y < thickness || face_uv.y > (1.0 - thickness);
}

fn hash1(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn hash2(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453),
        fract(sin(dot(p, vec2<f32>(269.5, 183.3))) * 43758.5453),
    );
}

struct VoronoiResult {
    edge_dist: f32,
    cell_hash: f32,
};

fn voronoi_edges(uv: vec2<f32>, scale: f32, seed: f32) -> VoronoiResult {
    let st = uv * scale;
    let i_st = floor(st);
    let f_st = fract(st);

    var d1: f32 = 1e10;
    var d2: f32 = 1e10;
    var nearest_cell: vec2<f32>;

    for (var y: i32 = -1; y <= 1; y += 1) {
        for (var x: i32 = -1; x <= 1; x += 1) {
            let neighbor = vec2<f32>(f32(x), f32(y));
            let cell = i_st + neighbor;
            let point = neighbor + hash2(cell + vec2<f32>(seed, seed * 0.7)) - f_st;
            let dist = dot(point, point);
            if dist < d1 {
                d2 = d1;
                d1 = dist;
                nearest_cell = cell;
            } else if dist < d2 {
                d2 = dist;
            }
        }
    }

    var result: VoronoiResult;
    result.edge_dist = sqrt(d2) - sqrt(d1);
    result.cell_hash = hash1(nearest_cell + vec2<f32>(seed));
    return result;
}

fn crack_overlay(face_uv: vec2<f32>, progress: f32, seed: f32) -> f32 {
    let v1 = voronoi_edges(face_uv, 3.0, seed);
    let v2 = voronoi_edges(face_uv, 6.0, seed + 100.0);

    let crack_width = mix(0.02, 0.15, progress * progress);
    var darken: f32 = 0.0;

    // Primary cracks: appear progressively based on cell hash
    let threshold1 = v1.cell_hash * 0.8;
    if progress > threshold1 {
        let crack1 = smoothstep(crack_width, 0.0, v1.edge_dist);
        darken += crack1 * 0.6;
    }

    // Secondary fine cracks: appear later in the break
    let threshold2 = v2.cell_hash * 0.6 + 0.3;
    if progress > threshold2 {
        let crack2 = smoothstep(crack_width * 0.7, 0.0, v2.edge_dist);
        darken += crack2 * 0.4;
    }

    // Subtle surface darkening for dust/damage feel
    darken += progress * progress * 0.3;

    return min(darken, 0.8);
}

fn wireframe_box(ro: vec3<f32>, rd: vec3<f32>, box_min: vec3<f32>, closest_t: f32, color_in: vec4<f32>, wire_color: vec3<f32>, thickness: f32) -> vec4<f32> {
    let box_max = box_min + vec3<f32>(1.0, 1.0, 1.0);
    let t = ray_box(ro, rd, box_min, box_max);
    let entry_t = t.x;
    if entry_t > t.y || t.y < 0.0 || entry_t > closest_t {
        return color_in;
    }
    let hit_point = ro + rd * max(entry_t, 0.0001);
    let local = hit_point - box_min;
    let inv_rd = 1.0 / rd;
    let t1 = (box_min - ro) * inv_rd;
    let t2 = (box_max - ro) * inv_rd;
    let tmin_v = min(t1, t2);
    var face_uv: vec2<f32>;
    if tmin_v.x >= tmin_v.y && tmin_v.x >= tmin_v.z {
        face_uv = vec2<f32>(fract(local.z), fract(local.y));
    } else if tmin_v.y >= tmin_v.z {
        face_uv = vec2<f32>(fract(local.x), fract(local.z));
    } else {
        face_uv = vec2<f32>(fract(local.x), fract(local.y));
    }
    face_uv = clamp(face_uv, vec2<f32>(0.0), vec2<f32>(1.0));
    if is_wireframe_edge(face_uv, thickness) {
        let alpha = 0.6;
        return vec4<f32>(mix(color_in.rgb, wire_color, alpha), 1.0);
    }
    return color_in;
}

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

struct TriHitResult {
    hit: bool,
    t: f32,
    normal: vec3<f32>,
};

fn ray_triangle(ro: vec3<f32>, rd: vec3<f32>, v0: vec3<f32>, v1: vec3<f32>, v2: vec3<f32>, max_t: f32) -> TriHitResult {
    var result: TriHitResult;
    result.hit = false;
    result.t = max_t;
    result.normal = vec3<f32>(0.0);

    let e1 = v1 - v0;
    let e2 = v2 - v0;
    let h = cross(rd, e2);
    let a = dot(e1, h);

    // Ray is parallel to triangle
    if abs(a) < 1e-8 {
        return result;
    }

    let f = 1.0 / a;
    let s = ro - v0;
    let u = f * dot(s, h);

    if u < 0.0 || u > 1.0 {
        return result;
    }

    let q = cross(s, e1);
    let v = f * dot(rd, q);

    if v < 0.0 || u + v > 1.0 {
        return result;
    }

    let t = f * dot(e2, q);

    if t > 0.001 && t < max_t {
        result.hit = true;
        result.t = t;
        var n = normalize(cross(e1, e2));
        // Orient normal to face the ray
        if dot(n, rd) > 0.0 {
            n = -n;
        }
        result.normal = n;
    }

    return result;
}

fn intersect_mesh_voxel(ro: vec3<f32>, rd: vec3<f32>, voxel_world_min: vec3<f32>, mesh_type: u32, max_t: f32) -> MarchResult {
    var result: MarchResult;
    result.hit = false;
    result.color = vec4<f32>(0.0);
    result.t = max_t;
    result.normal = vec3<f32>(0.0);
    result.water_hit = false;
    result.water_color = vec4<f32>(0.0);
    result.water_t = 1e30;

    let local_ro = ro - voxel_world_min;
    let info = mesh_infos[mesh_type];
    let tri_end = info.tri_offset + info.tri_count;

    for (var i = info.tri_offset; i < tri_end; i += 1u) {
        let tri = mesh_triangles[i];
        let tri_hit = ray_triangle(local_ro, rd, tri.v0, tri.v1, tri.v2, result.t);
        if tri_hit.hit {
            result.hit = true;
            result.t = tri_hit.t;
            result.normal = tri_hit.normal;
            result.color = vec4<f32>(tri.color_r, tri.color_g, tri.color_b, 1.0);
        }
    }

    return result;
}

struct MarchResult {
    hit: bool,
    color: vec4<f32>,
    t: f32,
    normal: vec3<f32>,
    water_hit: bool,
    water_color: vec4<f32>,
    water_t: f32,
};

fn march_chunk(ro: vec3<f32>, rd: vec3<f32>, chunk_min: vec3<f32>,
               data_offset: u32, max_t: f32) -> MarchResult {
    var result: MarchResult;
    result.hit = false;
    result.color = vec4<f32>(0.0);
    result.t = max_t;
    result.water_hit = false;
    result.water_color = vec4<f32>(0.0);
    result.water_t = 1e30;

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

            if v >= 128u {
                // Mesh voxel: ray-trace triangles
                let mesh_type = v - 128u;
                let voxel_world = chunk_min + vec3<f32>(f32(voxel.x), f32(voxel.y), f32(voxel.z));
                let mesh_result = intersect_mesh_voxel(ro, rd, voxel_world, mesh_type, max_t);
                if mesh_result.hit {
                    result.hit = true;
                    result.color = mesh_result.color;
                    result.t = mesh_result.t;
                    result.normal = mesh_result.normal;
                    return result;
                }
                // No triangle hit — ray passes through, continue DDA
            } else if v == WATER_ID {
                // Water: record but continue marching
                if !result.water_hit {
                    var water_t_hit: f32;
                    if abs(normal.y) > 0.5 && normal.y < 0.0 {
                        // Top face: lower surface by 0.15
                        let voxel_world_y = chunk_min.y + f32(voxel.y);
                        water_t_hit = ((voxel_world_y + 0.85) - ro.y) / rd.y;
                    } else {
                        water_t_hit = t_hit;
                    }
                    if water_t_hit < max_t {
                        let water_hit_pos = ro + rd * water_t_hit;
                        let water_local = water_hit_pos - chunk_min - vec3<f32>(f32(voxel.x), f32(voxel.y), f32(voxel.z));
                        var water_face_uv: vec2<f32>;
                        if abs(normal.x) > 0.5 {
                            water_face_uv = vec2<f32>(water_local.z, water_local.y);
                        } else if abs(normal.y) > 0.5 {
                            water_face_uv = vec2<f32>(water_local.x, water_local.z);
                        } else {
                            water_face_uv = vec2<f32>(water_local.x, water_local.y);
                        }
                        water_face_uv = clamp(water_face_uv, vec2<f32>(0.0), vec2<f32>(1.0));
                        let water_uv_rect = uv_map[WATER_ID];
                        let water_atlas_uv = water_uv_rect.xy + water_face_uv * (water_uv_rect.zw - water_uv_rect.xy);
                        let water_tex = textureSampleLevel(atlas_tex, atlas_sampler, water_atlas_uv, 0.0);
                        result.water_hit = true;
                        result.water_color = water_tex;
                        result.water_t = water_t_hit;
                    }
                }
                // Continue DDA - don't return
            } else {
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
                    result.hit = true;
                    result.color = tex_color;
                    result.t = t_hit;
                    result.normal = normal;
                    return result;
                }
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

fn march_chunk_occlusion(ro: vec3<f32>, rd: vec3<f32>, chunk_min: vec3<f32>,
                         data_offset: u32, max_t: f32) -> bool {
    let chunk_max = chunk_min + vec3<f32>(16.0, 16.0, 16.0);

    let t = ray_box(ro, rd, chunk_min, chunk_max);
    if t.x > t.y || t.y < 0.0 || t.x > max_t {
        return false;
    }
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

    for (var i = 0; i < 128; i = i + 1) {
        let v = get_voxel(voxel.x, voxel.y, voxel.z, data_offset);
        if v != 0u && v < 128u {
            return true;
        }

        if t_max_dda.x < t_max_dda.y && t_max_dda.x < t_max_dda.z {
            if t_max_dda.x > max_t { break; }
            voxel.x = voxel.x + step.x;
            t_max_dda.x = t_max_dda.x + t_delta.x;
        } else if t_max_dda.y < t_max_dda.z {
            if t_max_dda.y > max_t { break; }
            voxel.y = voxel.y + step.y;
            t_max_dda.y = t_max_dda.y + t_delta.y;
        } else {
            if t_max_dda.z > max_t { break; }
            voxel.z = voxel.z + step.z;
            t_max_dda.z = t_max_dda.z + t_delta.z;
        }

        if voxel.x < 0 || voxel.x >= 16 || voxel.y < 0 || voxel.y >= 16 || voxel.z < 0 || voxel.z >= 16 {
            break;
        }
    }

    return false;
}

fn is_occluded(origin: vec3<f32>, direction: vec3<f32>, max_dist: f32) -> bool {
    var stack: array<u32, 32>;
    var stack_ptr: i32 = 0;

    if bvh_node_count > 0u {
        stack[0] = 0u;
        stack_ptr = 1;
    }

    while stack_ptr > 0 {
        stack_ptr -= 1;
        let node_idx = stack[stack_ptr];
        let node = bvh_nodes[node_idx];

        let t = ray_box(origin, direction, node.aabb_min, node.aabb_max);
        if t.x > t.y || t.y < 0.0 || t.x > max_dist {
            continue;
        }

        let is_leaf = (node.right_or_chunk & 0x80000000u) != 0u;
        if is_leaf {
            let chunk_idx = node.right_or_chunk & 0x7FFFFFFFu;
            if march_chunk_occlusion(origin, direction, chunks[chunk_idx].world_offset,
                                     chunks[chunk_idx].data_offset, max_dist) {
                return true;
            }
        } else {
            let left_idx = node_idx + 1u;
            let right_idx = node.right_or_chunk;

            if stack_ptr < 30 {
                let t_left = ray_box(origin, direction, bvh_nodes[left_idx].aabb_min, bvh_nodes[left_idx].aabb_max);
                let t_right = ray_box(origin, direction, bvh_nodes[right_idx].aabb_min, bvh_nodes[right_idx].aabb_max);

                let left_valid = t_left.x <= t_left.y && t_left.y >= 0.0 && t_left.x <= max_dist;
                let right_valid = t_right.x <= t_right.y && t_right.y >= 0.0 && t_right.x <= max_dist;

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
    }

    return false;
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
    var result_normal: vec3<f32> = vec3<f32>(0.0);
    var hit_anything: bool = false;
    var water_hit_any: bool = false;
    var water_color_final: vec4<f32> = vec4<f32>(0.0);
    var water_t_final: f32 = 1e30;

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
                result_normal = mr.normal;
                hit_anything = true;
            }
            if mr.water_hit && mr.water_t < water_t_final {
                water_hit_any = true;
                water_color_final = mr.water_color;
                water_t_final = mr.water_t;
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

    var final_color: vec4<f32>;
    if hit_anything {
        let hit_pos = ro + rd * closest_t;
        let shadow_origin = hit_pos + result_normal * 0.01;

        var total_light = vec3<f32>(lighting.ambient);

        let sun_diffuse = max(dot(result_normal, lighting.sun_dir), 0.0);
        if sun_diffuse > 0.0 {
            let in_shadow = is_occluded(shadow_origin, lighting.sun_dir, 128.0);
            if !in_shadow {
                total_light += lighting.sun_color * lighting.sun_intensity * sun_diffuse;
            }
        }

        let pc = point_light_count;
        for (var i = 0u; i < pc; i += 1u) {
            let light = point_lights[i];
            let to_light = light.position - hit_pos;
            let dist = length(to_light);
            if dist > light.radius { continue; }
            let dir = to_light / dist;
            let ndotl = max(dot(result_normal, dir), 0.0);
            if ndotl <= 0.0 { continue; }
            let attenuation = max(1.0 - (dist * dist) / (light.radius * light.radius), 0.0);
            total_light += light.color * ndotl * attenuation;
        }

        final_color = vec4<f32>(result_color.rgb * total_light, 1.0);
    } else {
        let sun_elev = lighting.sun_dir.y;
        let sky_t = rd.y * 0.5 + 0.5;

        let day_top = vec3<f32>(0.3, 0.5, 0.8);
        let day_bottom = vec3<f32>(0.8, 0.85, 0.9);
        let sunset_top = vec3<f32>(0.2, 0.15, 0.4);
        let sunset_bottom = vec3<f32>(0.9, 0.4, 0.2);
        let night_top = vec3<f32>(0.02, 0.02, 0.06);
        let night_bottom = vec3<f32>(0.05, 0.05, 0.1);

        var sky: vec3<f32>;
        if sun_elev > 0.2 {
            sky = mix(day_bottom, day_top, sky_t);
        } else if sun_elev > -0.1 {
            let blend = smoothstep(-0.1, 0.2, sun_elev);
            let top = mix(sunset_top, day_top, blend);
            let bottom = mix(sunset_bottom, day_bottom, blend);
            sky = mix(bottom, top, sky_t);
        } else {
            let blend = smoothstep(-0.3, -0.1, sun_elev);
            let top = mix(night_top, sunset_top, blend);
            let bottom = mix(night_bottom, sunset_bottom, blend);
            sky = mix(bottom, top, sky_t);
        }

        let sun_dot = dot(rd, lighting.sun_dir);
        let sun_angular_size = 0.995;
        if sun_dot > sun_angular_size && rd.y > 0.0 {
            let sun_edge = smoothstep(sun_angular_size, sun_angular_size + 0.003, sun_dot);
            let sun_glow = vec3<f32>(1.0, 0.95, 0.8) * sun_edge * 2.0;
            final_color = vec4<f32>(min(sun_glow, vec3<f32>(1.0)), 1.0);
        } else if sun_dot > 0.98 && rd.y > 0.0 {
            let glow_strength = smoothstep(0.98, sun_angular_size, sun_dot);
            let glow = vec3<f32>(1.0, 0.9, 0.7) * glow_strength * 0.3;
            final_color = vec4<f32>(sky + glow, 1.0);
        } else {
            final_color = vec4<f32>(sky, 1.0);
        }
    }

    // Water transparency blending
    if water_hit_any && water_t_final < closest_t {
        let water_alpha = water_color_final.a;
        final_color = vec4<f32>(
            mix(final_color.rgb, water_color_final.rgb, water_alpha),
            1.0
        );
    }

    // Break progress overlay on the voxel being broken (ray-box intersection approach)
    if interaction.break_progress > 0.0 && hit_anything {
        let bp = interaction.break_pos;
        let box_max = bp + vec3<f32>(1.0, 1.0, 1.0);
        let t = ray_box(ro, rd, bp, box_max);
        // Check ray hits this box AND it's close to the closest voxel hit
        if t.x <= t.y && t.y >= 0.0 && t.x <= closest_t + 0.01 {
            let hit_point = ro + rd * max(t.x, 0.0001);
            let local = hit_point - bp;
            let inv_rd = 1.0 / rd;
            let t1 = (bp - ro) * inv_rd;
            let t2 = (box_max - ro) * inv_rd;
            let tmin_v = min(t1, t2);
            var face_uv: vec2<f32>;
            var face_index: f32;
            if tmin_v.x >= tmin_v.y && tmin_v.x >= tmin_v.z {
                face_uv = vec2<f32>(fract(local.z), fract(local.y));
                face_index = select(1.0, 0.0, rd.x >= 0.0);
            } else if tmin_v.y >= tmin_v.z {
                face_uv = vec2<f32>(fract(local.x), fract(local.z));
                face_index = select(3.0, 2.0, rd.y >= 0.0);
            } else {
                face_uv = vec2<f32>(fract(local.x), fract(local.y));
                face_index = select(5.0, 4.0, rd.z >= 0.0);
            }
            face_uv = clamp(face_uv, vec2<f32>(0.0), vec2<f32>(1.0));
            let seed = dot(bp, vec3<f32>(17.0, 31.0, 59.0)) + face_index * 97.0;
            let darken = crack_overlay(face_uv, interaction.break_progress, seed);
            final_color = vec4<f32>(final_color.rgb * (1.0 - darken), 1.0);
        }
    }

    // Placement preview wireframe
    if interaction.highlight_active != 0u {
        final_color = wireframe_box(ro, rd, interaction.highlight_pos, closest_t + 0.01, final_color, vec3<f32>(1.0, 1.0, 1.0), 0.04);
    }

    // Underwater tint
    if interaction.is_submerged != 0u {
        let underwater_tint = vec3<f32>(0.2, 0.4, 0.75);
        final_color = vec4<f32>(mix(final_color.rgb, underwater_tint, 0.12), 1.0);
        if hit_anything {
            let fog_dist = closest_t / 48.0;
            let fog_factor = clamp(fog_dist * fog_dist, 0.0, 0.5);
            final_color = vec4<f32>(mix(final_color.rgb, underwater_tint, fog_factor), 1.0);
        }
    }

    return final_color;
}

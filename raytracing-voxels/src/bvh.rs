use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::world::GpuChunkInfo;

const CHUNK_SIZE: f32 = 16.0;
const LEAF_FLAG: u32 = 0x80000000;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GpuBvhNode {
    pub aabb_min: Vec3,
    pub right_or_chunk: u32,
    pub aabb_max: Vec3,
    pub _padding: u32,
}

struct Primitive {
    chunk_index: u32,
    centroid: Vec3,
    aabb_min: Vec3,
    aabb_max: Vec3,
}

pub fn build_bvh(chunk_infos: &[GpuChunkInfo]) -> Vec<GpuBvhNode> {
    if chunk_infos.is_empty() {
        return Vec::new();
    }

    let mut primitives: Vec<Primitive> = chunk_infos
        .iter()
        .enumerate()
        .map(|(i, info)| {
            let aabb_min = info.world_offset;
            let aabb_max = info.world_offset + Vec3::splat(CHUNK_SIZE);
            let centroid = (aabb_min + aabb_max) * 0.5;
            Primitive {
                chunk_index: i as u32,
                centroid,
                aabb_min,
                aabb_max,
            }
        })
        .collect();

    let n = primitives.len();
    let mut nodes = Vec::with_capacity(2 * n - 1);
    build_recursive(&mut primitives, &mut nodes);
    nodes
}

fn build_recursive(primitives: &mut [Primitive], nodes: &mut Vec<GpuBvhNode>) {
    let (aabb_min, aabb_max) = bounds_of(primitives);

    if primitives.len() == 1 {
        nodes.push(GpuBvhNode {
            aabb_min,
            right_or_chunk: primitives[0].chunk_index | LEAF_FLAG,
            aabb_max,
            _padding: 0,
        });
        return;
    }

    // Find longest axis of centroid bounds
    let (cent_min, cent_max) = centroid_bounds(primitives);
    let extent = cent_max - cent_min;
    let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    };

    primitives.sort_by(|a, b| {
        let ca = match axis {
            0 => a.centroid.x,
            1 => a.centroid.y,
            _ => a.centroid.z,
        };
        let cb = match axis {
            0 => b.centroid.x,
            1 => b.centroid.y,
            _ => b.centroid.z,
        };
        ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mid = primitives.len() / 2;
    let (left_prims, right_prims) = primitives.split_at_mut(mid);

    let parent_index = nodes.len();
    nodes.push(GpuBvhNode {
        aabb_min,
        right_or_chunk: 0, // placeholder
        aabb_max,
        _padding: 0,
    });

    build_recursive(left_prims, nodes);

    let right_index = nodes.len() as u32;
    nodes[parent_index].right_or_chunk = right_index;

    build_recursive(right_prims, nodes);
}

fn bounds_of(primitives: &[Primitive]) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for p in primitives {
        min = min.min(p.aabb_min);
        max = max.max(p.aabb_max);
    }
    (min, max)
}

fn centroid_bounds(primitives: &[Primitive]) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for p in primitives {
        min = min.min(p.centroid);
        max = max.max(p.centroid);
    }
    (min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chunk_info(x: i32, y: i32, z: i32) -> GpuChunkInfo {
        GpuChunkInfo {
            world_offset: Vec3::new(x as f32 * 16.0, y as f32 * 16.0, z as f32 * 16.0),
            data_offset: 0,
        }
    }

    #[test]
    fn empty_input_produces_empty_array() {
        let nodes = build_bvh(&[]);
        assert!(nodes.is_empty());
    }

    #[test]
    fn single_chunk_produces_one_leaf() {
        let infos = vec![make_chunk_info(0, 0, 0)];
        let nodes = build_bvh(&infos);
        assert_eq!(nodes.len(), 1);
        assert_ne!(nodes[0].right_or_chunk & LEAF_FLAG, 0);
        assert_eq!(nodes[0].right_or_chunk & !LEAF_FLAG, 0);
    }

    #[test]
    fn two_chunks_produce_three_nodes() {
        let infos = vec![make_chunk_info(0, 0, 0), make_chunk_info(1, 0, 0)];
        let nodes = build_bvh(&infos);
        assert_eq!(nodes.len(), 3);
    }

    #[test]
    fn root_aabb_encloses_all_chunks() {
        let infos = vec![
            make_chunk_info(0, 0, 0),
            make_chunk_info(3, 2, 1),
            make_chunk_info(-1, 0, 5),
        ];
        let nodes = build_bvh(&infos);

        let root = &nodes[0];
        for info in &infos {
            let chunk_min = info.world_offset;
            let chunk_max = info.world_offset + Vec3::splat(16.0);
            assert!(root.aabb_min.x <= chunk_min.x);
            assert!(root.aabb_min.y <= chunk_min.y);
            assert!(root.aabb_min.z <= chunk_min.z);
            assert!(root.aabb_max.x >= chunk_max.x);
            assert!(root.aabb_max.y >= chunk_max.y);
            assert!(root.aabb_max.z >= chunk_max.z);
        }
    }

    #[test]
    fn node_count_equals_2n_minus_1() {
        for n in [5, 10, 50] {
            let infos: Vec<GpuChunkInfo> =
                (0..n).map(|i| make_chunk_info(i, 0, 0)).collect();
            let nodes = build_bvh(&infos);
            assert_eq!(nodes.len(), 2 * n as usize - 1);
        }
    }

    #[test]
    fn left_child_at_parent_plus_one() {
        let infos: Vec<GpuChunkInfo> =
            (0..10).map(|i| make_chunk_info(i, 0, 0)).collect();
        let nodes = build_bvh(&infos);

        for (i, node) in nodes.iter().enumerate() {
            let is_leaf = (node.right_or_chunk & LEAF_FLAG) != 0;
            if !is_leaf {
                // Left child should be at i+1; verify it exists
                assert!(i + 1 < nodes.len());
            }
        }
    }

    #[test]
    fn leaf_nodes_have_bit_31_set_internal_nodes_dont() {
        let infos: Vec<GpuChunkInfo> =
            (0..8).map(|i| make_chunk_info(i, 0, 0)).collect();
        let nodes = build_bvh(&infos);

        let mut leaf_count = 0;
        let mut internal_count = 0;
        for node in &nodes {
            if (node.right_or_chunk & LEAF_FLAG) != 0 {
                leaf_count += 1;
            } else {
                internal_count += 1;
            }
        }
        assert_eq!(leaf_count, 8);
        assert_eq!(internal_count, 7);
    }
}

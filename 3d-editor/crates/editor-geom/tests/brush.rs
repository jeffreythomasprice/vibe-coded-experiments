mod common;

use std::collections::HashSet;

use common::*;
use editor_core::{Brush, FaceAttrs, FaceId};
use editor_geom::shapes::{cuboid, prism};
use editor_geom::{
    BrushError, brush_geometry, brush_intersect, brush_subtract, clip_brush, mesh_to_brush,
    normalize,
};
use glam::DVec3;
use proptest::prelude::*;

fn unit_cube() -> Brush {
    cuboid(DVec3::ZERO, DVec3::splat(10.0))
}

#[test]
fn cube_geometry() {
    let geo = brush_geometry(&unit_cube()).unwrap();
    assert_eq!(geo.vertices.len(), 8);
    assert_eq!(geo.faces.len(), 6);
    assert!(geo.dropped.is_empty());
    assert!((geo.volume() - 1000.0).abs() < 1e-9);
}

#[test]
fn duplicate_face_is_dropped() {
    let mut b = unit_cube();
    let mut dup = b.faces[0].clone();
    dup.id = FaceId(99);
    b.faces.push(dup);
    let geo = brush_geometry(&b).unwrap();
    assert_eq!(geo.dropped, vec![FaceId(99)]);
    assert_eq!(normalize(&b).unwrap().faces.len(), 6);
}

#[test]
fn redundant_outer_plane_is_dropped() {
    let mut b = unit_cube();
    let outer = cuboid(DVec3::splat(-5.0), DVec3::splat(15.0));
    let mut f = outer.faces[0].clone();
    f.id = FaceId(50);
    b.faces.push(f);
    assert_eq!(brush_geometry(&b).unwrap().dropped, vec![FaceId(50)]);
}

#[test]
fn plane_touching_a_vertex_is_dropped() {
    let mut b = unit_cube();
    b.faces.push(editor_core::BrushFace {
        id: FaceId(77),
        points: [
            DVec3::new(10.0, 10.0, 10.0),
            DVec3::new(11.0, 10.0, 9.0),
            DVec3::new(11.0, 9.0, 10.0),
        ],
        attrs: FaceAttrs::default(),
    });
    let geo = brush_geometry(&b).unwrap();
    assert_eq!(geo.faces.len(), 6);
    assert!(geo.dropped.contains(&FaceId(77)));
}

#[test]
fn missing_face_is_unbounded() {
    let mut b = unit_cube();
    b.faces.pop();
    assert_eq!(brush_geometry(&b).unwrap_err(), BrushError::Unbounded);
}

#[test]
fn too_few_faces() {
    let mut b = unit_cube();
    b.faces.truncate(3);
    assert_eq!(brush_geometry(&b).unwrap_err(), BrushError::TooFewFaces);
}

#[test]
fn inverted_slab_is_empty() {
    let mut b = unit_cube();
    b.faces = vec![
        cuboid(DVec3::ZERO, DVec3::splat(10.0)).faces[0].clone(),
        cuboid(DVec3::new(0.0, 0.0, 20.0), DVec3::splat(30.0)).faces[1].clone(),
    ]
    .into_iter()
    .chain(b.faces.iter().skip(2).cloned())
    .collect();
    assert_eq!(brush_geometry(&b).unwrap_err(), BrushError::Empty);
}

#[test]
fn coplanar_opposite_faces_are_empty() {
    let mut b = unit_cube();
    let top = b.faces[0].points;
    b.faces[1].points = [top[0], top[2], top[1]];
    assert_eq!(brush_geometry(&b).unwrap_err(), BrushError::Empty);
}

#[test]
fn degenerate_face_points() {
    let mut b = unit_cube();
    b.faces[2].points = [DVec3::ZERO, DVec3::X, DVec3::X * 2.0];
    assert_eq!(
        brush_geometry(&b).unwrap_err(),
        BrushError::DegeneratePlane(b.faces[2].id)
    );
}

#[test]
fn clip_cube_in_half() {
    let r = clip_brush(
        &unit_cube(),
        [
            DVec3::new(5.0, 0.0, 0.0),
            DVec3::new(5.0, 10.0, 0.0),
            DVec3::new(5.0, 0.0, 10.0),
        ],
        FaceAttrs::default(),
    )
    .unwrap();
    assert!((brush_volume(&r.front.unwrap()) - 500.0).abs() < 1e-9);
    assert!((brush_volume(&r.back.unwrap()) - 500.0).abs() < 1e-9);
}

#[test]
fn clip_missing_plane_gives_one_side() {
    let r = clip_brush(
        &unit_cube(),
        [
            DVec3::new(50.0, 0.0, 0.0),
            DVec3::new(50.0, 10.0, 0.0),
            DVec3::new(50.0, 0.0, 10.0),
        ],
        FaceAttrs::default(),
    )
    .unwrap();
    assert!(r.front.is_none() ^ r.back.is_none());
}

#[test]
fn subtract_corner_from_cube() {
    let a = unit_cube();
    let b = cuboid(DVec3::splat(5.0), DVec3::splat(15.0));
    let frags = brush_subtract(&a, &b).unwrap();
    let total: f64 = frags.iter().map(brush_volume).sum();
    assert!((total - 875.0).abs() < 1e-9);
}

#[test]
fn prism_round_trips_through_mesh() {
    let p = prism(DVec3::ZERO, 40.0, 32.0, 12);
    let mesh = brush_mesh(&p);
    assert_clean(&mesh);
    let back = mesh_to_brush(&mesh).unwrap();
    assert!((brush_volume(&back) - brush_volume(&p)).abs() < 1e-6);
}

fn edge_count(b: &Brush) -> usize {
    let geo = brush_geometry(b).unwrap();
    let mut edges = HashSet::new();
    for f in &geo.faces {
        for i in 0..f.loop_.len() {
            let (x, y) = (f.loop_[i], f.loop_[(i + 1) % f.loop_.len()]);
            edges.insert((x.min(y), x.max(y)));
        }
    }
    edges.len()
}

#[test]
fn polytope_is_consistent() {
    check(brush_strategy(), 256, |b| {
        let geo = brush_geometry(&b).unwrap();
        let planes: Vec<_> = geo.faces.iter().map(|f| f.plane).collect();
        for v in &geo.vertices {
            let mut on = 0;
            for p in &planes {
                let d = p.normal.dot(*v) - p.dist;
                prop_assert!(d < 5e-3, "vertex outside plane by {d}");
                if d.abs() < 5e-3 {
                    on += 1;
                }
            }
            prop_assert!(on >= 3);
        }
        let (v, e, f) = (
            geo.vertices.len() as i64,
            edge_count(&b) as i64,
            geo.faces.len() as i64,
        );
        prop_assert_eq!(v - e + f, 2, "euler");
        let issues = editor_geom::mesh::validate(&brush_mesh(&b));
        prop_assert!(issues.is_empty(), "{:?}", issues);
        Ok(())
    });
}

#[test]
fn clipped_pieces_recombine() {
    check(
        (brush_strategy(), plane_points_strategy()),
        256,
        |(b, pts)| {
            let Ok(r) = clip_brush(&b, pts, FaceAttrs::default()) else {
                return Ok(());
            };
            let total = r.front.as_ref().map_or(0.0, brush_volume)
                + r.back.as_ref().map_or(0.0, brush_volume);
            let tol = 1e-3 * (1.0 + surface_area(&b));
            prop_assert!(
                (total - brush_volume(&b)).abs() <= tol,
                "{total} vs {}",
                brush_volume(&b)
            );
            for piece in [&r.front, &r.back].into_iter().flatten() {
                let issues = editor_geom::mesh::validate(&brush_mesh(piece));
                prop_assert!(issues.is_empty(), "{:?}", issues);
            }
            Ok(())
        },
    );
}

#[test]
fn subtract_fragments_partition() {
    check((brush_strategy(), brush_strategy()), 256, |(a, b)| {
        let frags = brush_subtract(&a, &b).unwrap();
        let inter = brush_intersect(&a, &b).unwrap();
        let inter_vol = inter.as_ref().map_or(0.0, brush_volume);
        let total: f64 = frags.iter().map(brush_volume).sum();
        let tol = 2e-3 * (1.0 + surface_area(&a) + surface_area(&b));
        prop_assert!(
            (total - (brush_volume(&a) - inter_vol)).abs() <= tol,
            "{total} vs {} - {inter_vol}",
            brush_volume(&a)
        );
        for i in 0..frags.len() {
            for j in i + 1..frags.len() {
                let ov = brush_intersect(&frags[i], &frags[j])
                    .unwrap()
                    .map_or(0.0, |x| brush_volume(&x));
                prop_assert!(ov <= tol, "fragments overlap by {ov}");
            }
        }
        Ok(())
    });
}

#[test]
fn format_doc_room_is_valid() {
    let wall = |id, a: [f64; 3], b: [f64; 3], c: [f64; 3]| editor_core::BrushFace {
        id: FaceId(id),
        points: [DVec3::from(a), DVec3::from(b), DVec3::from(c)],
        attrs: FaceAttrs::default(),
    };
    let room = Brush {
        faces: vec![
            wall(1, [0., 0., 0.], [0., 256., 0.], [256., 0., 0.]),
            wall(2, [0., 0., -16.], [256., 0., -16.], [0., 256., -16.]),
            wall(3, [0., 0., 0.], [0., 256., 0.], [0., 0., 1.]),
            wall(4, [256., 0., 0.], [256., 0., 1.], [256., 256., 0.]),
            wall(5, [0., 0., 0.], [0., 0., 1.], [256., 0., 0.]),
            wall(6, [0., 256., 0.], [256., 256., 0.], [0., 256., 1.]),
        ],
    };
    assert!((brush_volume(&room) - 256.0 * 256.0 * 16.0).abs() < 1e-6);
}

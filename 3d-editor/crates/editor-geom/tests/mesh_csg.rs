mod common;

use common::*;
use editor_core::Mesh;
use editor_geom::mesh::{MeshIssue, intersect, subtract, union, validate};
use editor_geom::shapes::{cuboid, prism};
use editor_geom::volume::mesh_volume;
use editor_geom::{MeshToBrushError, mesh_to_brush};
use glam::DVec3;
use proptest::prelude::*;

fn cube_mesh(min: [f64; 3], max: [f64; 3]) -> Mesh {
    brush_mesh(&cuboid(DVec3::from(min), DVec3::from(max)))
}

fn tol(m: &Mesh) -> f64 {
    1e-3 * (1.0 + mesh_volume(m).abs().cbrt().powi(2))
}

#[test]
fn overlapping_cubes() {
    let a = cube_mesh([0.0; 3], [10.0; 3]);
    let b = cube_mesh([5.0; 3], [15.0; 3]);
    let u = union(&a, &b).unwrap();
    assert_clean(&u);
    assert_volume(&u, 1875.0, 1e-6);
    let s = subtract(&a, &b).unwrap();
    assert_clean(&s);
    assert_volume(&s, 875.0, 1e-6);
    let i = intersect(&a, &b).unwrap();
    assert_clean(&i);
    assert_volume(&i, 125.0, 1e-6);
}

#[test]
fn shared_face() {
    let a = cube_mesh([0.0; 3], [10.0; 3]);
    let b = cube_mesh([10.0, 0.0, 0.0], [20.0, 10.0, 10.0]);
    let u = union(&a, &b).unwrap();
    assert_clean(&u);
    assert_volume(&u, 2000.0, 1e-6);
    let s = subtract(&a, &b).unwrap();
    assert_clean(&s);
    assert_volume(&s, 1000.0, 1e-6);
    let i = intersect(&a, &b).unwrap();
    assert_volume(&i, 0.0, 1e-6);
}

#[test]
fn shared_face_partial() {
    let a = cube_mesh([0.0; 3], [10.0; 3]);
    let b = cube_mesh([10.0, 3.0, 3.0], [20.0, 7.0, 7.0]);
    let u = union(&a, &b).unwrap();
    assert_clean(&u);
    assert_volume(&u, 1000.0 + 160.0, 1e-6);
    let s = subtract(&a, &b).unwrap();
    assert_clean(&s);
    assert_volume(&s, 1000.0, 1e-6);
}

#[test]
fn touching_edge_and_vertex() {
    let a = cube_mesh([0.0; 3], [10.0; 3]);
    let edge = cube_mesh([10.0, 10.0, 0.0], [20.0, 20.0, 10.0]);
    let vert = cube_mesh([10.0, 10.0, 10.0], [20.0, 20.0, 20.0]);
    assert_volume(&union(&a, &edge).unwrap(), 2000.0, 1e-6);
    assert_volume(&union(&a, &vert).unwrap(), 2000.0, 1e-6);
    assert_volume(&intersect(&a, &edge).unwrap(), 0.0, 1e-6);
    assert_volume(&subtract(&a, &vert).unwrap(), 1000.0, 1e-6);
    assert_clean(&subtract(&a, &edge).unwrap());
}

#[test]
fn identical_operands() {
    let a = cube_mesh([0.0; 3], [10.0; 3]);
    let u = union(&a, &a).unwrap();
    assert_clean(&u);
    assert_volume(&u, 1000.0, 1e-6);
    let i = intersect(&a, &a).unwrap();
    assert_clean(&i);
    assert_volume(&i, 1000.0, 1e-6);
    let s = subtract(&a, &a).unwrap();
    assert_volume(&s, 0.0, 1e-6);
}

#[test]
fn contained_and_disjoint() {
    let a = cube_mesh([0.0; 3], [10.0; 3]);
    let inner = cube_mesh([3.0; 3], [6.0; 3]);
    let far = cube_mesh([30.0; 3], [40.0; 3]);
    let hollow = subtract(&a, &inner).unwrap();
    assert_clean(&hollow);
    assert_volume(&hollow, 1000.0 - 27.0, 1e-6);
    let u = union(&a, &inner).unwrap();
    assert_clean(&u);
    assert_volume(&u, 1000.0, 1e-6);
    let two = union(&a, &far).unwrap();
    assert_clean(&two);
    assert_volume(&two, 2000.0, 1e-6);
    assert_volume(&intersect(&a, &far).unwrap(), 0.0, 1e-6);
}

#[test]
fn chained_subtractions() {
    let a = cube_mesh([0.0; 3], [20.0; 3]);
    let b = cube_mesh([5.0, -5.0, 5.0], [15.0, 25.0, 15.0]);
    let c = cube_mesh([-5.0, 5.0, 5.0], [25.0, 15.0, 15.0]);
    let r = subtract(&subtract(&a, &b).unwrap(), &c).unwrap();
    assert_clean(&r);
    let expected = 8000.0 - 2000.0 - 2000.0 + 1000.0;
    assert_volume(&r, expected, 1e-6);
}

#[test]
fn open_operand_is_rejected() {
    let a = cube_mesh([0.0; 3], [10.0; 3]);
    let mut open = a.clone();
    open.faces.pop();
    assert!(union(&a, &open).is_err());
    assert!(union(&open, &a).is_err());
}

#[test]
fn union_is_not_a_brush() {
    let a = cube_mesh([0.0; 3], [10.0; 3]);
    let b = cube_mesh([5.0; 3], [15.0; 3]);
    let u = union(&a, &b).unwrap();
    assert_eq!(mesh_to_brush(&u), Err(MeshToBrushError::NotConvex));
}

#[test]
fn convex_csg_result_converts_back() {
    let a = cube_mesh([0.0; 3], [10.0; 3]);
    let b = cube_mesh([5.0, -5.0, -5.0], [15.0, 15.0, 15.0]);
    let i = intersect(&a, &b).unwrap();
    let brush = mesh_to_brush(&i).unwrap();
    assert!((brush_volume(&brush) - 500.0).abs() < 1e-6);
}

#[test]
fn donut() {
    let sides = 16;
    let outer_b = prism(DVec3::ZERO, 64.0, 32.0, sides);
    let inner_b = prism(DVec3::ZERO, 24.0, 64.0, sides);
    let outer = brush_mesh(&outer_b);
    let inner = brush_mesh(&inner_b);
    let mut donut = subtract(&outer, &inner).unwrap();
    assert_clean(&donut);

    let ngon = |r: f64| 0.5 * sides as f64 * r * r * (std::f64::consts::TAU / sides as f64).sin();
    let expected = (ngon(64.0) - ngon(24.0)) * 32.0;
    assert_volume(&donut, expected, expected * 2e-3);

    let (idx, _) = donut
        .vertices
        .iter()
        .enumerate()
        .max_by(|a, b| {
            let key = |v: &editor_core::MeshVertex| (v.pos.truncate().length(), v.pos.z);
            key(a.1).partial_cmp(&key(b.1)).unwrap()
        })
        .unwrap();
    let moved_id = donut.vertices[idx].id;
    let dir = donut.vertices[idx].pos.truncate().normalize();
    donut.vertices[idx].pos += DVec3::new(dir.x, dir.y, 0.0) * 8.0;

    let issues = validate(&donut);
    for issue in &issues {
        match issue {
            MeshIssue::NonPlanarFace { face, .. } => {
                let f = donut.faces.iter().find(|f| f.id == *face).unwrap();
                assert!(f.verts.contains(&moved_id));
            }
            other => panic!("unexpected issue after vertex edit: {other:?}"),
        }
    }
    assert!(mesh_volume(&donut) > expected);
}

fn mesh_strategy() -> impl Strategy<Value = Mesh> {
    brush_strategy().prop_map(|b| brush_mesh(&b))
}

#[test]
fn csg_results_are_closed_and_volumes_add_up() {
    let tally = Tally::default();
    check((mesh_strategy(), mesh_strategy()), 256, |(a, b)| {
        let (Some(u), Some(s), Some(i)) = (
            tally.record(union(&a, &b)),
            tally.record(subtract(&a, &b)),
            tally.record(intersect(&a, &b)),
        ) else {
            return Ok(());
        };
        for m in [&u, &s, &i] {
            let issues = watertight_issues(m);
            prop_assert!(issues.is_empty(), "{:?}", issues);
        }
        let (va, vb) = (mesh_volume(&a), mesh_volume(&b));
        let (vu, vs, vi) = (mesh_volume(&u), mesh_volume(&s), mesh_volume(&i));
        let t = tol(&a) + tol(&b);
        prop_assert!(
            (vu - (va + vb - vi)).abs() <= t,
            "union {vu} vs {va}+{vb}-{vi}"
        );
        prop_assert!((vs - (va - vi)).abs() <= t, "subtract {vs} vs {va}-{vi}");
        prop_assert!(vi >= -t && vi <= va.min(vb) + t);
        Ok(())
    });
    tally.assert_rate_below(0.005);
}

#[test]
fn chained_csg_stays_closed() {
    let tally = Tally::default();
    check(
        (mesh_strategy(), mesh_strategy(), mesh_strategy()),
        256,
        |(a, b, c)| {
            let Some(ab) = tally.record(subtract(&a, &b)) else {
                return Ok(());
            };
            let Some(r) = tally.record(subtract(&ab, &c)) else {
                return Ok(());
            };
            let issues = watertight_issues(&r);
            prop_assert!(issues.is_empty(), "{:?}", issues);
            prop_assert!(mesh_volume(&r) >= -tol(&a));
            prop_assert!(mesh_volume(&r) <= mesh_volume(&a) + tol(&a));
            Ok(())
        },
    );
    tally.assert_rate_below(0.005);
}

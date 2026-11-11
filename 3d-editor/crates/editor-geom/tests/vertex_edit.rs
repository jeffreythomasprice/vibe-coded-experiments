mod common;

use editor_geom::shapes::cuboid;
use editor_geom::{VertexEditError, brush_geometry, move_brush_vertices};
use glam::DVec3;

fn cube() -> editor_core::Brush {
    cuboid(DVec3::ZERO, DVec3::splat(64.0))
}

fn has_vertex(brush: &editor_core::Brush, p: DVec3) -> bool {
    brush_geometry(brush)
        .unwrap()
        .vertices
        .iter()
        .any(|v| (*v - p).length() < 1e-4)
}

#[test]
fn raising_a_corner_splits_the_top_face() {
    let corner = DVec3::splat(64.0);
    let out = move_brush_vertices(&cube(), &[corner], DVec3::new(0.0, 0.0, 32.0), false).unwrap();
    let geo = brush_geometry(&out).unwrap();
    assert_eq!(geo.vertices.len(), 8);
    assert_eq!(out.faces.len(), 7);
    assert!(has_vertex(&out, DVec3::new(64.0, 64.0, 96.0)));
    assert!(!has_vertex(&out, corner));
    let ids: std::collections::BTreeSet<_> = out.faces.iter().map(|f| f.id).collect();
    assert_eq!(ids.len(), 7);
    for original in cube().faces.iter().filter(|f| f.id.0 != 1) {
        assert!(ids.contains(&original.id), "{:?} lost", original.id);
    }
}

#[test]
fn lowering_a_corner_keeps_the_top_as_a_triangle() {
    let out = move_brush_vertices(
        &cube(),
        &[DVec3::splat(64.0)],
        DVec3::new(0.0, 0.0, -32.0),
        false,
    )
    .unwrap();
    assert_eq!(out.faces.len(), 7);
    let ids: std::collections::BTreeSet<_> = out.faces.iter().map(|f| f.id.0).collect();
    for kept in 1..=6 {
        assert!(ids.contains(&kept));
    }
    let top = out.faces.iter().find(|f| f.id.0 == 1).unwrap();
    assert_eq!(top.points, cube().faces[0].points);
}

#[test]
fn fusing_two_vertices_drops_one() {
    let out = move_brush_vertices(
        &cube(),
        &[DVec3::new(0.0, 0.0, 64.0)],
        DVec3::new(64.0, 0.0, 0.0),
        false,
    )
    .unwrap();
    assert_eq!(brush_geometry(&out).unwrap().vertices.len(), 7);
    assert_eq!(out.faces.len(), 7);
    assert!(!has_vertex(&out, DVec3::new(0.0, 0.0, 64.0)));
}

#[test]
fn pushing_a_corner_inward_is_rejected() {
    let r = move_brush_vertices(&cube(), &[DVec3::splat(64.0)], DVec3::splat(-32.0), false);
    assert!(matches!(r, Err(VertexEditError::WouldBeConcave(_))));
}

#[test]
fn unknown_vertex_is_reported() {
    let r = move_brush_vertices(&cube(), &[DVec3::new(1.0, 2.0, 3.0)], DVec3::X, false);
    assert_eq!(
        r,
        Err(VertexEditError::VertexNotFound(DVec3::new(1.0, 2.0, 3.0)))
    );
}

#[test]
fn moving_a_whole_face_keeps_every_face_id() {
    let top: Vec<DVec3> = [(0.0, 0.0), (64.0, 0.0), (64.0, 64.0), (0.0, 64.0)]
        .iter()
        .map(|&(x, y)| DVec3::new(x, y, 64.0))
        .collect();
    let out = move_brush_vertices(&cube(), &top, DVec3::new(0.0, 0.0, 16.0), false).unwrap();
    assert_eq!(out.faces.len(), 6);
    let ids: Vec<_> = out.faces.iter().map(|f| f.id).collect();
    let want: Vec<_> = cube().faces.iter().map(|f| f.id).collect();
    assert_eq!(ids, want);
    assert!(has_vertex(&out, DVec3::new(64.0, 64.0, 80.0)));
}

#[test]
fn shearing_a_face_keeps_the_brush_valid() {
    let top: Vec<DVec3> = [(0.0, 0.0), (64.0, 0.0), (64.0, 64.0), (0.0, 64.0)]
        .iter()
        .map(|&(x, y)| DVec3::new(x, y, 64.0))
        .collect();
    let out = move_brush_vertices(&cube(), &top, DVec3::new(32.0, 0.0, 0.0), false).unwrap();
    assert_eq!(out.faces.len(), 6);
    assert!(has_vertex(&out, DVec3::new(96.0, 64.0, 64.0)));
}

mod random {
    use super::*;
    use editor_core::Brush;
    use proptest::prelude::*;

    use super::common;

    fn moves() -> impl Strategy<Value = (Brush, Vec<usize>, [i32; 3])> {
        (
            common::brush_strategy(),
            prop::collection::vec(0usize..32, 1..4),
            prop::array::uniform3(-6i32..7),
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(400))]

        #[test]
        fn result_is_a_valid_brush_with_the_moved_corners(
            (brush, picks, d) in moves()
        ) {
            let geo = brush_geometry(&brush).unwrap();
            let picked: Vec<DVec3> = picks
                .iter()
                .map(|&i| geo.vertices[i % geo.vertices.len()])
                .collect();
            let delta = DVec3::new(d[0] as f64, d[1] as f64, d[2] as f64);
            match move_brush_vertices(&brush, &picked, delta, false) {
                Ok(out) => {
                    let derived = brush_geometry(&out).expect("result is a valid brush");
                    prop_assert!(derived.volume() > 0.0);
                    let mut ids: Vec<_> = out.faces.iter().map(|f| f.id).collect();
                    ids.sort();
                    ids.dedup();
                    prop_assert_eq!(ids.len(), out.faces.len());
                    for p in &picked {
                        let want = *p + delta;
                        prop_assert!(
                            derived.vertices.iter().any(|v| (*v - want).length() < 1e-3),
                            "{want} missing"
                        );
                    }
                }
                Err(VertexEditError::WouldBeConcave(_)) | Err(VertexEditError::Brush(_)) => {}
                Err(e) => prop_assert!(false, "unexpected {e}"),
            }
        }
    }
}

use editor_geom::brush_geometry;
use editor_geom::shapes::cuboid;
use editor_geom::transform::transform_brush;
use glam::{DAffine3, DMat3, DVec3};
use proptest::prelude::*;

fn extent(b: &editor_core::Brush) -> (DVec3, DVec3) {
    let g = brush_geometry(b).unwrap();
    let min = g.vertices.iter().fold(DVec3::MAX, |a, v| a.min(*v));
    let max = g.vertices.iter().fold(DVec3::MIN, |a, v| a.max(*v));
    (min, max)
}

proptest! {
    #[test]
    fn rotation_then_inverse_round_trips(
        size in (1i32..64, 1i32..64, 1i32..64),
        origin in (-100i32..100, -100i32..100, -100i32..100),
        axis in (-1.0f64..1.0, -1.0f64..1.0, 0.1f64..1.0),
        degrees in -180.0f64..180.0,
    ) {
        let min = DVec3::new(origin.0 as f64, origin.1 as f64, origin.2 as f64);
        let max = min + DVec3::new(size.0 as f64, size.1 as f64, size.2 as f64);
        let brush = cuboid(min, max);
        let axis = DVec3::new(axis.0, axis.1, axis.2).normalize();
        let there = DAffine3::from_mat3(DMat3::from_axis_angle(axis, degrees.to_radians()));
        let back = there.inverse();
        let rotated = transform_brush(&brush, &there, false).unwrap();
        let restored = transform_brush(&rotated, &back, false).unwrap();
        let (a, b) = extent(&brush);
        let (c, d) = extent(&restored);
        prop_assert!((a - c).length() < 1e-3 && (b - d).length() < 1e-3, "{a:?} {b:?} vs {c:?} {d:?}");
    }

    #[test]
    fn mirror_preserves_volume(
        size in (1i32..64, 1i32..64, 1i32..64),
        axis in 0usize..3,
    ) {
        let brush = cuboid(DVec3::ZERO, DVec3::new(size.0 as f64, size.1 as f64, size.2 as f64));
        let mut scale = DVec3::ONE;
        scale[axis] = -1.0;
        let mirrored = transform_brush(&brush, &DAffine3::from_scale(scale), false).unwrap();
        let (v0, v1) = (brush_geometry(&brush).unwrap().volume(), brush_geometry(&mirrored).unwrap().volume());
        prop_assert!((v0 - v1).abs() < 1e-6);
    }
}

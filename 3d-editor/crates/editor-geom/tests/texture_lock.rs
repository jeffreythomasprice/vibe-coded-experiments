use editor_core::{Brush, FaceAttrs, Mesh};
use editor_geom::shapes::cuboid;
use editor_geom::transform::{transform_brush, transform_mesh};
use editor_geom::uv::project;
use editor_geom::{brush_geometry, brush_to_mesh, move_brush_vertices};
use glam::{DAffine3, DMat3, DVec2, DVec3};
use proptest::prelude::*;

fn attrs_strategy() -> impl Strategy<Value = FaceAttrs> {
    (
        -64.0f64..64.0,
        -64.0f64..64.0,
        0.25f64..4.0,
        0.25f64..4.0,
        -180.0f64..180.0,
    )
        .prop_map(|(ox, oy, sx, sy, rot)| FaceAttrs {
            material: "m".into(),
            offset: DVec2::new(ox, oy),
            scale: DVec2::new(sx, sy),
            rotation: rot,
            axes: None,
        })
}

fn textured_cube(attrs: &[FaceAttrs]) -> Brush {
    let mut b = cuboid(DVec3::ZERO, DVec3::new(64.0, 48.0, 32.0));
    for (f, a) in b.faces.iter_mut().zip(attrs.iter().cycle()) {
        f.attrs = a.clone();
    }
    b
}

fn assert_locked(
    old: &Brush,
    new: &Brush,
    m: &dyn Fn(DVec3) -> DVec3,
) -> Result<(), TestCaseError> {
    let (go, gn) = (brush_geometry(old).unwrap(), brush_geometry(new).unwrap());
    for fo in &go.faces {
        let ao = &old.faces.iter().find(|f| f.id == fo.face).unwrap().attrs;
        let fnw = gn.faces.iter().find(|f| f.face == fo.face).unwrap();
        let an = &new.faces.iter().find(|f| f.id == fo.face).unwrap().attrs;
        for &i in &fo.loop_ {
            let p = go.vertices[i];
            let before = project(ao, fo.plane.normal, p);
            let after = project(an, fnw.plane.normal, m(p));
            prop_assert!(
                (before - after).length() < 1e-3,
                "face {:?} corner {p:?}: {before:?} vs {after:?}",
                fo.face
            );
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn rotation_keeps_texels(
        attrs in proptest::collection::vec(attrs_strategy(), 6),
        axis in (-1.0f64..1.0, -1.0f64..1.0, 0.1f64..1.0),
        degrees in -180.0f64..180.0,
        shift in (-64i32..64, -64i32..64, -64i32..64),
    ) {
        let brush = textured_cube(&attrs);
        let axis = DVec3::new(axis.0, axis.1, axis.2).normalize();
        let m = DAffine3 {
            matrix3: DMat3::from_axis_angle(axis, degrees.to_radians()),
            translation: DVec3::new(shift.0 as f64, shift.1 as f64, shift.2 as f64),
        };
        let out = transform_brush(&brush, &m, true).unwrap();
        assert_locked(&brush, &out, &|p| m.transform_point3(p))?;
    }

    #[test]
    fn mirror_and_scale_keep_texels(
        attrs in proptest::collection::vec(attrs_strategy(), 6),
        scale in (1u32..4, 1u32..4, 1u32..4),
        flip in 0usize..4,
    ) {
        let brush = textured_cube(&attrs);
        let mut s = DVec3::new(scale.0 as f64, scale.1 as f64, scale.2 as f64);
        if flip < 3 {
            s[flip] = -s[flip];
        }
        let m = DAffine3::from_scale(s);
        let out = transform_brush(&brush, &m, true).unwrap();
        assert_locked(&brush, &out, &|p| m.transform_point3(p))?;
    }

    #[test]
    fn mesh_rotation_keeps_texels(
        attrs in proptest::collection::vec(attrs_strategy(), 6),
        degrees in -180.0f64..180.0,
        flip in any::<bool>(),
    ) {
        let brush = textured_cube(&attrs);
        let mesh: Mesh = brush_to_mesh(&brush).unwrap();
        let mut m = DAffine3::from_mat3(DMat3::from_rotation_z(degrees.to_radians()));
        if flip {
            m *= DAffine3::from_scale(DVec3::new(-1.0, 1.0, 1.0));
        }
        let out = transform_mesh(&mesh, &m, true);
        for (fo, fnw) in mesh.faces.iter().zip(&out.faces) {
            let pts: Vec<DVec3> = fo.verts.iter().map(|&v| mesh.vertex(v).unwrap().pos).collect();
            let n = editor_geom::winding::newell(&pts).normalize();
            let new_pts: Vec<DVec3> = fnw.verts.iter().map(|&v| out.vertex(v).unwrap().pos).collect();
            let nn = editor_geom::winding::newell(&new_pts).normalize();
            for &v in &fo.verts {
                let p = mesh.vertex(v).unwrap().pos;
                let q = out.vertex(v).unwrap().pos;
                let d = (project(&fo.attrs, n, p) - project(&fnw.attrs, nn, q)).length();
                prop_assert!(d < 1e-3, "{d}");
            }
        }
    }

    #[test]
    fn vertex_moves_keep_stationary_texels(
        attrs in proptest::collection::vec(attrs_strategy(), 6),
        corner in 0usize..8,
        delta in (-24i32..24, -24i32..24, 1i32..24),
    ) {
        let brush = textured_cube(&attrs);
        let geo = brush_geometry(&brush).unwrap();
        let pick = geo.vertices[corner];
        let delta = DVec3::new(delta.0 as f64, delta.1 as f64, delta.2 as f64);
        let Ok(out) = move_brush_vertices(&brush, &[pick], delta, true) else {
            return Ok(());
        };
        let gn = brush_geometry(&out).unwrap();
        for fo in &geo.faces {
            let Some(fnw) = gn.faces.iter().find(|f| f.face == fo.face) else { continue };
            let ao = &brush.faces.iter().find(|f| f.id == fo.face).unwrap().attrs;
            let an = &out.faces.iter().find(|f| f.id == fo.face).unwrap().attrs;
            for &i in &fo.loop_ {
                let p = geo.vertices[i];
                if (p - pick).length() < 1e-6 {
                    continue;
                }
                let still_there = gn.face_positions(fnw).iter().any(|q| (*q - p).length() < 1e-6);
                if !still_there {
                    continue;
                }
                let d = (project(ao, fo.plane.normal, p) - project(an, fnw.plane.normal, p)).length();
                prop_assert!(d < 1e-3, "face {:?}: {d}", fo.face);
            }
        }
    }
}

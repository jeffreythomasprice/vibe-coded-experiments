mod common;

use common::*;
use serde_json::json;

#[test]
fn translate_moves_brush_and_undoes() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let r = exec(
        &mut s,
        "transform.translate",
        json!({ "ids": [&b], "delta": [10, 0, -4] }),
    );
    assert_eq!(ok(&r)["changes"]["modified"], json!([b]));
    assert_eq!(bounds(&mut s, &b), ([10.0, 0.0, -4.0], [18.0, 8.0, 4.0]));
    exec(&mut s, "history.undo", json!({}));
    assert_eq!(bounds(&mut s, &b), ([0.0; 3], [8.0; 3]));
}

#[test]
fn rotate_about_center_and_origin() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 4, 2]);
    ok(&exec(
        &mut s,
        "transform.rotate",
        json!({ "ids": [&b], "axis": "z", "degrees": 90 }),
    ));
    let (min, max) = bounds(&mut s, &b);
    assert!(near(min, [2.0, -2.0, 0.0]), "{min:?}");
    assert!(near(max, [6.0, 6.0, 2.0]), "{max:?}");

    let c = make_box(&mut s, [0, 0, 0], [8, 4, 2]);
    ok(&exec(
        &mut s,
        "transform.rotate",
        json!({ "ids": [&c], "axis": [0, 0, 1], "degrees": 90, "pivot": "origin" }),
    ));
    let (min, max) = bounds(&mut s, &c);
    assert!(
        near(min, [-4.0, 0.0, 0.0]) && near(max, [0.0, 8.0, 2.0]),
        "{min:?} {max:?}"
    );
}

#[test]
fn scale_and_flip() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    ok(&exec(
        &mut s,
        "transform.scale",
        json!({ "ids": [&b], "factors": [2, 1, 0.5], "pivot": [0, 0, 0] }),
    ));
    assert_eq!(bounds(&mut s, &b), ([0.0; 3], [16.0, 8.0, 4.0]));
    ok(&exec(
        &mut s,
        "transform.flip",
        json!({ "ids": [&b], "axis": "x", "pivot": [0, 0, 0] }),
    ));
    let (min, max) = bounds(&mut s, &b);
    assert!(
        near(min, [-16.0, 0.0, 0.0]) && near(max, [0.0, 8.0, 4.0]),
        "{min:?} {max:?}"
    );
    assert_eq!(
        exec(&mut s, "check.validate", json!({}))["result"]["errors"],
        0
    );
    ok(&exec(
        &mut s,
        "transform.scale",
        json!({ "ids": [&b], "factors": 2.0 }),
    ));
}

#[test]
fn bad_transform_arguments() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let bad = |s: &mut _, op: &str, args| err_code(&exec(s, op, args)).to_string();
    assert_eq!(
        bad(
            &mut s,
            "transform.scale",
            json!({ "ids": [&b], "factors": 0 })
        ),
        "InvalidArgument"
    );
    assert_eq!(
        bad(
            &mut s,
            "transform.rotate",
            json!({ "ids": [&b], "axis": [0, 0, 0], "degrees": 5 })
        ),
        "InvalidArgument"
    );
    assert_eq!(
        bad(
            &mut s,
            "transform.translate",
            json!({ "ids": [], "delta": [1, 0, 0] })
        ),
        "InvalidArgument"
    );
    assert_eq!(
        bad(
            &mut s,
            "transform.translate",
            json!({ "ids": ["b99"], "delta": [1, 0, 0] })
        ),
        "NotFound"
    );
    let layer = layer_id(&mut s);
    assert_eq!(
        bad(
            &mut s,
            "transform.translate",
            json!({ "ids": [layer], "delta": [1, 0, 0] })
        ),
        "WrongKind"
    );
    assert_eq!(
        bad(
            &mut s,
            "transform.translate",
            json!({ "ids": [&b], "delta": [9e9, 0, 0] })
        ),
        "InvalidArgument"
    );
    assert_eq!(bounds(&mut s, &b), ([0.0; 3], [8.0; 3]));
}

#[test]
fn entity_origin_and_angles_follow_rotation() {
    let mut s = session();
    let e = id_of(&exec(
        &mut s,
        "entity.create",
        json!({ "classname": "light", "origin": [10, 0, 0] }),
    ));
    ok(&exec(
        &mut s,
        "transform.rotate",
        json!({ "ids": [&e], "axis": "z", "degrees": 90, "pivot": "origin" }),
    ));
    let v = full(&mut s, &e);
    assert!(near(
        [
            v["origin"][0].as_f64().unwrap(),
            v["origin"][1].as_f64().unwrap(),
            v["origin"][2].as_f64().unwrap()
        ],
        [0.0, 10.0, 0.0]
    ));
    assert!(
        (v["angles"][1].as_f64().unwrap() - 90.0).abs() < 1e-6,
        "{v}"
    );
    assert!(v["angles"][0].as_f64().unwrap().abs() < 1e-6);
}

#[test]
fn groups_and_brush_entities_transform_their_contents() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    let g = id_of(&exec(&mut s, "group.create", json!({ "objects": [&a] })));
    let ent = id_of(&exec(
        &mut s,
        "entity.create_brush_entity",
        json!({ "classname": "func_door", "objects": [&b] }),
    ));
    ok(&exec(
        &mut s,
        "transform.translate",
        json!({ "ids": [&g, &ent], "delta": [0, 0, 100] }),
    ));
    assert_eq!(bounds(&mut s, &a).0, [0.0, 0.0, 100.0]);
    assert_eq!(bounds(&mut s, &b).0, [16.0, 0.0, 100.0]);
}

#[test]
fn mirrored_mesh_stays_valid() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let m = ids_of(&exec(&mut s, "brush.to_mesh", json!({ "ids": [&b] })))[0].clone();
    ok(&exec(
        &mut s,
        "transform.flip",
        json!({ "ids": [&m], "axis": "y", "pivot": [0, 0, 0] }),
    ));
    assert_eq!(
        exec(&mut s, "check.validate", json!({}))["result"]["issues"],
        json!([])
    );
}

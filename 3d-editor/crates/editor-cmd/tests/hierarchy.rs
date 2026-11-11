#![allow(clippy::cloned_ref_to_slice_refs)]
mod common;

use common::*;
use serde_json::json;

#[test]
fn entity_create_and_edit() {
    let mut s = session();
    let r = exec(
        &mut s,
        "entity.create",
        json!({ "classname": "light", "origin": [0, 0, 64], "properties": { "intensity": 300, "color": [1, 0.5, 0], "cast": true, "name": "lamp" } }),
    );
    let e = id_of(&r);
    let v = full(&mut s, &e);
    assert_eq!(v["classname"], "light");
    assert_eq!(v["properties"]["intensity"], 300.0);

    ok(&exec(
        &mut s,
        "entity.set_properties",
        json!({ "ids": [&e], "set": { "intensity": 500 }, "remove": ["cast"] }),
    ));
    let v = full(&mut s, &e);
    assert_eq!(v["properties"]["intensity"], 500.0);
    assert!(v["properties"].get("cast").is_none());

    ok(&exec(
        &mut s,
        "entity.set_classname",
        json!({ "ids": [&e], "classname": "light_spot" }),
    ));
    ok(&exec(
        &mut s,
        "entity.set_transform",
        json!({ "id": &e, "origin": [1, 2, 3], "angles": [10, 20, 30] }),
    ));
    let v = full(&mut s, &e);
    assert_eq!(v["classname"], "light_spot");
    assert_eq!(v["origin"], json!([1.0, 2.0, 3.0]));
    assert_eq!(v["angles"], json!([10.0, 20.0, 30.0]));

    exec(&mut s, "history.undo", json!({}));
    assert_eq!(full(&mut s, &e)["origin"], json!([0.0, 0.0, 64.0]));
}

#[test]
fn entity_errors() {
    let mut s = session();
    let bad = |s: &mut _, op: &str, args| err_code(&exec(s, op, args)).to_string();
    assert_eq!(
        bad(
            &mut s,
            "entity.create",
            json!({ "classname": " ", "origin": [0, 0, 0] })
        ),
        "InvalidArgument"
    );
    assert_eq!(
        bad(
            &mut s,
            "entity.create",
            json!({ "classname": "x", "origin": [0, 0, 0], "properties": { "origin": "1 2 3" } })
        ),
        "InvalidArgument"
    );
    assert_eq!(
        bad(
            &mut s,
            "entity.create",
            json!({ "classname": "x", "origin": [0, 0, 0], "layer": "l99" })
        ),
        "NotFound"
    );
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    assert_eq!(
        bad(
            &mut s,
            "entity.set_properties",
            json!({ "ids": [&b], "set": { "a": 1 } })
        ),
        "WrongKind"
    );
    let brush_ent = id_of(&exec(
        &mut s,
        "entity.create_brush_entity",
        json!({ "classname": "func_wall", "objects": [&b] }),
    ));
    assert_eq!(
        bad(
            &mut s,
            "entity.set_transform",
            json!({ "id": &brush_ent, "origin": [0, 0, 0] })
        ),
        "InvalidArgument"
    );
    assert_eq!(
        bad(
            &mut s,
            "entity.create_brush_entity",
            json!({ "classname": "x", "objects": [&b] })
        ),
        "InvalidArgument"
    );
}

#[test]
fn brush_entity_lifecycle() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    let ent = id_of(&exec(
        &mut s,
        "entity.create_brush_entity",
        json!({ "classname": "func_door", "objects": [&a, &b], "properties": { "speed": 100 } }),
    ));
    assert_eq!(full(&mut s, &a)["entity"], json!(ent));

    let r = exec(&mut s, "entity.release_objects", json!({ "ids": [&a] }));
    assert!(ok(&r)["warnings"].as_array().unwrap().is_empty());
    assert!(full(&mut s, &a).get("entity").is_none_or(|e| e.is_null()));

    let r = exec(&mut s, "entity.release_objects", json!({ "ids": [&b] }));
    assert_eq!(ok(&r)["warnings"].as_array().unwrap().len(), 1);
    assert_eq!(r["changes"]["deleted"], json!([ent]));
    assert_eq!(summary(&mut s)["counts"]["entity"], 0);

    exec(&mut s, "history.undo", json!({}));
    assert_eq!(summary(&mut s)["counts"]["entity"], 1);
    assert_eq!(
        err_code(&exec(
            &mut s,
            "entity.release_objects",
            json!({ "ids": [&a] })
        )),
        "InvalidArgument"
    );
}

#[test]
fn brush_entity_needs_matching_layer_and_group() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    let l2 = id_of(&exec(&mut s, "layer.create", json!({ "name": "other" })));
    ok(&exec(
        &mut s,
        "object.set_layer",
        json!({ "ids": [&b], "layer": &l2 }),
    ));
    assert_eq!(
        err_code(&exec(
            &mut s,
            "entity.create_brush_entity",
            json!({ "classname": "x", "objects": [&a, &b] })
        )),
        "InvalidArgument"
    );
}

#[test]
fn groups_nest_and_ungroup() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    let inner = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [&a], "name": "inner" }),
    ));
    let outer = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [&inner, &b], "name": "outer" }),
    ));
    assert_eq!(full(&mut s, &inner)["parent"], json!(outer));
    assert_eq!(full(&mut s, &a)["group"], json!(inner));

    ok(&exec(&mut s, "group.ungroup", json!({ "ids": [&inner] })));
    assert_eq!(full(&mut s, &a)["group"], json!(outer));
    assert_eq!(summary(&mut s)["counts"]["group"], 1);

    ok(&exec(&mut s, "group.move", json!({ "objects": [&a] })));
    assert!(full(&mut s, &a).get("group").is_none_or(|g| g.is_null()));
    ok(&exec(
        &mut s,
        "group.move",
        json!({ "objects": [&a], "group": &outer }),
    ));
    assert_eq!(full(&mut s, &a)["group"], json!(outer));
}

#[test]
fn group_rules() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    let g = id_of(&exec(&mut s, "group.create", json!({ "objects": [&a] })));
    let sub = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [&b], "name": "sub" }),
    ));
    ok(&exec(
        &mut s,
        "group.move",
        json!({ "objects": [&sub], "group": &g }),
    ));
    assert_eq!(
        err_code(&exec(
            &mut s,
            "group.move",
            json!({ "objects": [&g], "group": &sub })
        )),
        "InvalidArgument"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "group.create",
            json!({ "objects": [&a, &b] })
        )),
        "InvalidArgument"
    );
    let ent = id_of(&exec(
        &mut s,
        "entity.create_brush_entity",
        json!({ "classname": "x", "objects": [&a] }),
    ));
    let _ = ent;
    assert_eq!(
        err_code(&exec(&mut s, "group.create", json!({ "objects": [&a] }))),
        "InvalidArgument"
    );
}

#[test]
fn layers() {
    let mut s = session();
    let main = layer_id(&mut s);
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let l2 = id_of(&exec(&mut s, "layer.create", json!({ "name": "detail" })));
    assert_eq!(
        err_code(&exec(&mut s, "layer.create", json!({ "name": "detail" }))),
        "InvalidArgument"
    );
    assert_eq!(
        err_code(&exec(&mut s, "layer.delete", json!({ "id": &main }))),
        "InvalidArgument"
    );

    ok(&exec(
        &mut s,
        "object.set_layer",
        json!({ "ids": [&b], "layer": &l2 }),
    ));
    assert_eq!(full(&mut s, &b)["layer"], json!(l2));
    assert_eq!(
        err_code(&exec(&mut s, "layer.delete", json!({ "id": &l2 }))),
        "InvalidArgument"
    );

    let r = exec(
        &mut s,
        "layer.delete",
        json!({ "id": &l2, "move_to": &main }),
    );
    assert_eq!(ok(&r)["result"]["moved"], 1);
    assert_eq!(full(&mut s, &b)["layer"], json!(main));
    assert_eq!(summary(&mut s)["counts"]["layer"], 1);
    exec(&mut s, "history.undo", json!({}));
    assert_eq!(summary(&mut s)["counts"]["layer"], 2);
}

#[test]
fn set_layer_carries_contents_and_checks_parents() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let g = id_of(&exec(&mut s, "group.create", json!({ "objects": [&a] })));
    let l2 = id_of(&exec(&mut s, "layer.create", json!({ "name": "two" })));
    assert_eq!(
        err_code(&exec(
            &mut s,
            "object.set_layer",
            json!({ "ids": [&a], "layer": &l2 })
        )),
        "InvalidArgument"
    );
    ok(&exec(
        &mut s,
        "object.set_layer",
        json!({ "ids": [&g], "layer": &l2 }),
    ));
    assert_eq!(full(&mut s, &a)["layer"], json!(l2));
    assert_eq!(full(&mut s, &g)["layer"], json!(l2));
}

#[test]
fn rename_group_layer_camera() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let g = id_of(&exec(&mut s, "group.create", json!({ "objects": [&a] })));
    ok(&exec(
        &mut s,
        "object.rename",
        json!({ "id": &g, "name": "walls" }),
    ));
    assert_eq!(full(&mut s, &g)["name"], "walls");
    let main = layer_id(&mut s);
    ok(&exec(
        &mut s,
        "object.rename",
        json!({ "id": &main, "name": "world" }),
    ));
    let cam = id_of(&exec(
        &mut s,
        "view.camera.create",
        json!({ "name": "cam-one", "position": [0, 0, 0] }),
    ));
    ok(&exec(
        &mut s,
        "object.rename",
        json!({ "id": &cam, "name": "overview" }),
    ));
    assert_eq!(
        err_code(&exec(
            &mut s,
            "object.rename",
            json!({ "id": &a, "name": "x" })
        )),
        "WrongKind"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "object.rename",
            json!({ "id": &g, "name": "" })
        )),
        "InvalidArgument"
    );
}

#[test]
fn duplicate_deep_copies_with_offset() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    let g = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [&a, &b] }),
    ));
    let r = exec(
        &mut s,
        "object.duplicate",
        json!({ "ids": [&g], "offset": [0, 100, 0] }),
    );
    let ids = ids_of(&r);
    assert_eq!(ids.len(), 3);
    let copy = &ids[0];
    assert_ne!(copy, &g);
    assert_eq!(r["result"]["map"][&g], json!(copy));
    let children: Vec<_> = ids[1..].to_vec();
    for c in &children {
        assert_eq!(
            full(&mut s, c)["group"],
            json!(copy),
            "children belong to the copied group"
        );
    }
    assert_eq!(bounds(&mut s, &children[0]).0[1], 100.0);
    assert_eq!(bounds(&mut s, &a).0[1], 0.0);
    exec(&mut s, "history.undo", json!({}));
    assert_eq!(summary(&mut s)["counts"]["brush"], 2);
}

#[test]
fn duplicate_brush_entity_and_point_entity() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let ent = id_of(&exec(
        &mut s,
        "entity.create_brush_entity",
        json!({ "classname": "func_door", "objects": [&a] }),
    ));
    let light = id_of(&exec(
        &mut s,
        "entity.create",
        json!({ "classname": "light", "origin": [0, 0, 0] }),
    ));
    let r = exec(
        &mut s,
        "object.duplicate",
        json!({ "ids": [&ent, &light], "offset": [64, 0, 0] }),
    );
    let ids = ids_of(&r);
    assert_eq!(ids.len(), 3);
    let new_brush = &ids[2];
    assert_eq!(full(&mut s, new_brush)["entity"], json!(ids[0]));
    assert_eq!(full(&mut s, &ids[1])["origin"], json!([64.0, 0.0, 0.0]));
    assert_eq!(full(&mut s, &a)["entity"], json!(ent));
    let layer = layer_id(&mut s);
    assert_eq!(
        err_code(&exec(&mut s, "object.duplicate", json!({ "ids": [layer] }))),
        "WrongKind"
    );
}

fn array(s: &mut editor_cmd::Session, args: serde_json::Value) -> serde_json::Value {
    exec(s, "object.array", args)
}

#[test]
fn array_makes_offset_copies_with_a_map() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let r = array(
        &mut s,
        json!({ "ids": [b], "count": 3, "offset": [32, 0, 0] }),
    );
    let ids = ids_of(&r);
    assert_eq!(ids.len(), 3);
    assert_eq!(r["result"]["copy_size"], 1);
    assert_eq!(r["result"]["map"][&b], json!(ids));
    for (k, id) in ids.iter().enumerate() {
        let (lo, hi) = bounds(&mut s, id);
        assert!(near(lo, [32.0 * (k + 1) as f64, 0.0, 0.0]));
        assert!(near(hi, [32.0 * (k + 1) as f64 + 8.0, 8.0, 8.0]));
    }
    assert_eq!(summary(&mut s)["counts"]["brush"], 4);
}

#[test]
fn array_copies_group_and_entity_contents() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let c = make_box(&mut s, [0, 20, 0], [8, 28, 8]);
    let ent = id_of(&exec(
        &mut s,
        "entity.create_brush_entity",
        json!({ "classname": "func_door", "objects": [c] }),
    ));
    let g = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [a, ent] }),
    ));
    let r = array(
        &mut s,
        json!({ "ids": [g], "count": 2, "offset": [0, 0, 16] }),
    );
    assert_eq!(r["result"]["copy_size"], 4);
    assert_eq!(ids_of(&r).len(), 8);
    let summary = summary(&mut s);
    assert_eq!(summary["counts"]["brush"], 6);
    assert_eq!(summary["counts"]["group"], 3);
    assert_eq!(summary["counts"]["entity"], 3);
    let new_g = r["result"]["map"][&g][1].as_str().unwrap().to_string();
    let b = exec(&mut s, "scene.bounds", json!({ "ids": [new_g] }));
    assert_eq!(
        b["result"]["combined"],
        json!({ "min": [0.0, 0.0, 32.0], "max": [8.0, 28.0, 40.0] })
    );
    assert_eq!(
        exec(&mut s, "check.validate", json!({}))["result"]["errors"],
        0
    );
}

#[test]
fn array_mirror_adds_a_mirrored_row() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let r = array(
        &mut s,
        json!({ "ids": [b], "count": 2, "offset": [16, 0, 0],
                "mirror": { "axis": "x", "pivot": [100, 0, 0] } }),
    );
    let ids = ids_of(&r);
    assert_eq!(ids.len(), 5);
    assert_eq!(r["result"]["map"][&b].as_array().unwrap().len(), 5);
    let expect = [[192.0, 200.0], [176.0, 184.0], [160.0, 168.0]];
    for (id, x) in ids[2..].iter().zip(expect) {
        let (lo, hi) = bounds(&mut s, id);
        assert!(near(lo, [x[0], 0.0, 0.0]), "{lo:?}");
        assert!(near(hi, [x[1], 8.0, 8.0]), "{hi:?}");
    }
    assert_eq!(
        exec(&mut s, "check.validate", json!({}))["result"]["errors"],
        0
    );
    assert_eq!(
        exec(&mut s, "scene.overlaps", json!({}))["result"]["count"],
        0
    );
}

#[test]
fn array_is_one_undo_and_dry_run_changes_nothing() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let args = json!({ "ids": [b], "count": 4, "offset": [16, 0, 0],
                       "mirror": { "axis": "y", "pivot": [0, 0, 0] } });
    let v = s
        .execute_value(
            json!({ "op": "object.array", "args": args, "dry_run": true }),
            "test",
        )
        .to_value();
    ok(&v);
    assert_eq!(summary(&mut s)["counts"]["brush"], 1);
    ok(&array(&mut s, args));
    assert_eq!(summary(&mut s)["counts"]["brush"], 10);
    ok(&exec(&mut s, "history.undo", json!({})));
    assert_eq!(summary(&mut s)["counts"]["brush"], 1);
}

#[test]
fn array_errors() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    for (args, field) in [
        (
            json!({ "ids": [b], "count": 0, "offset": [8, 0, 0] }),
            "count",
        ),
        (
            json!({ "ids": [b], "count": 257, "offset": [8, 0, 0] }),
            "count",
        ),
        (
            json!({ "ids": [b], "count": 2, "offset": [0, 0, 0] }),
            "offset",
        ),
        (json!({ "ids": [], "count": 2, "offset": [8, 0, 0] }), "ids"),
        (
            json!({ "ids": [b], "count": 2, "offset": [8, 0, 0],
                    "mirror": { "axis": [0, 0, 0], "pivot": [0, 0, 0] } }),
            "mirror.axis",
        ),
    ] {
        let v = array(&mut s, args);
        assert_eq!(err_code(&v), "InvalidArgument", "{field}");
        assert_eq!(v["field"], field);
    }
    let v = array(
        &mut s,
        json!({ "ids": [b], "count": 3, "offset": [400000, 0, 0] }),
    );
    assert_eq!(err_code(&v), "InvalidGeometry");
    let v = array(
        &mut s,
        json!({ "ids": ["b99"], "count": 2, "offset": [8, 0, 0] }),
    );
    assert_eq!(err_code(&v), "NotFound");
    assert_eq!(summary(&mut s)["counts"]["brush"], 1);
}

#[test]
fn array_object_limit() {
    let mut s = session();
    let ids: Vec<String> = (0..40)
        .map(|i| make_box(&mut s, [i * 16, 0, 0], [i * 16 + 8, 8, 8]))
        .collect();
    let v = array(
        &mut s,
        json!({ "ids": ids, "count": 256, "offset": [0, 0, 1] }),
    );
    assert_eq!(err_code(&v), "InvalidArgument");
    assert_eq!(v["field"], "count");
}

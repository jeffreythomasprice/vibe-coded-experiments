mod common;

use common::*;
use editor_cmd::Session;
use editor_core::*;
use serde_json::json;

#[test]
fn create_box_and_get() {
    let mut s = session();
    let r = exec(
        &mut s,
        "brush.create_box",
        json!({ "min": [0, 0, 0], "max": [64, 32, 16], "material": "base/x" }),
    );
    let id = id_of(&r);
    assert_eq!(r["changes"]["created"], json!([id]));
    assert_eq!(r["revision"], 1);

    let g = exec(&mut s, "scene.get", json!({ "ids": [id] }));
    let obj = &ok(&g)["result"]["objects"][0];
    assert_eq!(obj["kind"], "brush");
    assert_eq!(obj["faces"], 6);
    assert_eq!(obj["bounds"]["max"], json!([64.0, 32.0, 16.0]));
}

#[test]
fn create_box_rejects_bad_args() {
    let mut s = session();
    let r = exec(
        &mut s,
        "brush.create_box",
        json!({ "min": [0, 0, 0], "max": [0, 5, 5] }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    assert_eq!(r["field"], "max");

    let r = exec(&mut s, "brush.create_box", json!({ "min": [0, 0, 0] }));
    assert_eq!(err_code(&r), "InvalidArgument");
    assert_eq!(r["field"], "max");

    let r = exec(
        &mut s,
        "brush.create_box",
        json!({ "min": [0, 0, 0], "max": [1, 1, 1], "bogus": 1 }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");

    let r = exec(
        &mut s,
        "brush.create_box",
        json!({ "min": [0, 0, 0], "max": [1, 1, 1], "layer": "l99" }),
    );
    assert_eq!(err_code(&r), "NotFound");

    let r = exec(&mut s, "nope", json!({}));
    assert_eq!(err_code(&r), "InvalidArgument");
    assert!(r["hint"].as_str().unwrap().contains("brush.create_box"));
    assert_eq!(s.revision(), 0);
}

#[test]
fn create_prism_axes() {
    let mut s = session();
    for (axis, expect) in [
        ("z", json!([[-8.0, -8.0, -32.0], [8.0, 8.0, 32.0]])),
        ("x", json!([[-32.0, -8.0, -8.0], [32.0, 8.0, 8.0]])),
        ("y", json!([[-8.0, -32.0, -8.0], [8.0, 32.0, 8.0]])),
    ] {
        let id = id_of(&exec(
            &mut s,
            "brush.create_prism",
            json!({ "center": [0, 0, 0], "radius": 8, "sides": 4, "height": 64, "axis": axis }),
        ));
        let g = exec(&mut s, "scene.get", json!({ "ids": [id] }));
        let b = &g["result"]["objects"][0]["bounds"];
        let close = |a: &serde_json::Value, e: &serde_json::Value| {
            for i in 0..3 {
                let d = a[i].as_f64().unwrap() - e[i].as_f64().unwrap();
                assert!(d.abs() < 1e-6, "{axis}: {a} vs {e}");
            }
        };
        close(&b["min"], &expect[0]);
        close(&b["max"], &expect[1]);
    }
}

#[test]
fn clip_keep_both_splits_volume() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [64, 64, 64]);
    let r = exec(
        &mut s,
        "brush.clip",
        json!({ "ids": [b], "plane": [[32, 0, 0], [32, 64, 0], [32, 0, 64]], "keep": "both" }),
    );
    let new_ids = ids_of(&r);
    assert_eq!(new_ids.len(), 1);
    assert_eq!(r["changes"]["modified"], json!([b]));

    let mut xs = Vec::new();
    for id in [b.clone(), new_ids[0].clone()] {
        let g = exec(&mut s, "scene.get", json!({ "ids": [id] }));
        let bb = &g["result"]["objects"][0]["bounds"];
        xs.push((
            bb["min"][0].as_f64().unwrap(),
            bb["max"][0].as_f64().unwrap(),
        ));
    }
    xs.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(xs, vec![(0.0, 32.0), (32.0, 64.0)]);
}

#[test]
fn clip_keep_one_side_and_errors() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [64, 64, 64]);
    let plane = json!([[32, 0, 0], [32, 64, 0], [32, 0, 64]]);
    let r = exec(
        &mut s,
        "brush.clip",
        json!({ "ids": [b], "plane": plane, "keep": "front" }),
    );
    ok(&r);
    assert_eq!(r["changes"]["modified"], json!([b]));

    let r = exec(
        &mut s,
        "brush.clip",
        json!({ "ids": [b], "plane": plane, "keep": "front" }),
    );
    ok(&r);
    assert_eq!(r["warnings"].as_array().unwrap().len(), 1);

    let other = json!([[100, 0, 0], [100, 64, 0], [100, 0, 64]]);
    let r = exec(
        &mut s,
        "brush.clip",
        json!({ "ids": [b], "plane": other, "keep": "both" }),
    );
    assert_eq!(r["warnings"].as_array().unwrap().len(), 1);
    assert!(r["changes"]["modified"].as_array().unwrap().is_empty());

    let degenerate = json!([[0, 0, 0], [1, 0, 0], [2, 0, 0]]);
    let r = exec(
        &mut s,
        "brush.clip",
        json!({ "ids": [b], "plane": degenerate, "keep": "both" }),
    );
    assert_eq!(err_code(&r), "InvalidGeometry");

    let r = exec(
        &mut s,
        "brush.clip",
        json!({ "ids": ["l1"], "plane": plane, "keep": "both" }),
    );
    assert_eq!(err_code(&r), "WrongKind");
    let r = exec(
        &mut s,
        "brush.clip",
        json!({ "ids": ["b999"], "plane": plane, "keep": "both" }),
    );
    assert_eq!(err_code(&r), "NotFound");
}

#[test]
fn subtract_brushes_into_fragments() {
    let mut s = session();
    let target = make_box(&mut s, [0, 0, 0], [64, 64, 64]);
    let cutter = make_box(&mut s, [16, 16, 16], [48, 48, 48]);
    let r = exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [target], "cutters": [cutter] }),
    );
    let created = ids_of(&r);
    assert_eq!(created.len(), 6);
    assert_eq!(r["changes"]["deleted"], json!([target]));
    assert_eq!(r["changes"]["created"].as_array().unwrap().len(), 6);

    let v = exec(&mut s, "scene.get", json!({ "ids": [cutter] }));
    ok(&v);
    let c = exec(&mut s, "check.validate", json!({}));
    assert_eq!(c["result"]["errors"], 0);
}

#[test]
fn subtract_untouched_target_is_noop() {
    let mut s = session();
    let target = make_box(&mut s, [0, 0, 0], [16, 16, 16]);
    let cutter = make_box(&mut s, [100, 100, 100], [110, 110, 110]);
    let before = s.revision();
    let r = exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [target], "cutters": [cutter] }),
    );
    ok(&r);
    assert!(r["changes"]["deleted"].as_array().unwrap().is_empty());
    assert_eq!(s.revision(), before);
}

#[test]
fn subtract_single_fragment_keeps_id() {
    let mut s = session();
    let target = make_box(&mut s, [0, 0, 0], [64, 64, 64]);
    let cutter = make_box(&mut s, [32, -16, -16], [128, 128, 128]);
    let r = exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [target], "cutters": [cutter] }),
    );
    ok(&r);
    assert_eq!(r["changes"]["modified"], json!([target]));
    assert!(r["changes"]["created"].as_array().unwrap().is_empty());
}

#[test]
fn donut_mesh_subtract_is_valid() {
    let mut s = session();
    let outer = id_of(&exec(
        &mut s,
        "brush.create_prism",
        json!({ "center": [0, 0, 0], "radius": 64, "sides": 8, "height": 32 }),
    ));
    let inner = id_of(&exec(
        &mut s,
        "brush.create_prism",
        json!({ "center": [0, 0, 0], "radius": 24, "sides": 8, "height": 64 }),
    ));
    let r = exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [outer], "cutters": [inner], "output": "mesh" }),
    );
    let ids = ids_of(&r);
    assert!(ids[0].starts_with('m'));
    assert_eq!(r["changes"]["deleted"], json!([outer]));

    let c = exec(&mut s, "check.validate", json!({ "ids": [ids[0]] }));
    assert_eq!(c["result"]["errors"], 0, "{c}");
    assert_eq!(c["result"]["warnings"], 0, "{c}");
}

#[test]
fn subtract_argument_errors() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let r = exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [a], "cutters": [a] }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    let r = exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [], "cutters": [a] }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
}

fn doc_with_entity() -> (Session, EntityId, BrushId, LayerId) {
    let mut doc = Document::new("t");
    let layer = *doc.layers.keys().next().unwrap();
    let eid = EntityId(doc.alloc_id());
    doc.entities.insert(
        eid,
        Entity {
            flags: Default::default(),
            id: eid,
            classname: "func_door".into(),
            layer,
            group: None,
            origin: None,
            angles: DVec3::ZERO,
            properties: Properties::new(),
            extra: Extra::new(),
        },
    );
    let bid = BrushId(doc.alloc_id());
    doc.brushes.insert(
        bid,
        BrushObject {
            flags: Default::default(),
            id: bid,
            layer,
            group: None,
            entity: Some(eid),
            brush: editor_geom::shapes::cuboid(DVec3::ZERO, DVec3::splat(8.0)),
            extra: Extra::new(),
        },
    );
    (Session::new(doc), eid, bid, layer)
}

#[test]
fn delete_cascades_from_entity() {
    let (mut s, eid, bid, _) = doc_with_entity();
    let r = exec(&mut s, "object.delete", json!({ "ids": [eid.to_string()] }));
    ok(&r);
    let mut deleted: Vec<String> = serde_json::from_value(r["changes"]["deleted"].clone()).unwrap();
    deleted.sort();
    let mut expect = vec![eid.to_string(), bid.to_string()];
    expect.sort();
    assert_eq!(deleted, expect);
    assert!(s.document().brushes.is_empty());
}

#[test]
fn delete_layer_rules() {
    let (mut s, _, _, layer) = doc_with_entity();
    let r = exec(
        &mut s,
        "object.delete",
        json!({ "ids": [layer.to_string()] }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    assert!(r["message"].as_str().unwrap().contains("still contains"));

    let mut s = session();
    let r = exec(&mut s, "object.delete", json!({ "ids": ["l1"] }));
    assert_eq!(err_code(&r), "InvalidArgument");
    assert!(
        r["message"]
            .as_str()
            .unwrap()
            .contains("at least one layer")
    );

    let r = exec(&mut s, "object.delete", json!({ "ids": ["b77"] }));
    assert_eq!(err_code(&r), "NotFound");
}

#[test]
fn validate_reports_issues() {
    let (mut s, eid, _, _) = doc_with_entity();
    let r = exec(&mut s, "object.delete", json!({ "ids": [] }));
    assert_eq!(err_code(&r), "InvalidArgument");

    let mut doc = s.into_document();
    doc.brushes.clear();
    let e = doc.entities.get_mut(&eid).unwrap();
    e.properties
        .insert("target".into(), PropertyValue::String("nowhere".into()));
    let mut s = Session::new(doc);
    let c = exec(&mut s, "check.validate", json!({}));
    let codes: Vec<&str> = c["result"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"empty_brush_entity"), "{codes:?}");
    assert!(codes.contains(&"missing_target"), "{codes:?}");
}

#[test]
fn validate_flags_duplicates() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let c = exec(&mut s, "check.validate", json!({}));
    assert_eq!(c["result"]["issues"][0]["code"], "duplicate_object");
}

#[test]
fn scene_summary() {
    let (mut s, _, _, _) = doc_with_entity();
    let r = exec(&mut s, "scene.summary", json!({}));
    let res = &ok(&r)["result"];
    assert_eq!(res["counts"]["brush"], 1);
    assert_eq!(res["counts"]["entity"], 1);
    assert_eq!(res["classnames"]["func_door"], 1);
    assert_eq!(res["layers"][0]["objects"], 2);
    assert_eq!(res["bounds"]["max"], json!([8.0, 8.0, 8.0]));
}

#[test]
fn create_wedge_rises_toward_each_direction() {
    for (dir, probe_high, probe_low) in [
        ("+x", [95.0, 50.0], [5.0, 50.0]),
        ("-x", [5.0, 50.0], [95.0, 50.0]),
        ("+y", [50.0, 95.0], [50.0, 5.0]),
        ("-y", [50.0, 5.0], [50.0, 95.0]),
    ] {
        let mut s = session();
        let r = exec(
            &mut s,
            "brush.create_wedge",
            json!({ "min": [0, 0, 0], "max": [100, 100, 40], "direction": dir }),
        );
        ok(&r);
        let id = id_of(&r);
        assert_eq!(bounds(&mut s, &id), ([0.0; 3], [100.0, 100.0, 40.0]));
        assert_eq!(
            full(&mut s, &id)["brush"]["faces"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        let top = |s: &mut editor_cmd::Session, p: [f64; 2]| {
            let hit = exec(
                s,
                "scene.raycast",
                json!({ "origin": [p[0], p[1], 100], "dir": [0, 0, -1] }),
            );
            100.0 - hit["result"]["hit"]["distance"].as_f64().unwrap()
        };
        assert!((top(&mut s, probe_high) - 38.0).abs() < 1e-6, "{dir}");
        assert!((top(&mut s, probe_low) - 2.0).abs() < 1e-6, "{dir}");
        assert_eq!(
            exec(&mut s, "check.validate", json!({}))["result"]["errors"],
            0
        );
    }
}

#[test]
fn create_wedge_rejects_bad_boxes() {
    let mut s = session();
    let v = exec(
        &mut s,
        "brush.create_wedge",
        json!({ "min": [0, 0, 0], "max": [100, 0, 40], "direction": "+x" }),
    );
    assert_eq!(err_code(&v), "InvalidArgument");
    let v = exec(
        &mut s,
        "brush.create_wedge",
        json!({ "min": [0, 0, 0], "max": [10, 10, 10], "direction": "up" }),
    );
    assert_eq!(err_code(&v), "InvalidArgument");
}

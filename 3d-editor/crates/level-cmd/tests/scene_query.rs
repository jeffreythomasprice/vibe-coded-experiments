#![allow(clippy::cloned_ref_to_slice_refs)]
mod common;

use common::*;
use serde_json::json;

fn ids(v: &serde_json::Value, key: &str) -> Vec<String> {
    ok(v)["result"][key]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            x.as_str()
                .map(str::to_string)
                .unwrap_or_else(|| x["id"].as_str().unwrap().to_string())
        })
        .collect()
}

#[test]
fn list_filters_and_pages() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [100, 0, 0], [108, 8, 8]);
    let e = id_of(&exec(
        &mut s,
        "entity.create",
        json!({ "classname": "light", "origin": [4, 4, 4] }),
    ));
    let _ = id_of(&exec(
        &mut s,
        "entity.create",
        json!({ "classname": "spawn", "origin": [200, 0, 0] }),
    ));

    let brushes = exec(&mut s, "scene.list", json!({ "kind": "brush" }));
    assert_eq!(ids(&brushes, "objects"), [a.clone(), b.clone()]);
    assert_eq!(
        ids(
            &exec(&mut s, "scene.list", json!({ "classname": "light" })),
            "objects"
        ),
        [e.clone()]
    );
    assert_eq!(
        ids(
            &exec(
                &mut s,
                "scene.list",
                json!({ "intersects": { "min": [-1,-1,-1], "max": [10,10,10] } })
            ),
            "objects"
        ),
        [a.clone(), e.clone()]
    );
    assert_eq!(
        ids(
            &exec(
                &mut s,
                "scene.list",
                json!({ "within": { "min": [-1,-1,-1], "max": [10,10,10] }, "kind": "brush" })
            ),
            "objects"
        ),
        [a.clone()]
    );

    let page1 = exec(&mut s, "scene.list", json!({ "kind": "brush", "limit": 1 }));
    assert_eq!(ids(&page1, "objects"), [a.clone()]);
    let cursor = page1["result"]["next_cursor"].clone();
    assert_eq!(cursor, json!(a));
    let page2 = exec(
        &mut s,
        "scene.list",
        json!({ "kind": "brush", "limit": 1, "cursor": cursor }),
    );
    assert_eq!(ids(&page2, "objects"), [b.clone()]);
    assert!(page2["result"]["next_cursor"].is_null());

    let slim = exec(
        &mut s,
        "scene.list",
        json!({ "kind": "brush", "limit": 1, "fields": ["faces"] }),
    );
    let obj = &slim["result"]["objects"][0];
    assert!(obj.get("faces").is_some() && obj.get("bounds").is_none() && obj.get("id").is_some());

    assert_eq!(
        err_code(&exec(&mut s, "scene.list", json!({ "limit": 0 }))),
        "InvalidArgument"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "scene.list",
            json!({ "within": { "min": [5,5,5], "max": [1,1,1] } })
        )),
        "InvalidArgument"
    );
}

#[test]
fn list_by_layer_group_and_name() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let _b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    let g = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [&a], "name": "walls" }),
    ));
    assert_eq!(
        ids(
            &exec(&mut s, "scene.list", json!({ "group": &g })),
            "objects"
        ),
        [a.clone()]
    );
    assert_eq!(
        ids(
            &exec(&mut s, "scene.list", json!({ "name": "walls" })),
            "objects"
        ),
        [g.clone()]
    );
    let layer = layer_id(&mut s);
    assert_eq!(
        exec(
            &mut s,
            "scene.list",
            json!({ "layer": layer, "kind": "brush" })
        )["result"]["objects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn bounds_each_and_combined() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 4]);
    let layer = layer_id(&mut s);
    let r = exec(&mut s, "scene.bounds", json!({ "ids": [&a, &b, layer] }));
    assert_eq!(
        ok(&r)["result"]["combined"],
        json!({ "min": [0.0, 0.0, 0.0], "max": [24.0, 8.0, 8.0] })
    );
    assert!(r["result"]["bounds"][2]["bounds"].is_null());
    assert_eq!(
        err_code(&exec(&mut s, "scene.bounds", json!({ "ids": ["b99"] }))),
        "NotFound"
    );
}

#[test]
fn raycast_hits_brush_mesh_and_entity() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let hit = exec(
        &mut s,
        "scene.raycast",
        json!({ "origin": [4, 4, 50], "dir": [0, 0, -1] }),
    );
    let h = &ok(&hit)["result"]["hit"];
    assert_eq!(h["id"], json!(a));
    assert_eq!(h["distance"], 42.0);
    assert_eq!(h["point"], json!([4.0, 4.0, 8.0]));
    assert_eq!(h["normal"], json!([0.0, 0.0, 1.0]));
    assert!(h["face"].as_str().unwrap().starts_with(&format!("{a}/f")));

    assert!(
        exec(
            &mut s,
            "scene.raycast",
            json!({ "origin": [40, 4, 50], "dir": [0, 0, -1] })
        )["result"]["hit"]
            .is_null()
    );
    assert!(
        exec(
            &mut s,
            "scene.raycast",
            json!({ "origin": [4, 4, 50], "dir": [0, 0, -1], "max_dist": 10 })
        )["result"]["hit"]
            .is_null()
    );
    assert!(
        exec(
            &mut s,
            "scene.raycast",
            json!({ "origin": [4, 4, 50], "dir": [0, 0, -1], "ignore": [&a] })
        )["result"]["hit"]
            .is_null()
    );

    let m = ids_of(&exec(&mut s, "brush.to_mesh", json!({ "ids": [&a] })))[0].clone();
    let h = &exec(
        &mut s,
        "scene.raycast",
        json!({ "origin": [4, 4, 50], "dir": [0, 0, -2], "kinds": ["mesh"] }),
    )["result"]["hit"];
    assert_eq!(h["id"], json!(m));
    assert!((h["distance"].as_f64().unwrap() - 42.0).abs() < 1e-9);

    let e = id_of(&exec(
        &mut s,
        "entity.create",
        json!({ "classname": "light", "origin": [100, 0, 0] }),
    ));
    let h = &exec(
        &mut s,
        "scene.raycast",
        json!({ "origin": [100, -50, 0], "dir": [0, 1, 0] }),
    )["result"]["hit"];
    assert_eq!(h["id"], json!(e));
    assert!(h["face"].is_null());
    assert_eq!(h["distance"], 42.0);

    assert_eq!(
        err_code(&exec(
            &mut s,
            "scene.raycast",
            json!({ "origin": [0,0,0], "dir": [0,0,0] })
        )),
        "InvalidArgument"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "scene.raycast",
            json!({ "origin": [0,0,0], "dir": [1,0,0], "kinds": ["group"] })
        )),
        "InvalidArgument"
    );
}

#[test]
fn query_box_modes() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [6, 0, 0], [30, 8, 8]);
    let mut q = |mode: &str| {
        exec(
            &mut s,
            "scene.query_box",
            json!({ "min": [-1,-1,-1], "max": [10,10,10], "mode": mode }),
        )
    };
    assert_eq!(ids(&q("intersects"), "ids"), [a.clone(), b.clone()]);
    assert_eq!(ids(&q("contains"), "ids"), [a.clone()]);
    assert_eq!(
        err_code(&exec(
            &mut s,
            "scene.query_box",
            json!({ "min": [1,1,1], "max": [0,0,0] })
        )),
        "InvalidArgument"
    );
}

#[test]
fn overlaps_exact_for_brushes_and_approx_for_meshes() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [4, 4, 4], [12, 12, 12]);
    let _touching = make_box(&mut s, [8, 0, 0], [16, 8, 8]);
    let far = make_box(&mut s, [100, 100, 100], [108, 108, 108]);
    let r = exec(&mut s, "scene.overlaps", json!({}));
    let pairs = ok(&r)["result"]["pairs"].as_array().unwrap();
    assert!(
        pairs
            .iter()
            .any(|p| p["a"] == json!(a) && p["b"] == json!(b) && p["approx"] == false),
        "{pairs:?}"
    );
    assert!(
        pairs
            .iter()
            .all(|p| p["a"] != json!(far) && p["b"] != json!(far))
    );
    assert_eq!(
        r["result"]["count"], 2,
        "a-b and b-touching; a only touches the other: {pairs:?}"
    );

    let m = ids_of(&exec(&mut s, "brush.to_mesh", json!({ "ids": [&b] })))[0].clone();
    let r = exec(&mut s, "scene.overlaps", json!({ "ids": [&a, &m] }));
    assert_eq!(r["result"]["pairs"][0]["approx"], true);
    let layer = layer_id(&mut s);
    assert_eq!(
        err_code(&exec(&mut s, "scene.overlaps", json!({ "ids": [layer] }))),
        "WrongKind"
    );
}

#[test]
fn summary_reports_selection_size() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    exec(&mut s, "select.set", json!({ "ids": [a] }));
    assert_eq!(summary(&mut s)["selection"], 1);
}

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
        json!({ "origin": [4, 4, 4], "components": { "light": {} } }),
    ));
    let _ = id_of(&exec(
        &mut s,
        "entity.create",
        json!({ "origin": [200, 0, 0] }),
    ));

    let brushes = exec(&mut s, "scene.list", json!({ "kind": "brush" }));
    assert_eq!(ids(&brushes, "objects"), [a.clone(), b.clone()]);
    assert_eq!(
        ids(
            &exec(&mut s, "scene.list", json!({ "component": "light" })),
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
    assert_eq!(r["result"]["bounds"][2]["bounds"], r["result"]["combined"]);
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
        json!({ "origin": [100, 0, 0] }),
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

#[test]
fn bounds_of_groups_entities_and_layers() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [100, 0, 0], [108, 8, 8]);
    let c = make_box(&mut s, [0, 50, 0], [8, 58, 8]);
    let inner = id_of(&exec(&mut s, "group.create", json!({ "objects": [b] })));
    let outer = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [a, inner] }),
    ));
    let ent = id_of(&exec(
        &mut s,
        "entity.create_brush_entity",
        json!({ "objects": [c] }),
    ));
    let r = exec(&mut s, "scene.bounds", json!({ "ids": [outer, ent] }));
    let items = &ok(&r)["result"]["bounds"];
    assert_eq!(
        items[0]["bounds"],
        json!({ "min": [0.0, 0.0, 0.0], "max": [108.0, 8.0, 8.0] })
    );
    assert_eq!(
        items[1]["bounds"],
        json!({ "min": [0.0, 50.0, 0.0], "max": [8.0, 58.0, 8.0] })
    );
    assert_eq!(
        r["result"]["combined"],
        json!({ "min": [0.0, 0.0, 0.0], "max": [108.0, 58.0, 8.0] })
    );
    let layer = layer_id(&mut s);
    let r = exec(&mut s, "scene.bounds", json!({ "ids": [layer] }));
    assert_eq!(
        ok(&r)["result"]["combined"],
        json!({ "min": [0.0, 0.0, 0.0], "max": [108.0, 58.0, 8.0] })
    );
}

#[test]
fn empty_group_and_layer_have_null_bounds() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let g = id_of(&exec(&mut s, "group.create", json!({ "objects": [a] })));
    exec(&mut s, "object.delete", json!({ "ids": [a] }));
    let l = id_of(&exec(&mut s, "layer.create", json!({ "name": "empty" })));
    let r = exec(&mut s, "scene.bounds", json!({ "ids": [g, l] }));
    assert!(ok(&r)["result"]["bounds"][0]["bounds"].is_null());
    assert!(r["result"]["bounds"][1]["bounds"].is_null());
    assert!(r["result"]["combined"].is_null());
}

fn pair_count(s: &mut editor_cmd::Session, extra: serde_json::Value) -> usize {
    let mut args = json!({});
    for (k, v) in extra.as_object().unwrap() {
        args[k] = v.clone();
    }
    ok(&exec(s, "scene.overlaps", args))["result"]["pairs"]
        .as_array()
        .unwrap()
        .len()
}

#[test]
fn ignore_touching_drops_thin_interpenetration() {
    let mut s = session();
    let step1 = make_box(&mut s, [0, 0, 0], [16, 32, 8]);
    let step2 = make_box(&mut s, [15, 0, 0], [32, 32, 16]);
    let step3 = make_box(&mut s, [28, 0, 0], [48, 32, 24]);
    let wall_a = make_box(&mut s, [100, 0, 0], [108, 64, 32]);
    let wall_b = make_box(&mut s, [107, 0, 0], [140, 8, 32]);
    assert_eq!(pair_count(&mut s, json!({})), 3);
    let r = exec(&mut s, "scene.overlaps", json!({ "ignore_touching": true }));
    let pairs = ok(&r)["result"]["pairs"].as_array().unwrap();
    assert_eq!(pairs.len(), 1, "{pairs:?}");
    assert_eq!(pairs[0]["a"], json!(step2));
    assert_eq!(pairs[0]["b"], json!(step3));
    assert_eq!(pairs[0]["depth"], 4.0);
    let all = exec(&mut s, "scene.overlaps", json!({}));
    let depths: Vec<f64> = all["result"]["pairs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["depth"].as_f64().unwrap())
        .collect();
    assert_eq!(depths, [1.0, 4.0, 1.0]);
    let _ = (step1, wall_a, wall_b);
}

#[test]
fn ignore_same_group_uses_the_direct_group() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [16, 16, 16]);
    let b = make_box(&mut s, [8, 8, 8], [24, 24, 24]);
    let c = make_box(&mut s, [4, 4, 4], [12, 12, 12]);
    let inner = id_of(&exec(&mut s, "group.create", json!({ "objects": [a, b] })));
    let outer = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [inner, c] }),
    ));
    assert_eq!(pair_count(&mut s, json!({})), 3);
    let r = exec(
        &mut s,
        "scene.overlaps",
        json!({ "ignore_same_group": true }),
    );
    let pairs = ok(&r)["result"]["pairs"].as_array().unwrap();
    assert_eq!(pairs.len(), 2, "{pairs:?}");
    assert!(
        pairs
            .iter()
            .all(|p| p["a"] != json!(a) || p["b"] != json!(b))
    );
    assert_eq!(
        pair_count(
            &mut s,
            json!({ "ignore_same_group": true, "ignore_touching": true })
        ),
        2
    );
    let _ = outer;
}

#[test]
fn ignore_same_group_keeps_ungrouped_pairs() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [16, 16, 16]);
    make_box(&mut s, [8, 8, 8], [24, 24, 24]);
    assert_eq!(pair_count(&mut s, json!({ "ignore_same_group": true })), 1);
}

#[test]
fn ray_hits_lists_every_hit_nearest_first() {
    use std::collections::BTreeSet;

    use editor_core::ObjectKind;
    use glam::DVec3;

    let mut s = session();
    let low = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let high = make_box(&mut s, [0, 0, 20], [8, 8, 28]);
    let kinds: BTreeSet<ObjectKind> = [ObjectKind::Brush].into();
    let origin = DVec3::new(4.0, 4.0, 50.0);
    let down = DVec3::NEG_Z;

    let hits = editor_cmd::ray_hits(s.document(), origin, down, &kinds, &BTreeSet::new(), None);
    let ids: Vec<String> = hits.iter().map(|h| h.id.to_string()).collect();
    assert_eq!(ids, [high.clone(), low.clone()]);
    assert!(hits[0].distance < hits[1].distance);

    let ignore = [high.parse().unwrap()].into();
    let hits = editor_cmd::ray_hits(s.document(), origin, down, &kinds, &ignore, None);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id.to_string(), low);

    let hits = editor_cmd::ray_hits(
        s.document(),
        origin,
        down,
        &kinds,
        &BTreeSet::new(),
        Some(25.0),
    );
    assert_eq!(hits.len(), 1);
}

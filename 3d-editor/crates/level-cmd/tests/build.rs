mod common;

use common::*;
use serde_json::{Value, json};

fn room(s: &mut level_cmd::Session, extra: Value) -> Value {
    let mut args = json!({ "min": [0,0,0], "max": [128,96,64], "wall": 8 });
    for (k, v) in extra.as_object().unwrap() {
        args[k] = v.clone();
    }
    exec(s, "room.create", args)
}

fn hit(s: &mut level_cmd::Session, origin: [f64; 3], dir: [f64; 3]) -> Value {
    exec(s, "scene.raycast", json!({ "origin": origin, "dir": dir }))["result"]["hit"].clone()
}

#[test]
fn room_is_hollow_and_watertight() {
    let mut s = session();
    let r = room(&mut s, json!({ "material": "wall" }));
    let ids = ids_of(&r);
    assert_eq!(ids.len(), 6);
    let roles = &r["result"]["roles"];
    for role in [
        "floor",
        "ceiling",
        "wall_north",
        "wall_south",
        "wall_east",
        "wall_west",
    ] {
        assert!(roles[role].is_string(), "{role}");
    }
    let c = [64.0, 48.0, 32.0];
    for (dir, role) in [
        ([1.0, 0.0, 0.0], "wall_east"),
        ([-1.0, 0.0, 0.0], "wall_west"),
        ([0.0, 1.0, 0.0], "wall_north"),
        ([0.0, -1.0, 0.0], "wall_south"),
        ([0.0, 0.0, 1.0], "ceiling"),
        ([0.0, 0.0, -1.0], "floor"),
    ] {
        let h = hit(&mut s, c, dir);
        assert_eq!(h["id"], roles[role], "{role}");
    }
    let overlaps = exec(&mut s, "scene.overlaps", json!({}));
    assert_eq!(overlaps["result"]["count"], 0);
    assert_eq!(
        exec(&mut s, "check.validate", json!({}))["result"]["errors"],
        0
    );
}

#[test]
fn room_materials_open_sides_and_groups() {
    let mut s = session();
    let r = room(
        &mut s,
        json!({ "floor_material": "f", "wall_material": "w", "material": "c",
                "open_sides": ["east", "west"], "name": "hall" }),
    );
    assert_eq!(ids_of(&r).len(), 4);
    assert!(r["result"]["roles"].get("wall_east").is_none());
    let group = r["result"]["group"].as_str().unwrap();
    let floor = full(&mut s, r["result"]["roles"]["floor"].as_str().unwrap());
    assert_eq!(floor["group"], group);
    let h = hit(&mut s, [64.0, 48.0, 32.0], [1.0, 0.0, 0.0]);
    assert!(h.is_null());

    let r2 = room(&mut s, json!({ "group": group }));
    assert_eq!(
        full(&mut s, r2["result"]["roles"]["floor"].as_str().unwrap())["group"],
        group
    );
}

#[test]
fn room_errors() {
    let mut s = session();
    for extra in [
        json!({ "wall": 0 }),
        json!({ "min": [10, 0, 0], "max": [0, 5, 5] }),
        json!({ "open_sides": ["floor", "ceiling", "north", "south", "east", "west"] }),
    ] {
        assert_eq!(err_code(&room(&mut s, extra)), "InvalidArgument");
    }
    assert!(s.document().brushes.is_empty());
}

#[test]
fn cut_opening_joins_two_rooms() {
    let mut s = session();
    let a = room(&mut s, json!({ "material": "a" }));
    let b = exec(
        &mut s,
        "room.create",
        json!({ "min": [200, 0, 0], "max": [328, 96, 64], "wall": 8, "material": "b" }),
    );
    let corridor = exec(
        &mut s,
        "room.create",
        json!({ "min": [136, 32, 0], "max": [200, 64, 48], "wall": 8,
                "open_sides": ["east", "west"] }),
    );
    ok(&corridor);
    let before = s.document().brushes.len();
    assert!(hit(&mut s, [100.0, 48.0, 20.0], [1.0, 0.0, 0.0])["id"].is_string());

    let cut = exec(
        &mut s,
        "room.cut_opening",
        json!({ "min": [120, 32, 0], "max": [208, 64, 48] }),
    );
    ok(&cut);
    assert!(s.document().brushes.len() >= before);
    let through = hit(&mut s, [100.0, 48.0, 20.0], [1.0, 0.0, 0.0]);
    assert_eq!(through["id"], b["result"]["roles"]["wall_east"]);
    let east_a = a["result"]["roles"]["wall_east"].as_str().unwrap();
    assert!(cut["result"]["map"].get(east_a).is_some());
    let west_b = b["result"]["roles"]["wall_west"].as_str().unwrap();
    assert!(cut["result"]["map"].get(west_b).is_some());
    assert_eq!(
        exec(&mut s, "check.validate", json!({}))["result"]["errors"],
        0
    );
}

#[test]
fn cut_opening_keeps_material_and_errors() {
    let mut s = session();
    let wall = make_box(&mut s, [0, 0, 0], [8, 64, 64]);
    exec(
        &mut s,
        "face.set_material",
        json!({ "faces": [wall], "material": "brick" }),
    );
    let r = exec(
        &mut s,
        "room.cut_opening",
        json!({ "min": [-4, 16, 0], "max": [12, 48, 32] }),
    );
    ok(&r);
    let fragments = r["result"]["map"][&wall].as_array().unwrap().clone();
    assert!(!fragments.is_empty());
    let f = full(&mut s, fragments[0].as_str().unwrap());
    assert!(f["brush"]["faces"].to_string().contains("brick"));
    assert!(
        !f["brush"]["faces"]
            .to_string()
            .contains("\"material\":\"\"")
    );

    let none = exec(
        &mut s,
        "room.cut_opening",
        json!({ "min": [500, 0, 0], "max": [510, 8, 8] }),
    );
    assert_eq!(err_code(&none), "InvalidArgument");

    let m = exec(
        &mut s,
        "mesh.create",
        json!({ "vertices": [[0,0,0],[8,0,0],[0,8,0]], "faces": [{ "verts": [0,1,2] }] }),
    );
    let bad = exec(
        &mut s,
        "room.cut_opening",
        json!({ "min": [0, 0, 0], "max": [4, 4, 4], "targets": [id_of(&m)] }),
    );
    assert_eq!(err_code(&bad), "InvalidArgument");
    assert!(bad["hint"].as_str().unwrap().contains("csg.subtract"));
}

#[test]
fn stairs_climb_without_overlap() {
    let mut s = session();
    let r = exec(
        &mut s,
        "stairs.create",
        json!({ "origin": [0, 0, 0], "direction": "+x", "steps": 4, "rise": 8, "run": 16,
                "width": 64, "material": "stone", "name": "dais" }),
    );
    let ids = ids_of(&r);
    assert_eq!(ids.len(), 4);
    assert_eq!(r["result"]["top"], json!([64.0, 0.0, 32.0]));
    let mut last_top = 0.0;
    for (i, id) in ids.iter().enumerate() {
        let (min, max) = bounds(&mut s, id);
        assert!(near(min, [16.0 * i as f64, -32.0, 0.0]), "{min:?}");
        assert!(
            near(max, [16.0 * (i + 1) as f64, 32.0, 8.0 * (i + 1) as f64]),
            "{max:?}"
        );
        assert!(max[2] > last_top);
        last_top = max[2];
    }
    assert_eq!(
        exec(&mut s, "scene.overlaps", json!({}))["result"]["count"],
        0
    );
    assert!(r["result"]["group"].is_string());
}

#[test]
fn stairs_other_directions_and_errors() {
    let mut s = session();
    let r = exec(
        &mut s,
        "stairs.create",
        json!({ "origin": [0, 0, 0], "direction": "-y", "steps": 2, "rise": 8, "run": 16, "width": 32 }),
    );
    let (min, max) = bounds(&mut s, &ids_of(&r)[1]);
    assert!(
        near(min, [-16.0, -32.0, 0.0]) && near(max, [16.0, -16.0, 16.0]),
        "{min:?} {max:?}"
    );
    for bad in [
        json!({ "steps": 0 }),
        json!({ "rise": 0 }),
        json!({ "direction": "up" }),
    ] {
        let mut args = json!({ "origin": [0,0,0], "direction": "+x", "steps": 2, "rise": 8, "run": 16, "width": 32 });
        for (k, v) in bad.as_object().unwrap() {
            args[k] = v.clone();
        }
        assert_eq!(
            err_code(&exec(&mut s, "stairs.create", args)),
            "InvalidArgument"
        );
    }
}

#[test]
fn build_ops_are_one_undoable_dry_runnable_step() {
    let mut s = session();
    let dry = s
        .execute_value(
            json!({ "op": "room.create", "dry_run": true,
                    "args": { "min": [0,0,0], "max": [64,64,64], "wall": 8 } }),
            "test",
        )
        .to_value();
    assert_eq!(ok(&dry)["changes"]["created"].as_array().unwrap().len(), 6);
    assert!(s.document().brushes.is_empty());

    room(&mut s, json!({}));
    assert_eq!(s.document().brushes.len(), 6);
    ok(&exec(&mut s, "history.undo", json!({})));
    assert!(s.document().brushes.is_empty());
}

#[test]
fn rooms_compose_with_batch_and_brush_entities() {
    let mut s = session();
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "room.create", "as": "hall", "args": { "min": [0,0,0], "max": [64,64,64], "wall": 8 } },
            { "op": "entity.create_brush_entity", "args": {
                "classname": "func_group", "objects": ["$hall.ids[0]", "$hall.ids[1]"] } },
        ]}),
    );
    ok(&r);
}

#[test]
fn open_ends_stop_at_the_interior() {
    let mut s = session();
    let a = room(&mut s, json!({}));
    let c = exec(
        &mut s,
        "room.create",
        json!({ "min": [136, 16, 0], "max": [200, 80, 48], "wall": 8,
                "open_sides": ["east", "west"] }),
    );
    let b = exec(
        &mut s,
        "room.create",
        json!({ "min": [208, 0, 0], "max": [336, 96, 64], "wall": 8 }),
    );
    ok(&a);
    ok(&b);
    for role in ["floor", "ceiling", "wall_north", "wall_south"] {
        let id = c["result"]["roles"][role].as_str().unwrap();
        let (lo, hi) = bounds(&mut s, id);
        assert_eq!((lo[0], hi[0]), (136.0, 200.0), "{role}");
    }
    assert_eq!(
        exec(&mut s, "scene.overlaps", json!({}))["result"]["count"],
        0
    );
    assert_eq!(
        exec(&mut s, "check.validate", json!({}))["result"]["errors"],
        0
    );
}

#[test]
fn open_north_south_stop_at_the_interior() {
    let mut s = session();
    let r = room(&mut s, json!({ "open_sides": ["north", "south"] }));
    for role in ["floor", "ceiling"] {
        let id = r["result"]["roles"][role].as_str().unwrap();
        let (lo, hi) = bounds(&mut s, id);
        assert_eq!((lo[1], hi[1]), (0.0, 96.0), "{role}");
        assert_eq!((lo[0], hi[0]), (-8.0, 136.0), "{role}");
    }
    for role in ["wall_east", "wall_west"] {
        let id = r["result"]["roles"][role].as_str().unwrap();
        let (lo, hi) = bounds(&mut s, id);
        assert_eq!((lo[1], hi[1]), (0.0, 96.0), "{role}");
    }
}

#[test]
fn closed_room_geometry_is_unchanged() {
    let mut s = session();
    let r = room(&mut s, json!({}));
    let expect = [
        ("floor", [-8.0, -8.0, -8.0], [136.0, 104.0, 0.0]),
        ("ceiling", [-8.0, -8.0, 64.0], [136.0, 104.0, 72.0]),
        ("wall_north", [-8.0, 96.0, 0.0], [136.0, 104.0, 64.0]),
        ("wall_south", [-8.0, -8.0, 0.0], [136.0, 0.0, 64.0]),
        ("wall_east", [128.0, 0.0, 0.0], [136.0, 96.0, 64.0]),
        ("wall_west", [-8.0, 0.0, 0.0], [0.0, 96.0, 64.0]),
    ];
    for (role, lo, hi) in expect {
        let id = r["result"]["roles"][role].as_str().unwrap();
        let (l, h) = bounds(&mut s, id);
        assert_eq!((l, h), (lo, hi), "{role}");
    }
}

mod common;

use common::*;
use serde_json::json;

fn flags(s: &mut editor_cmd::Session, id: &str) -> (bool, bool) {
    let o = full(s, id);
    let flag = |k: &str| o[k].as_bool().unwrap_or(false);
    (flag("hidden"), flag("locked"))
}

fn lock(s: &mut editor_cmd::Session, ids: &[&str], locked: bool) {
    ok(&exec(
        s,
        "object.set_locked",
        json!({ "ids": ids, "locked": locked }),
    ));
}

#[test]
fn locked_objects_refuse_edits_and_deletion() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [32, 0, 0], [40, 8, 8]);
    lock(&mut s, &[&a], true);
    assert_eq!(flags(&mut s, &a), (false, true));

    for (op, args) in [
        (
            "transform.translate",
            json!({ "ids": [&a], "delta": [1, 0, 0] }),
        ),
        ("object.delete", json!({ "ids": [&a] })),
        (
            "face.set_material",
            json!({ "faces": [format!("{a}/f1")], "material": "x" }),
        ),
        (
            "vertex.translate",
            json!({ "brush_vertices": [{ "brush": &a, "at": [8, 8, 8] }], "delta": [0, 0, 1] }),
        ),
    ] {
        let r = exec(&mut s, op, args);
        assert_eq!(err_code(&r), "Locked", "{op}: {r}");
        assert_eq!(r["target"], a.as_str());
        assert!(!r["hint"].as_str().unwrap().is_empty());
    }
    assert_eq!(bounds(&mut s, &a).0, [0.0, 0.0, 0.0]);

    ok(&exec(
        &mut s,
        "transform.translate",
        json!({ "ids": [&b], "delta": [1, 0, 0] }),
    ));

    lock(&mut s, &[&a], false);
    ok(&exec(
        &mut s,
        "transform.translate",
        json!({ "ids": [&a], "delta": [1, 0, 0] }),
    ));
}

#[test]
fn a_batch_with_a_locked_member_changes_nothing() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [32, 0, 0], [40, 8, 8]);
    lock(&mut s, &[&b], true);
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "transform.translate", "args": { "ids": [&a], "delta": [0, 0, 5] } },
            { "op": "transform.translate", "args": { "ids": [&b], "delta": [0, 0, 5] } },
        ] }),
    );
    assert_eq!(err_code(&r), "Locked");
    assert_eq!(r["index"], 1);
    assert_eq!(bounds(&mut s, &a).0[2], 0.0);
}

#[test]
fn a_batch_can_unlock_edit_and_relock() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    lock(&mut s, &[&a], true);
    ok(&exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "object.set_locked", "args": { "ids": [&a], "locked": false } },
            { "op": "transform.translate", "args": { "ids": [&a], "delta": [0, 0, 5] } },
            { "op": "object.set_locked", "args": { "ids": [&a], "locked": true } },
        ] }),
    ));
    assert_eq!(bounds(&mut s, &a).0[2], 5.0);
    assert_eq!(flags(&mut s, &a), (false, true));
}

#[test]
fn locking_a_container_locks_what_it_holds() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [32, 0, 0], [40, 8, 8]);
    let g = id_of(&exec(&mut s, "group.create", json!({ "objects": [&a] })));
    lock(&mut s, &[&g], true);

    let r = exec(
        &mut s,
        "transform.translate",
        json!({ "ids": [&a], "delta": [1, 0, 0] }),
    );
    assert_eq!(err_code(&r), "Locked");
    let r = exec(&mut s, "group.move", json!({ "objects": [&b], "group": g }));
    assert_eq!(err_code(&r), "Locked");
    let r = exec(&mut s, "group.move", json!({ "objects": [&a] }));
    assert_eq!(err_code(&r), "Locked");

    lock(&mut s, &[&g], false);
    let layer = layer_id(&mut s);
    lock(&mut s, &[&layer], true);
    let r = exec(
        &mut s,
        "brush.create_box",
        json!({ "min": [0, 0, 0], "max": [8, 8, 8] }),
    );
    assert_eq!(err_code(&r), "Locked");
    let r = exec(&mut s, "object.delete", json!({ "ids": [&b] }));
    assert_eq!(err_code(&r), "Locked");
}

#[test]
fn locking_is_undoable() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    lock(&mut s, &[&a], true);
    exec(&mut s, "history.undo", json!({}));
    assert_eq!(flags(&mut s, &a), (false, false));
    exec(&mut s, "history.redo", json!({}));
    assert_eq!(flags(&mut s, &a), (false, true));
}

#[test]
fn hide_show_and_isolate() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [32, 0, 0], [40, 8, 8]);
    let c = make_box(&mut s, [64, 0, 0], [72, 8, 8]);

    let r = exec(
        &mut s,
        "object.set_hidden",
        json!({ "ids": [&a], "hidden": true }),
    );
    assert_eq!(ok(&r)["result"]["updated"], 1);
    let r = exec(
        &mut s,
        "object.set_hidden",
        json!({ "ids": [&a], "hidden": true }),
    );
    assert_eq!(ok(&r)["result"]["updated"], 0);
    assert_eq!(flags(&mut s, &a), (true, false));
    exec(&mut s, "history.undo", json!({}));
    assert_eq!(flags(&mut s, &a), (false, false));

    let r = exec(&mut s, "object.isolate", json!({ "ids": [&b] }));
    assert_eq!(ok(&r)["result"]["updated"], 2);
    assert!(flags(&mut s, &a).0);
    assert!(!flags(&mut s, &b).0);
    assert!(flags(&mut s, &c).0);
    let layer = layer_id(&mut s);
    assert!(!flags(&mut s, &layer).0);

    let r = exec(&mut s, "object.show_all", json!({}));
    assert_eq!(ok(&r)["result"]["updated"], 2);
    assert!(!flags(&mut s, &a).0);
    assert!(!flags(&mut s, &c).0);
}

#[test]
fn isolating_in_a_group_keeps_the_group_and_hides_other_layers() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [32, 0, 0], [40, 8, 8]);
    let g = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [&a, &b] }),
    ));
    let other = id_of(&exec(&mut s, "layer.create", json!({ "name": "other" })));
    let c = id_of(&exec(
        &mut s,
        "brush.create_box",
        json!({ "min": [64, 0, 0], "max": [72, 8, 8], "layer": other }),
    ));

    ok(&exec(&mut s, "object.isolate", json!({ "ids": [&a] })));
    assert!(!flags(&mut s, &a).0);
    assert!(!flags(&mut s, &g).0);
    assert!(flags(&mut s, &b).0);
    assert!(flags(&mut s, &other).0);
    assert!(!flags(&mut s, &c).0);
}

#[test]
fn cameras_have_no_flags() {
    let mut s = session();
    let cam = id_of(&exec(
        &mut s,
        "view.camera.create",
        json!({ "name": "c", "position": [0, 0, 0] }),
    ));
    let r = exec(
        &mut s,
        "object.set_hidden",
        json!({ "ids": [cam], "hidden": true }),
    );
    assert_eq!(err_code(&r), "WrongKind");
    let r = exec(
        &mut s,
        "object.set_locked",
        json!({ "ids": ["b99"], "locked": true }),
    );
    assert_eq!(err_code(&r), "NotFound");
}

#[test]
fn flags_survive_save_and_reload() {
    let dir = std::env::temp_dir().join(format!("editor-flags-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("f.scene.toml");
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let layer = layer_id(&mut s);
    lock(&mut s, &[&a], true);
    ok(&exec(
        &mut s,
        "object.set_hidden",
        json!({ "ids": [&layer], "hidden": true }),
    ));
    ok(&exec(
        &mut s,
        "layer.set_omit_from_export",
        json!({ "id": &layer, "omit": true }),
    ));
    ok(&exec(
        &mut s,
        "file.save",
        json!({ "path": path.to_str().unwrap() }),
    ));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("locked = true") && text.contains("hidden = true"));

    let mut t = session();
    ok(&exec(
        &mut t,
        "file.open",
        json!({ "path": path.to_str().unwrap(), "discard": true }),
    ));
    assert_eq!(flags(&mut t, &a), (false, true));
    assert_eq!(flags(&mut t, &layer), (true, false));
    assert_eq!(full(&mut t, &layer)["omit_from_export"], true);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn hidden_objects_leave_queries_and_the_selection() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [32, 0, 0], [40, 8, 8]);
    ok(&exec(&mut s, "select.set", json!({ "ids": [&a, &b] })));
    ok(&exec(
        &mut s,
        "object.set_hidden",
        json!({ "ids": [&a], "hidden": true }),
    ));
    assert_eq!(
        ok(&exec(&mut s, "select.get", json!({})))["result"]["ids"],
        json!([&b])
    );

    let down = json!({ "origin": [4, 4, 100], "dir": [0, 0, -1] });
    let r = exec(&mut s, "scene.raycast", down.clone());
    assert!(ok(&r)["result"]["hit"].is_null());
    let mut with = down;
    with["include_hidden"] = json!(true);
    let r = exec(&mut s, "scene.raycast", with);
    assert_eq!(ok(&r)["result"]["hit"]["id"], a.as_str());

    let boxq = json!({ "min": [-1, -1, -1], "max": [50, 50, 50] });
    let r = exec(&mut s, "scene.query_box", boxq.clone());
    assert_eq!(ok(&r)["result"]["ids"], json!([&b]));
    let mut with = boxq;
    with["include_hidden"] = json!(true);
    let r = exec(&mut s, "scene.query_box", with);
    assert_eq!(ok(&r)["result"]["count"], 2);

    let r = exec(&mut s, "select.add", json!({ "ids": [&a] }));
    assert_eq!(err_code(&r), "InvalidArgument");
    ok(&exec(&mut s, "select.remove", json!({ "ids": [&a] })));
    ok(&exec(&mut s, "select.all", json!({})));
    assert_eq!(
        ok(&exec(&mut s, "select.get", json!({})))["result"]["ids"],
        json!([&b])
    );
}

#[test]
fn select_all_skips_locked_objects() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [32, 0, 0], [40, 8, 8]);
    lock(&mut s, &[&a], true);
    ok(&exec(&mut s, "select.all", json!({})));
    assert_eq!(
        ok(&exec(&mut s, "select.get", json!({})))["result"]["ids"],
        json!([&b])
    );
    ok(&exec(&mut s, "select.invert", json!({})));
    assert_eq!(
        ok(&exec(&mut s, "select.get", json!({})))["result"]["count"],
        0
    );
}

#[test]
fn summary_and_list_report_flags() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [32, 0, 0], [40, 8, 8]);
    let layer = layer_id(&mut s);
    lock(&mut s, &[&a], true);
    ok(&exec(
        &mut s,
        "object.set_hidden",
        json!({ "ids": [&b], "hidden": true }),
    ));
    let sum = summary(&mut s);
    assert_eq!(sum["hidden"], 1);
    assert_eq!(sum["locked"], 1);
    assert_eq!(sum["layers"][0]["hidden"], false);

    let listed = |s: &mut editor_cmd::Session, filter: serde_json::Value| -> Vec<String> {
        let mut args = json!({ "kind": "brush" });
        args.as_object_mut()
            .unwrap()
            .extend(filter.as_object().unwrap().clone());
        ok(&exec(s, "scene.list", args))["result"]["objects"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["id"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(listed(&mut s, json!({ "hidden": true })), vec![b.clone()]);
    assert_eq!(listed(&mut s, json!({ "locked": true })), vec![a.clone()]);
    assert_eq!(listed(&mut s, json!({ "locked": false })), vec![b.clone()]);

    ok(&exec(
        &mut s,
        "object.set_hidden",
        json!({ "ids": [&layer], "hidden": true }),
    ));
    assert_eq!(summary(&mut s)["hidden"], 3);
    assert_eq!(summary(&mut s)["layers"][0]["hidden"], true);
    let get = exec(&mut s, "scene.get", json!({ "ids": [&a] }));
    assert_eq!(ok(&get)["result"]["objects"][0]["locked"], true);
}

fn group_of(s: &mut editor_cmd::Session, ids: &[&str], name: &str) -> String {
    id_of(ok(&exec(
        s,
        "group.create",
        json!({ "objects": ids, "name": name }),
    )))
}

fn run(s: &mut editor_cmd::Session, op: &str, args: serde_json::Value) -> serde_json::Value {
    let r = exec(s, op, args);
    ok(&r);
    r
}

fn open_ids(s: &mut editor_cmd::Session) -> serde_json::Value {
    summary(s)["open_group"].clone()
}

#[test]
fn an_open_group_locks_everything_outside_it() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [32, 0, 0], [40, 8, 8]);
    let g = group_of(&mut s, &[&a], "inside");
    assert!(open_ids(&mut s).is_null());

    let r = run(&mut s, "group.open", json!({ "id": &g }));
    assert_eq!(r["result"]["open"], json!([&g]));
    assert_eq!(open_ids(&mut s), json!(&g));
    assert_eq!(full(&mut s, &g)["open"], true);

    let r = exec(
        &mut s,
        "transform.translate",
        json!({ "ids": [&b], "delta": [1, 0, 0] }),
    );
    assert_eq!(err_code(&r), "Locked", "{r}");
    assert_eq!(r["target"], b.as_str());
    ok(&exec(
        &mut s,
        "transform.translate",
        json!({ "ids": [&a], "delta": [1, 0, 0] }),
    ));

    ok(&exec(
        &mut s,
        "object.set_hidden",
        json!({ "ids": [&b], "hidden": true }),
    ));
    lock(&mut s, &[&b], true);
    lock(&mut s, &[&b], false);
    ok(&exec(
        &mut s,
        "object.set_hidden",
        json!({ "ids": [&b], "hidden": false }),
    ));

    assert_eq!(summary(&mut s)["locked"], 1);
    ok(&exec(&mut s, "select.all", json!({})));
    assert_eq!(
        ok(&exec(&mut s, "select.get", json!({})))["result"]["ids"],
        json!([&a, &g])
    );

    ok(&exec(&mut s, "group.close", json!({})));
    assert!(open_ids(&mut s).is_null());
    ok(&exec(
        &mut s,
        "transform.translate",
        json!({ "ids": [&b], "delta": [1, 0, 0] }),
    ));
}

#[test]
fn opening_and_closing_undo_and_redo() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let g = group_of(&mut s, &[&a], "g");
    ok(&exec(&mut s, "group.open", json!({ "id": &g })));
    exec(&mut s, "history.undo", json!({}));
    assert!(open_ids(&mut s).is_null());
    exec(&mut s, "history.redo", json!({}));
    assert_eq!(open_ids(&mut s), json!(&g));
    ok(&exec(&mut s, "group.close", json!({})));
    exec(&mut s, "history.undo", json!({}));
    assert_eq!(open_ids(&mut s), json!(&g));
}

#[test]
fn opening_a_nested_group_keeps_its_parents_open_and_switches_others() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let c = make_box(&mut s, [64, 0, 0], [72, 8, 8]);
    let inner = group_of(&mut s, &[&a], "inner");
    let outer = group_of(&mut s, &[&inner], "outer");
    let other = group_of(&mut s, &[&c], "other");

    let r = run(&mut s, "group.open", json!({ "id": &inner }));
    assert_eq!(r["result"]["open"], json!([&inner, &outer]));
    assert_eq!(open_ids(&mut s), json!(&inner));

    ok(&exec(&mut s, "group.close", json!({})));
    assert_eq!(open_ids(&mut s), json!(&outer));
    ok(&exec(&mut s, "group.close", json!({ "all": true })));
    assert!(open_ids(&mut s).is_null());

    ok(&exec(&mut s, "group.open", json!({ "id": &inner })));
    let r = run(&mut s, "group.open", json!({ "id": &other }));
    assert_eq!(r["result"]["open"], json!([&other]));
    let r = run(&mut s, "group.close", json!({}));
    assert_eq!(r["result"]["open"], json!([]));
    ok(&exec(&mut s, "group.close", json!({})));
}

#[test]
fn objects_created_while_a_group_is_open_join_it() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let g = group_of(&mut s, &[&a], "g");
    ok(&exec(&mut s, "group.open", json!({ "id": &g })));

    let n = make_box(&mut s, [100, 0, 0], [108, 8, 8]);
    assert_eq!(full(&mut s, &n)["group"], g.as_str());

    let r = run(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "brush.create_box", "args": { "min": [200, 0, 0], "max": [208, 8, 8] }, "as": "x" },
            { "op": "entity.create", "args": { "origin": [0, 0, 64] }, "as": "e" },
        ] }),
    );
    for i in 0..2 {
        let id = r["result"]["results"][i]["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(full(&mut s, &id)["group"], g.as_str(), "{id}");
    }
}

#[test]
fn removing_the_open_group_and_locked_groups() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let g = group_of(&mut s, &[&a], "g");
    ok(&exec(&mut s, "group.open", json!({ "id": &g })));
    ok(&exec(&mut s, "group.ungroup", json!({ "ids": [&g] })));
    assert!(open_ids(&mut s).is_null());

    let h = group_of(&mut s, &[&a], "h");
    lock(&mut s, &[&h], true);
    let r = exec(&mut s, "group.open", json!({ "id": &h }));
    assert_eq!(err_code(&r), "Locked", "{r}");
    let r = exec(&mut s, "group.open", json!({ "id": &a }));
    assert_eq!(err_code(&r), "InvalidArgument", "{r}");
    let r = exec(&mut s, "group.open", json!({ "id": "g999" }));
    assert_eq!(err_code(&r), "NotFound", "{r}");
}

#[test]
fn an_open_group_is_not_saved() {
    let dir = std::env::temp_dir().join(format!("editor-open-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("o.scene.toml");
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let g = group_of(&mut s, &[&a], "g");
    ok(&exec(&mut s, "group.open", json!({ "id": &g })));
    ok(&exec(
        &mut s,
        "file.save",
        json!({ "path": path.to_str().unwrap() }),
    ));
    assert!(!std::fs::read_to_string(&path).unwrap().contains("open"));

    let mut t = session();
    ok(&exec(
        &mut t,
        "file.open",
        json!({ "path": path.to_str().unwrap(), "discard": true }),
    ));
    assert!(open_ids(&mut t).is_null());
    std::fs::remove_dir_all(&dir).ok();
}

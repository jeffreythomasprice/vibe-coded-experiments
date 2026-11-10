mod common;

use common::*;
use level_cmd::Session;
use level_core::Document;
use serde_json::json;

fn sel_ids(v: &serde_json::Value) -> Vec<String> {
    ok(v)["result"]["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn select_set_add_remove_clear() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    assert_eq!(
        sel_ids(&exec(&mut s, "select.set", json!({ "ids": [&a] }))),
        std::slice::from_ref(&a)
    );
    assert_eq!(
        sel_ids(&exec(&mut s, "select.add", json!({ "ids": [b] }))).len(),
        2
    );
    assert_eq!(
        sel_ids(&exec(&mut s, "select.remove", json!({ "ids": [&a] }))),
        std::slice::from_ref(&b)
    );
    assert!(sel_ids(&exec(&mut s, "select.clear", json!({}))).is_empty());
    assert_eq!(
        ok(&exec(&mut s, "scene.summary", json!({})))["result"]["selection"],
        0
    );
}

#[test]
fn select_rejects_bad_ids() {
    let mut s = session();
    let layer = ok(&exec(&mut s, "scene.summary", json!({})))["result"]["layers"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        err_code(&exec(&mut s, "select.set", json!({ "ids": [layer] }))),
        "WrongKind"
    );
    assert_eq!(
        err_code(&exec(&mut s, "select.set", json!({ "ids": ["b999"] }))),
        "NotFound"
    );
}

#[test]
fn select_all_and_invert() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let _b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    assert_eq!(sel_ids(&exec(&mut s, "select.all", json!({}))).len(), 2);
    exec(&mut s, "select.set", json!({ "ids": [a] }));
    let inverted = sel_ids(&exec(&mut s, "select.invert", json!({})));
    assert_eq!(inverted.len(), 1);
    assert_ne!(inverted[0], a);
    assert!(sel_ids(&exec(&mut s, "select.all", json!({ "kind": "entity" }))).is_empty());
}

#[test]
fn selection_is_pruned_when_objects_vanish() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    exec(&mut s, "select.set", json!({ "ids": [a.clone()] }));
    exec(&mut s, "object.delete", json!({ "ids": [a.clone()] }));
    assert!(sel_ids(&exec(&mut s, "select.get", json!({}))).is_empty());
    exec(&mut s, "history.undo", json!({}));
    assert!(sel_ids(&exec(&mut s, "select.get", json!({}))).is_empty());
}

#[test]
fn selection_does_not_touch_history() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let rev = s.revision();
    exec(&mut s, "select.set", json!({ "ids": [a] }));
    assert_eq!(s.revision(), rev);
    let r = exec(&mut s, "history.list", json!({}));
    assert_eq!(ok(&r)["result"]["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn select_ops_are_rejected_in_dry_run() {
    let mut s = session();
    let r = s
        .execute_value(
            json!({ "op": "select.clear", "args": {}, "dry_run": true }),
            "test",
        )
        .to_value();
    assert_eq!(err_code(&r), "InvalidArgument");
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("level-cmd-file-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("t.level.toml")
}

#[test]
fn file_save_info_and_dirty() {
    let path = temp_path("save");
    let mut s = Session::new(Document::new("t"));
    let info = exec(&mut s, "file.info", json!({}));
    assert_eq!(ok(&info)["result"]["dirty"], false);
    assert_eq!(info["result"]["path"], serde_json::Value::Null);

    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    assert_eq!(
        exec(&mut s, "file.info", json!({}))["result"]["dirty"],
        true
    );
    assert_eq!(
        err_code(&exec(&mut s, "file.save", json!({}))),
        "InvalidArgument"
    );
    ok(&exec(
        &mut s,
        "file.save",
        json!({ "path": path.to_str().unwrap() }),
    ));
    let info = exec(&mut s, "file.info", json!({}));
    assert_eq!(info["result"]["dirty"], false);
    assert_eq!(info["result"]["path"], path.to_str().unwrap());
    assert!(path.exists());
    ok(&exec(&mut s, "file.save", json!({})));
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn file_open_and_new_guard_unsaved_changes() {
    let path = temp_path("open");
    let mut s = Session::new(Document::new("t"));
    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    ok(&exec(
        &mut s,
        "file.save",
        json!({ "path": path.to_str().unwrap() }),
    ));
    make_box(&mut s, [16, 0, 0], [24, 8, 8]);

    assert_eq!(
        err_code(&exec(&mut s, "file.new", json!({}))),
        "UnsavedChanges"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "file.open",
            json!({ "path": path.to_str().unwrap() })
        )),
        "UnsavedChanges"
    );
    let stale = s.revision();
    ok(&exec(
        &mut s,
        "file.open",
        json!({ "path": path.to_str().unwrap(), "discard": true }),
    ));
    assert!(s.revision() > stale);
    let summary = exec(&mut s, "scene.summary", json!({}));
    assert_eq!(summary["result"]["counts"]["brush"], 1);
    assert_eq!(
        exec(&mut s, "file.info", json!({}))["result"]["dirty"],
        false
    );
    assert_eq!(
        err_code(&exec(&mut s, "history.undo", json!({}))),
        "InvalidArgument"
    );

    ok(&exec(&mut s, "file.new", json!({ "name": "fresh" })));
    assert_eq!(
        exec(&mut s, "scene.summary", json!({}))["result"]["counts"]["brush"],
        0
    );
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn file_open_missing_file_is_io_error() {
    let mut s = session();
    let r = exec(
        &mut s,
        "file.open",
        json!({ "path": "/nonexistent/x.level.toml" }),
    );
    assert_eq!(err_code(&r), "Io");
}

#[test]
fn file_new_is_blocked_inside_a_transaction() {
    let mut s = session();
    exec(&mut s, "history.begin", json!({}));
    assert_eq!(
        err_code(&exec(&mut s, "file.new", json!({ "discard": true }))),
        "TransactionOpen"
    );
}

#[test]
fn file_save_as_refuses_existing_file_without_overwrite() {
    let path = temp_path("exists");
    let other = path.with_file_name("sub/dir/other.level.toml");
    let mut s = Session::new(Document::new("t"));
    let p = path.to_str().unwrap();
    ok(&exec(&mut s, "file.save", json!({ "path": p })));
    ok(&exec(&mut s, "file.save", json!({ "path": p })));

    let mut s2 = Session::new(Document::new("u"));
    let r = exec(&mut s2, "file.save", json!({ "path": p }));
    assert_eq!(err_code(&r), "FileExists");
    assert_eq!(r["field"], "path");
    ok(&exec(
        &mut s2,
        "file.save",
        json!({ "path": p, "overwrite": true }),
    ));

    ok(&exec(
        &mut s2,
        "file.save",
        json!({ "path": other.to_str().unwrap() }),
    ));
    assert!(other.exists());
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn file_new_with_path_binds_without_writing() {
    let path = temp_path("newpath");
    let target = path.with_file_name("fresh.level.toml");
    let mut s = Session::new(Document::new("t"));
    ok(&exec(
        &mut s,
        "file.new",
        json!({ "path": target.to_str().unwrap() }),
    ));
    let info = exec(&mut s, "file.info", json!({}));
    assert_eq!(info["result"]["path"], target.to_str().unwrap());
    assert_eq!(info["result"]["name"], "fresh");
    assert!(!target.exists());
    ok(&exec(&mut s, "file.save", json!({})));
    assert!(target.exists());

    let r = exec(
        &mut s,
        "file.new",
        json!({ "path": target.to_str().unwrap() }),
    );
    assert_eq!(err_code(&r), "FileExists");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn file_save_derives_name_from_path() {
    let mut s = Session::new(Document::new("untitled"));
    let dir = std::env::temp_dir().join(format!("level-save-name-{}", std::process::id()));
    let p = dir.join("hall.level.toml");
    ok(&exec(
        &mut s,
        "file.save",
        json!({ "path": p.display().to_string() }),
    ));
    assert_eq!(s.document().name, "hall");
    let _ = std::fs::remove_dir_all(dir);
}

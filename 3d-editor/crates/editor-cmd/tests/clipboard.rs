mod common;

use common::*;
use editor_cmd::Session;
use serde_json::json;

fn copy(s: &mut Session, ids: &[&str]) -> String {
    let ids: Vec<editor_core::ObjectId> = ids.iter().map(|i| i.parse().unwrap()).collect();
    editor_cmd::copy_text(s.document(), None, &ids)
        .unwrap()
        .text
}

fn paste(s: &mut Session, text: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut args = json!({ "text": text });
    args.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    exec(s, "clipboard.paste", args)
}

#[test]
fn paste_in_place_makes_fresh_objects_at_the_same_spot() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let text = copy(&mut s, &[&a]);
    let r = paste(&mut s, &text, json!({}));
    let ids = ids_of(&r);
    assert_eq!(ids.len(), 1);
    assert_ne!(ids[0], a);
    assert!(near(bounds(&mut s, &ids[0]).0, [0.0, 0.0, 0.0]));
    assert!(near(bounds(&mut s, &ids[0]).1, [8.0, 8.0, 8.0]));
}

#[test]
fn paste_at_centres_the_bounds_and_offset_shifts() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    let text = copy(&mut s, &[&a, &b]);

    let r = paste(&mut s, &text, json!({ "at": [100, 100, 100] }));
    let ids = ids_of(&r);
    let (min, _) = bounds(&mut s, &ids[0]);
    assert!(near(min, [88.0, 96.0, 96.0]), "{min:?}");

    let r = paste(&mut s, &text, json!({ "offset": [0, 0, 32] }));
    let ids = ids_of(&r);
    assert!(near(bounds(&mut s, &ids[0]).0, [0.0, 0.0, 32.0]));

    let both = paste(
        &mut s,
        &text,
        json!({ "at": [0, 0, 0], "offset": [1, 0, 0] }),
    );
    assert_eq!(err_code(&both), "InvalidArgument");
}

#[test]
fn paste_keeps_group_and_entity_contents() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    let g = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [&a, &b], "name": "pair" }),
    ));
    let e = id_of(&exec(
        &mut s,
        "entity.create",
        json!({ "origin": [0, 0, 64], "components": { "lamp": { "target": &a } } }),
    ));
    let text = copy(&mut s, &[&g, &e]);
    let r = paste(&mut s, &text, json!({ "offset": [0, 64, 0] }));
    let ids = ids_of(&r);
    assert_eq!(ids.len(), 4, "{r}");
    let map = &r["result"]["map"];
    let new_group = map[&g].as_str().unwrap();
    let new_a = map[&a].as_str().unwrap();
    assert_eq!(full(&mut s, new_a)["group"], new_group);
    assert_eq!(full(&mut s, new_group)["name"], "pair");
    assert_ne!(new_group, g);
}

#[test]
fn copying_a_group_member_alone_pastes_it_ungrouped() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [&a, &b] }),
    ));
    let text = copy(&mut s, &[&a]);
    let r = paste(&mut s, &text, json!({ "offset": [0, 32, 0] }));
    let new = ids_of(&r)[0].clone();
    assert!(full(&mut s, &new)["group"].is_null());
}

#[test]
fn materials_and_layers_are_matched_by_name() {
    let mut src = session();
    let a = make_box(&mut src, [0, 0, 0], [8, 8, 8]);
    ok(&exec(
        &mut src,
        "material.create",
        json!({ "name": "brick", "color": [1.0, 0.0, 0.0, 1.0] }),
    ));
    ok(&exec(
        &mut src,
        "face.set_material",
        json!({ "faces": [&a], "material": "brick" }),
    ));
    let layer = id_of(&exec(&mut src, "layer.create", json!({ "name": "props" })));
    ok(&exec(
        &mut src,
        "object.set_layer",
        json!({ "ids": [&a], "layer": layer }),
    ));
    let text = copy(&mut src, &[&a]);

    let mut dst = session();
    let r = paste(&mut dst, &text, json!({}));
    ok(&r);
    let list = exec(&mut dst, "material.list", json!({}));
    assert!(
        list["result"]["materials"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["name"] == "brick")
    );
    let new = ids_of(&r)[0].clone();
    assert_eq!(full(&mut dst, &new)["layer"], layer_id(&mut dst));

    exec(&mut dst, "layer.create", json!({ "name": "props" }));
    let again = paste(&mut dst, &text, json!({}));
    assert!(again["warnings"].as_array().is_none_or(|w| w.is_empty()));

    ok(&exec(
        &mut dst,
        "material.set",
        json!({ "material": "brick", "color": [0.0, 1.0, 0.0, 1.0] }),
    ));
    let differing = paste(&mut dst, &text, json!({}));
    assert!(
        differing["warnings"][0].as_str().unwrap().contains("brick"),
        "{differing}"
    );
}

#[test]
fn bad_text_and_empty_clipboard_are_argument_errors() {
    let mut s = session();
    let r = exec(&mut s, "clipboard.paste", json!({}));
    assert_eq!(err_code(&r), "InvalidArgument");
    assert_eq!(r["field"], "text");
    let r = paste(&mut s, "this is not toml [", json!({}));
    assert_eq!(err_code(&r), "InvalidArgument");
    assert_eq!(r["field"], "text");
}

#[test]
fn a_whole_scene_file_can_be_pasted() {
    let mut src = session();
    make_box(&mut src, [0, 0, 0], [8, 8, 8]);
    make_box(&mut src, [32, 0, 0], [40, 8, 8]);
    let text = editor_fmt::write(src.document());
    let mut dst = session();
    let r = paste(&mut dst, &text, json!({}));
    assert_eq!(ids_of(&r).len(), 2);
}

#[test]
fn paste_is_one_undo_step_and_dry_run_changes_nothing() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let text = copy(&mut s, &[&a]);
    let before = summary(&mut s);
    let dry = s
        .execute_value(
            json!({ "op": "clipboard.paste", "args": { "text": &text }, "dry_run": true }),
            "test",
        )
        .to_value();
    ok(&dry);
    assert_eq!(summary(&mut s), before);
    paste(&mut s, &text, json!({}));
    assert_ne!(summary(&mut s), before);
    ok(&exec(&mut s, "history.undo", json!({})));
    assert_eq!(summary(&mut s), before);
}

#[test]
fn paste_joins_an_open_group_and_respects_locks() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    let g = id_of(&exec(&mut s, "group.create", json!({ "objects": [&b] })));
    let text = copy(&mut s, &[&a]);
    ok(&exec(&mut s, "group.open", json!({ "id": &g })));
    let r = paste(&mut s, &text, json!({ "offset": [0, 32, 0] }));
    let new = ids_of(&r)[0].clone();
    assert_eq!(full(&mut s, &new)["group"], g);

    ok(&exec(&mut s, "group.close", json!({ "all": true })));
    let layer = layer_id(&mut s);
    ok(&exec(
        &mut s,
        "object.set_locked",
        json!({ "ids": [layer], "locked": true }),
    ));
    let r = paste(&mut s, &text, json!({}));
    assert_eq!(err_code(&r), "Locked", "{r}");
}

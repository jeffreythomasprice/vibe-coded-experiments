mod common;

use common::*;
use editor_cmd::Session;
use serde_json::{Value, json};

fn defs() -> Value {
    json!({ "version": 2, "components": [
        { "name": "button", "schema": { "type": "object", "properties": {
            "targets": { "type": "array", "items": { "$ref": "#/$defs/link" } },
            "event": { "type": "string", "default": "press" }
        }, "$defs": { "link": { "type": "string", "editor": { "linkType": "entityID" } } } } },
        { "name": "light", "color": [1, 0.5, 0], "schema": {
            "type": "object",
            "properties": {
                "intensity": { "type": "number", "default": 300 },
                "link": { "type": "string", "editor": { "linkType": "entityID" } },
                "zones": { "type": "array", "items": { "type": "object", "properties": {
                    "target": { "type": "string", "editor": { "linkType": "entityID" } }
                } } }
            },
            "additionalProperties": false
        } }
    ] })
}

fn with_defs() -> Session {
    let mut s = session();
    ok(&exec(
        &mut s,
        "component_defs.add",
        json!({ "source": "embedded", "defs": defs() }),
    ));
    s
}

fn light(s: &mut Session, data: Value) -> String {
    id_of(&exec(
        s,
        "entity.create",
        json!({ "origin": [0, 0, 0], "components": { "light": data } }),
    ))
}

fn codes(s: &mut Session) -> Vec<String> {
    let r = exec(s, "check.validate", json!({}));
    ok(&r)["result"]["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["code"].as_str().unwrap().to_string())
        .collect()
}

fn warnings(r: &Value) -> Vec<String> {
    ok(r)["warnings"]
        .as_array()
        .map(|w| w.iter().map(|x| x.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

#[test]
fn schema_mismatch_warns_but_stores() {
    let mut s = with_defs();
    let r = exec(
        &mut s,
        "entity.create",
        json!({ "origin": [0, 0, 0], "components": { "light": { "intensity": "bright" } } }),
    );
    let id = id_of(&r);
    assert!(warnings(&r).iter().any(|w| w.contains("/intensity")), "{r}");
    let got = full(&mut s, &id);
    assert_eq!(got["components"]["light"]["intensity"], "bright");
    assert!(codes(&mut s).contains(&"schema_violation".to_string()));
}

#[test]
fn unknown_component_only_warns_when_defs_exist() {
    let mut s = session();
    let r = exec(
        &mut s,
        "entity.create",
        json!({ "origin": [0, 0, 0], "components": { "mystery": {} } }),
    );
    assert!(warnings(&r).is_empty());

    let mut s = with_defs();
    let r = exec(
        &mut s,
        "entity.create",
        json!({ "origin": [0, 0, 0], "components": { "mystery": {} } }),
    );
    assert!(warnings(&r).iter().any(|w| w.contains("no definition")));
    assert!(codes(&mut s).contains(&"unknown_component".to_string()));
}

#[test]
fn builtin_components_validate_without_any_defs() {
    let mut s = session();
    let id = id_of(&exec(
        &mut s,
        "entity.create",
        json!({ "origin": [0, 0, 0] }),
    ));
    let r = exec(
        &mut s,
        "entity.add_component",
        json!({ "ids": [id], "name": "keyframe_animation", "data": { "keyframes": [] } }),
    );
    assert!(
        warnings(&r)
            .iter()
            .any(|w| w.contains("keyframe_animation")),
        "{r}"
    );
    assert!(codes(&mut s).contains(&"schema_violation".to_string()));
    let r = exec(
        &mut s,
        "entity.set_component",
        json!({ "ids": [id], "name": "keyframe_animation", "data": { "keyframes": [{ "time": 0 }, { "time": 2, "offset": [0, 0, 8] }] } }),
    );
    assert!(warnings(&r).is_empty(), "{r}");
}

#[test]
fn add_component_takes_schema_defaults() {
    let mut s = with_defs();
    let id = id_of(&exec(
        &mut s,
        "entity.create",
        json!({ "origin": [0, 0, 0] }),
    ));
    ok(&exec(
        &mut s,
        "entity.add_component",
        json!({ "ids": [id], "name": "light" }),
    ));
    assert_eq!(
        full(&mut s, &id)["components"]["light"],
        json!({ "intensity": 300 })
    );
    ok(&exec(
        &mut s,
        "entity.add_component",
        json!({ "ids": [id], "name": "button", "data": {} }),
    ));
    assert_eq!(full(&mut s, &id)["components"]["button"], json!({}));
}

#[test]
fn null_is_rejected_with_its_pointer() {
    let mut s = with_defs();
    let r = exec(
        &mut s,
        "entity.create",
        json!({ "origin": [0, 0, 0], "components": { "light": { "zones": [{ "target": null }] } } }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    assert!(r["message"].as_str().unwrap().contains("zones/0/target"));
}

#[test]
fn links_resolve_through_refs_arrays_and_nesting() {
    let mut s = with_defs();
    let target = light(&mut s, json!({}));
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let button = id_of(&exec(
        &mut s,
        "entity.create_brush_entity",
        json!({ "objects": [b],
                "components": { "button": { "targets": [target, "e9999", "b1", "nope"] } } }),
    ));
    let a = light(
        &mut s,
        json!({ "link": target, "zones": [{ "target": button }] }),
    );

    let r = exec(&mut s, "entity.links", json!({ "ids": [button] }));
    let links = ok(&r)["result"]["links"].as_array().unwrap().clone();
    let status: Vec<(&str, &str)> = links
        .iter()
        .map(|l| {
            (
                l["pointer"].as_str().unwrap(),
                l["status"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        status,
        vec![
            ("/targets/0", "ok"),
            ("/targets/1", "missing"),
            ("/targets/2", "not_entity"),
            ("/targets/3", "malformed"),
        ]
    );

    let r = exec(&mut s, "entity.links", json!({ "ids": [a] }));
    let links = ok(&r)["result"]["links"].as_array().unwrap().clone();
    assert_eq!(links.len(), 2);
    assert_eq!(links[1]["component"], "light");
    assert_eq!(links[1]["pointer"], "/zones/0/target");
    assert_eq!(links[1]["to"], button);

    let c = codes(&mut s);
    for want in ["dangling_link", "link_not_entity"] {
        assert!(c.contains(&want.to_string()), "{want} in {c:?}");
    }
}

#[test]
fn self_link_and_dangling_after_delete() {
    let mut s = with_defs();
    let a = light(&mut s, json!({}));
    exec(
        &mut s,
        "entity.update_component",
        json!({ "ids": [a], "name": "light", "set": { "link": a } }),
    );
    assert!(codes(&mut s).contains(&"self_link".to_string()));

    let b = light(&mut s, json!({ "link": a }));
    let r = exec(&mut s, "object.delete", json!({ "ids": [a] }));
    assert!(
        warnings(&r).iter().any(|w| w.contains(&b)),
        "deleting a linked target warns: {r}"
    );
    assert!(codes(&mut s).contains(&"dangling_link".to_string()));
}

#[test]
fn duplicate_remaps_links_inside_the_copied_set_only() {
    let mut s = with_defs();
    let b = light(&mut s, json!({}));
    let a = light(&mut s, json!({ "link": b }));

    let r = exec(&mut s, "object.duplicate", json!({ "ids": [a, b] }));
    let map = ok(&r)["result"]["map"].clone();
    let (a2, b2) = (map[&a].as_str().unwrap(), map[&b].as_str().unwrap());
    let got = full(&mut s, a2);
    assert_eq!(got["components"]["light"]["link"], b2);

    let r = exec(&mut s, "object.duplicate", json!({ "ids": [a] }));
    let a3 = ok(&r)["result"]["ids"][0].as_str().unwrap().to_string();
    let got = full(&mut s, &a3);
    assert_eq!(got["components"]["light"]["link"], b);
}

#[test]
fn set_component_swaps_the_whole_object() {
    let mut s = with_defs();
    let a = light(&mut s, json!({ "intensity": 5, "link": "e1" }));
    ok(&exec(
        &mut s,
        "entity.set_component",
        json!({ "ids": [a], "name": "light", "data": { "intensity": 9 } }),
    ));
    let got = full(&mut s, &a);
    assert_eq!(got["components"]["light"], json!({ "intensity": 9 }));
}

#[test]
fn file_defs_resolve_relative_to_the_scene_and_report_failures() {
    let dir = std::env::temp_dir().join(format!("editor-defs-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("defs")).unwrap();
    let file = dir.join("defs/game.components.json");
    std::fs::write(&file, defs().to_string()).unwrap();

    let mut s = session();
    let scene = dir.join("a.scene.toml");
    ok(&exec(
        &mut s,
        "file.save",
        json!({ "path": scene.display().to_string() }),
    ));
    let d = id_of(&exec(
        &mut s,
        "component_defs.add",
        json!({ "source": "file", "path": file.display().to_string() }),
    ));
    let list = exec(&mut s, "component_defs.list", json!({}));
    assert_eq!(
        ok(&list)["result"]["defs"][0]["path"],
        "defs/game.components.json"
    );
    assert_eq!(
        ok(&list)["result"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| !c["from"].is_null())
            .count(),
        2
    );
    assert!(
        ok(&list)["result"]["problems"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    std::fs::write(&file, "{ not json").unwrap();
    assert!(codes(&mut s).contains(&"defs_load_failed".to_string()));
    std::fs::remove_file(&file).unwrap();
    assert!(codes(&mut s).contains(&"defs_load_failed".to_string()));

    std::fs::write(&file, defs().to_string()).unwrap();
    assert!(!codes(&mut s).contains(&"defs_load_failed".to_string()));
    let r = exec(&mut s, "component_defs.embed", json!({ "ids": [d] }));
    assert_eq!(ok(&r)["result"]["updated"], 1);
    std::fs::remove_file(&file).unwrap();
    assert!(!codes(&mut s).contains(&"defs_load_failed".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn later_defs_override_and_bad_schemas_are_reported() {
    let mut s = with_defs();
    ok(&exec(
        &mut s,
        "component_defs.add",
        json!({ "source": "embedded", "defs": { "version": 2, "components": [
            { "name": "light", "schema": { "type": 12 } }
        ] } }),
    ));
    let c = codes(&mut s);
    assert!(c.contains(&"duplicate_component".to_string()), "{c:?}");
    assert!(c.contains(&"defs_bad_schema".to_string()), "{c:?}");
    let list = exec(&mut s, "component_defs.list", json!({}));
    let light = ok(&list)["result"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["def"]["name"] == "light")
        .unwrap()
        .clone();
    assert_eq!(light["def"]["schema"], json!({ "type": 12 }));
}

#[test]
fn builtins_cannot_be_overridden() {
    let mut s = session();
    ok(&exec(
        &mut s,
        "component_defs.add",
        json!({ "source": "embedded", "defs": { "version": 2, "components": [
            { "name": "player_start", "schema": { "type": "string" } }
        ] } }),
    ));
    assert!(codes(&mut s).contains(&"duplicate_component".to_string()));
    let list = exec(&mut s, "component_defs.list", json!({}));
    let found = ok(&list)["result"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["def"]["name"] == "player_start")
        .unwrap()
        .clone();
    assert!(found["from"].is_null());
}

#[test]
fn brush_entity_pivot_and_set_transform_carry_or_keep_members() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [16, 16, 16]);
    let e = id_of(&exec(
        &mut s,
        "entity.create_brush_entity",
        json!({ "objects": [b], "origin": [0, 0, 0] }),
    ));
    ok(&exec(
        &mut s,
        "entity.set_transform",
        json!({ "id": e, "origin": [32, 0, 0] }),
    ));
    let bounds = exec(&mut s, "scene.bounds", json!({ "ids": [b] }));
    assert_eq!(
        ok(&bounds)["result"]["combined"]["min"],
        json!([32.0, 0.0, 0.0])
    );

    ok(&exec(
        &mut s,
        "entity.set_transform",
        json!({ "id": e, "origin": [0, 0, 0], "members": "keep" }),
    ));
    let bounds = exec(&mut s, "scene.bounds", json!({ "ids": [b] }));
    assert_eq!(
        ok(&bounds)["result"]["combined"]["min"],
        json!([32.0, 0.0, 0.0])
    );
    assert_eq!(full(&mut s, &e)["origin"], json!([0.0, 0.0, 0.0]));

    ok(&exec(
        &mut s,
        "entity.set_transform",
        json!({ "id": e, "angles": [0, 90, 0] }),
    ));
    let bounds = exec(&mut s, "scene.bounds", json!({ "ids": [b] }));
    assert_eq!(
        ok(&bounds)["result"]["combined"]["min"],
        json!([-16.0, 32.0, 0.0])
    );
}

#[test]
fn relative_defs_need_a_saved_scene() {
    let mut s = session();
    let r = exec(
        &mut s,
        "component_defs.add",
        json!({ "source": "file", "path": "Cargo.toml", "store": "relative" }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
}

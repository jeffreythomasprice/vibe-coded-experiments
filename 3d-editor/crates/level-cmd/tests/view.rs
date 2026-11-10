mod common;

use common::*;
use serde_json::json;

fn create_persp(s: &mut level_cmd::Session, name: &str) -> String {
    id_of(&exec(
        s,
        "view.camera.create",
        json!({ "name": name, "position": [0, -100, 50], "target": [0, 0, 0] }),
    ))
}

#[test]
fn create_get_set_list_delete() {
    let mut s = session();
    let id = create_persp(&mut s, "main");

    let v = exec(&mut s, "view.camera.get", json!({ "camera": "main" }));
    let cam = &ok(&v)["result"]["camera"];
    assert_eq!(cam["id"], id.as_str());
    assert_eq!(cam["yaw"], 90.0);
    assert!(cam["pitch"].as_f64().unwrap() < 0.0);
    assert_eq!(cam["projection"]["perspective"]["fov"], 60.0);

    let v = exec(
        &mut s,
        "view.camera.set",
        json!({ "camera": id, "fov": 90, "name": "wide" }),
    );
    ok(&v);
    let v = exec(&mut s, "view.camera.get", json!({ "camera": "wide" }));
    assert_eq!(
        ok(&v)["result"]["camera"]["projection"]["perspective"]["fov"],
        90.0
    );

    let v = exec(&mut s, "view.camera.list", json!({}));
    let r = &ok(&v)["result"];
    assert_eq!(r["cameras"].as_array().unwrap().len(), 1);
    assert_eq!(r["viewports"].as_array().unwrap().len(), 4);

    ok(&exec(
        &mut s,
        "view.camera.delete",
        json!({ "cameras": ["wide"] }),
    ));
    let v = exec(&mut s, "view.camera.get", json!({ "camera": "wide" }));
    assert_eq!(err_code(&v), "NotFound");
}

#[test]
fn camera_edits_undo() {
    let mut s = session();
    let id = create_persp(&mut s, "main");
    ok(&exec(
        &mut s,
        "view.camera.set",
        json!({ "camera": id, "yaw": 10 }),
    ));
    ok(&exec(&mut s, "history.undo", json!({})));
    let v = exec(&mut s, "view.camera.get", json!({ "camera": id }));
    assert_eq!(ok(&v)["result"]["camera"]["yaw"], 90.0);
    ok(&exec(&mut s, "history.undo", json!({})));
    let v = exec(&mut s, "view.camera.get", json!({ "camera": id }));
    assert_eq!(err_code(&v), "NotFound");
}

#[test]
fn ortho_defaults_are_framed_on_the_scene() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [200, 100, 50]);
    let v = exec(
        &mut s,
        "view.camera.create",
        json!({ "name": "t", "ortho": "top" }),
    );
    let id = id_of(&v);
    let v = exec(&mut s, "view.camera.get", json!({ "camera": id }));
    let cam = &ok(&v)["result"]["camera"];
    assert_eq!(cam["position"], json!([100.0, 50.0, 25.0]));
    let zoom = cam["projection"]["orthographic"]["zoom"].as_f64().unwrap();
    assert!((zoom - 1024.0 / (200.0 * 1.1)).abs() < 1e-9, "{zoom}");
}

#[test]
fn viewport_cameras_resolve_and_are_read_only() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64]);
    for name in [
        "viewport.persp",
        "viewport.top",
        "viewport.front",
        "viewport.side",
    ] {
        let v = exec(&mut s, "view.camera.get", json!({ "camera": name }));
        let cam = &ok(&v)["result"]["camera"];
        assert_eq!(cam["session"], true, "{name}");
        assert_eq!(cam["name"], name);
    }
    let v = exec(
        &mut s,
        "view.camera.get",
        json!({ "camera": "viewport.nope" }),
    );
    assert_eq!(err_code(&v), "InvalidArgument");
    let v = exec(
        &mut s,
        "view.camera.set",
        json!({ "camera": "viewport.top", "zoom": 2 }),
    );
    assert_eq!(err_code(&v), "InvalidArgument");
    let v = exec(
        &mut s,
        "view.frame",
        json!({ "ids": ["b2"], "camera": "viewport.top" }),
    );
    assert_eq!(err_code(&v), "InvalidArgument");
}

#[test]
fn validation_errors() {
    let mut s = session();
    create_persp(&mut s, "main");
    let cases = [
        (
            "view.camera.create",
            json!({ "name": "main", "position": [0, 0, 0] }),
        ),
        (
            "view.camera.create",
            json!({ "name": "viewport.x", "position": [0, 0, 0] }),
        ),
        (
            "view.camera.create",
            json!({ "name": "b7", "position": [0, 0, 0] }),
        ),
        (
            "view.camera.create",
            json!({ "name": "", "position": [0, 0, 0] }),
        ),
        ("view.camera.create", json!({ "name": "a" })),
        (
            "view.camera.create",
            json!({ "name": "a", "position": [0, 0, 0], "fov": 0 }),
        ),
        (
            "view.camera.create",
            json!({ "name": "a", "position": [0, 0, 0], "fov": 200 }),
        ),
        (
            "view.camera.create",
            json!({ "name": "a", "position": [0, 0, 0], "zoom": 2 }),
        ),
        (
            "view.camera.create",
            json!({ "name": "a", "ortho": "top", "zoom": 0 }),
        ),
        (
            "view.camera.create",
            json!({ "name": "a", "ortho": "top", "fov": 50 }),
        ),
        (
            "view.camera.create",
            json!({ "name": "a", "position": [0, 0, 0], "target": [0, 0, 0] }),
        ),
        (
            "view.camera.create",
            json!({ "name": "a", "position": [0, 0, 0], "target": [1, 0, 0], "yaw": 3 }),
        ),
        (
            "view.camera.create",
            json!({ "name": "a", "position": [0, 0, 0], "pitch": 95 }),
        ),
        (
            "view.camera.create",
            json!({ "name": "a", "position": [0, 0, 0], "bogus": 1 }),
        ),
    ];
    for (op, args) in cases {
        let v = exec(&mut s, op, args.clone());
        assert_eq!(err_code(&v), "InvalidArgument", "{args}: {v}");
    }
    let v = exec(&mut s, "view.camera.delete", json!({ "cameras": ["nope"] }));
    assert_eq!(err_code(&v), "NotFound");
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let v = exec(&mut s, "view.camera.delete", json!({ "cameras": [b] }));
    assert_eq!(err_code(&v), "WrongKind");
}

#[test]
fn switching_projection_with_set() {
    let mut s = session();
    let id = create_persp(&mut s, "main");
    ok(&exec(
        &mut s,
        "view.camera.set",
        json!({ "camera": id, "ortho": "front", "zoom": 3 }),
    ));
    let v = exec(&mut s, "view.camera.get", json!({ "camera": id }));
    assert_eq!(
        ok(&v)["result"]["camera"]["projection"]["orthographic"]["view"],
        "front"
    );
    ok(&exec(
        &mut s,
        "view.camera.set",
        json!({ "camera": id, "fov": 70 }),
    ));
    let v = exec(&mut s, "view.camera.get", json!({ "camera": id }));
    assert_eq!(
        ok(&v)["result"]["camera"]["projection"]["perspective"]["fov"],
        70.0
    );
}

#[test]
fn frame_moves_a_document_camera() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [64, 64, 64]);
    let id = create_persp(&mut s, "main");
    let v = exec(
        &mut s,
        "view.frame",
        json!({ "ids": [b], "camera": "main" }),
    );
    let moved = ok(&v)["result"]["camera"]["position"].clone();
    assert_ne!(moved, json!([0.0, -100.0, 50.0]));
    assert_eq!(v["changes"]["modified"], json!([id]));

    let empty = id_of(&exec(&mut s, "layer.create", json!({ "name": "empty" })));
    let v = exec(
        &mut s,
        "view.frame",
        json!({ "ids": [empty], "camera": "main" }),
    );
    assert_eq!(err_code(&v), "InvalidArgument");
    let v = exec(
        &mut s,
        "view.frame",
        json!({ "ids": [b], "camera": "main", "width": 0 }),
    );
    assert_eq!(err_code(&v), "InvalidArgument");
}

#[test]
fn screenshot_is_client_side_only() {
    let mut s = session();
    let v = exec(&mut s, "view.screenshot", json!({}));
    assert_eq!(err_code(&v), "InvalidArgument");
    assert!(v["hint"].as_str().unwrap().contains("screenshot"));

    let v = exec(
        &mut s,
        "batch",
        json!({ "commands": [{ "op": "view.screenshot", "args": {} }] }),
    );
    assert_eq!(err_code(&v), "InvalidArgument");
    assert_eq!(v["index"], 0);

    let info = level_cmd::OPS
        .iter()
        .find(|o| o.name == "view.screenshot")
        .unwrap();
    assert_eq!(info.exec, level_cmd::Exec::Client);
    assert_eq!(
        level_cmd::OPS
            .iter()
            .filter(|o| o.exec == level_cmd::Exec::Client)
            .count(),
        1
    );
}

#[test]
fn frame_fits_groups() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let b = make_box(&mut s, [4000, 0, 0], [4008, 8, 8]);
    let g = id_of(&exec(&mut s, "group.create", json!({ "objects": [b] })));
    create_persp(&mut s, "main");
    let v = exec(
        &mut s,
        "view.frame",
        json!({ "ids": [g], "camera": "main" }),
    );
    let near_b = ok(&v)["result"]["camera"]["position"][0].as_f64().unwrap();
    let v = exec(
        &mut s,
        "view.frame",
        json!({ "ids": [g, a], "camera": "main" }),
    );
    let both = ok(&v)["result"]["camera"]["position"][0].as_f64().unwrap();
    assert!(near_b > 3000.0, "{near_b}");
    assert!(both < near_b, "{both} {near_b}");
}

mod common;

use common::*;
use serde_json::{Value, json};

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("editor-materials-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn brick_png(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("tex").join("brick.png");
    write_png(&path, 4, 2, |x, _| {
        if x < 2 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        }
    });
    path
}

fn faces(s: &mut Session, target: &str) -> Vec<Value> {
    let r = exec(s, "face.get_uv", json!({ "faces": [target] }));
    ok(&r)["result"]["faces"].as_array().unwrap().clone()
}

use editor_cmd::Session;

fn top_of(s: &mut Session, brush: &str) -> String {
    faces(s, brush)
        .into_iter()
        .find(|f| f["normal"][2].as_f64().unwrap() > 0.99)
        .map(|f| f["face"].as_str().unwrap().to_string())
        .unwrap()
}

fn side_of(s: &mut Session, brush: &str) -> String {
    faces(s, brush)
        .into_iter()
        .find(|f| f["normal"][0].as_f64().unwrap() > 0.99)
        .map(|f| f["face"].as_str().unwrap().to_string())
        .unwrap()
}

fn texels(face: &Value) -> Vec<[i64; 2]> {
    let mut t: Vec<[i64; 2]> = face["corners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let t = &c["texel"];
            [
                (t[0].as_f64().unwrap() * 1000.0).round() as i64,
                (t[1].as_f64().unwrap() * 1000.0).round() as i64,
            ]
        })
        .collect();
    t.sort();
    t
}

fn with_brick(s: &mut Session, dir: &std::path::Path) {
    let png = brick_png(dir);
    ok(&exec(
        s,
        "material.create",
        json!({ "name": "brick", "texture": { "source": "file", "path": png.display().to_string(), "store": "absolute" } }),
    ));
}

#[test]
fn fit_covers_the_image_once() {
    let dir = temp("fit");
    let mut s = session();
    with_brick(&mut s, &dir);
    let b = make_box(&mut s, [0, 0, 0], [128, 64, 32]);
    let top = top_of(&mut s, &b);
    ok(&exec(
        &mut s,
        "face.set_material",
        json!({ "faces": [top], "material": "brick" }),
    ));
    ok(&exec(
        &mut s,
        "face.align_uv",
        json!({ "faces": [top], "action": { "type": "fit" } }),
    ));
    let f = &faces(&mut s, &top)[0];
    assert_eq!(f["size"], json!([4.0, 2.0]));
    let (lo, hi) = (&f["texel_min"], &f["texel_max"]);
    assert!(lo[0].as_f64().unwrap().abs() < 1e-6 && lo[1].as_f64().unwrap().abs() < 1e-6);
    assert!(
        (hi[0].as_f64().unwrap() - 4.0).abs() < 1e-6
            && (hi[1].as_f64().unwrap() - 2.0).abs() < 1e-6
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn group_fit_runs_the_image_across_faces() {
    let dir = temp("group");
    let mut s = session();
    with_brick(&mut s, &dir);
    let a = make_box(&mut s, [0, 0, 0], [64, 64, 32]);
    let b = make_box(&mut s, [64, 0, 0], [128, 64, 32]);
    let (ta, tb) = (top_of(&mut s, &a), top_of(&mut s, &b));
    ok(&exec(
        &mut s,
        "face.set_material",
        json!({ "faces": [ta, tb], "material": "brick" }),
    ));
    ok(&exec(
        &mut s,
        "face.align_uv",
        json!({ "faces": [ta, tb], "action": { "type": "fit" }, "as_group": true }),
    ));
    let (fa, fb) = (faces(&mut s, &ta).remove(0), faces(&mut s, &tb).remove(0));
    let range = |f: &Value, axis: usize| {
        (
            f["texel_min"][axis].as_f64().unwrap(),
            f["texel_max"][axis].as_f64().unwrap(),
        )
    };
    let (a0, a1) = range(&fa, 0);
    let (b0, b1) = range(&fb, 0);
    assert!(a0.abs() < 1e-6 && (a1 - 2.0).abs() < 1e-6, "{a0} {a1}");
    assert!(
        (b0 - 2.0).abs() < 1e-6 && (b1 - 4.0).abs() < 1e-6,
        "{b0} {b1}"
    );
    let (v0, v1) = range(&fb, 1);
    assert!(v0.abs() < 1e-6 && (v1 - 2.0).abs() < 1e-6);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rotating_keeps_the_texture_on_the_geometry() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [128, 64, 32]);
    let top = top_of(&mut s, &b);
    ok(&exec(
        &mut s,
        "face.set_material",
        json!({ "faces": [top], "material": "m" }),
    ));
    ok(&exec(
        &mut s,
        "face.set_uv",
        json!({ "faces": [top], "offset": [5, 3], "scale": [2, 4], "rotation": 20 }),
    ));
    let before = texels(&faces(&mut s, &top)[0]);
    ok(&exec(
        &mut s,
        "transform.rotate",
        json!({ "ids": [b], "axis": "z", "degrees": 90 }),
    ));
    let after = texels(&faces(&mut s, &top)[0]);
    assert_eq!(before, after);
    ok(&exec(
        &mut s,
        "transform.rotate",
        json!({ "ids": [b], "axis": "z", "degrees": 90, "texture_lock": false }),
    ));
    let unlocked = texels(&faces(&mut s, &top)[0]);
    assert_ne!(before, unlocked);
}

#[test]
fn moving_a_vertex_keeps_the_other_corners_texels() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [64, 64, 64]);
    let top = top_of(&mut s, &b);
    ok(&exec(
        &mut s,
        "face.set_uv",
        json!({ "faces": [top], "offset": [7, 9], "scale": [2, 2] }),
    ));
    let before = faces(&mut s, &top).remove(0);
    ok(&exec(
        &mut s,
        "vertex.translate",
        json!({ "brush_vertices": [{ "brush": b, "at": [64, 64, 64] }], "delta": [32, 0, 0] }),
    ));
    let after = faces(&mut s, &b)
        .into_iter()
        .find(|f| f["face"] == before["face"])
        .unwrap();
    let find = |f: &Value, p: [f64; 3]| {
        f["corners"].as_array().unwrap().iter().find_map(|c| {
            let q: Vec<f64> = c["position"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect();
            ((q[0] - p[0]).abs() < 1e-6 && (q[1] - p[1]).abs() < 1e-6 && (q[2] - p[2]).abs() < 1e-6)
                .then(|| c["texel"].clone())
        })
    };
    for (from, to) in [
        ([0.0, 0.0, 64.0], [0.0, 0.0, 64.0]),
        ([64.0, 0.0, 64.0], [64.0, 0.0, 64.0]),
        ([0.0, 64.0, 64.0], [0.0, 64.0, 64.0]),
    ] {
        let (was, now) = (find(&before, from).unwrap(), find(&after, to).unwrap());
        for axis in 0..2 {
            assert!(
                (was[axis].as_f64().unwrap() - now[axis].as_f64().unwrap()).abs() < 1e-5,
                "{from:?}: {was} vs {now}"
            );
        }
    }
    let moved = find(&after, [96.0, 64.0, 64.0]).unwrap();
    let was = find(&before, [64.0, 64.0, 64.0]).unwrap();
    assert!((moved[0].as_f64().unwrap() - (was[0].as_f64().unwrap() + 16.0)).abs() < 1e-5);
}

#[test]
fn relative_textures_follow_the_scene_file() {
    let dir = temp("relative");
    let png = brick_png(&dir);
    let mut s = session();
    let args = json!({ "name": "brick", "texture": { "source": "file", "path": png.display().to_string(), "store": "relative" } });
    assert_eq!(
        err_code(&exec(&mut s, "material.create", args.clone())),
        "InvalidArgument"
    );

    let first = dir.join("levels").join("a.scene.toml");
    ok(&exec(
        &mut s,
        "file.save",
        json!({ "path": first.display().to_string() }),
    ));
    ok(&exec(&mut s, "material.create", args));
    let m = ok(&exec(
        &mut s,
        "material.get",
        json!({ "material": "brick" }),
    ))
    .clone();
    assert_eq!(m["result"]["texture"]["path"], "../tex/brick.png");
    assert_eq!(m["result"]["texture"]["width"], 4);
    assert!(m["result"]["texture"].get("error").is_none());

    let second = dir.join("other").join("deep").join("b.scene.toml");
    ok(&exec(
        &mut s,
        "file.save",
        json!({ "path": second.display().to_string() }),
    ));
    let m = ok(&exec(
        &mut s,
        "material.get",
        json!({ "material": "brick" }),
    ))
    .clone();
    assert_eq!(m["result"]["texture"]["path"], "../../tex/brick.png");
    assert_eq!(m["result"]["texture"]["width"], 4);

    let saved = std::fs::read_to_string(&second).unwrap();
    assert!(
        saved.contains("texture = { path = \"../../tex/brick.png\" }"),
        "{saved}"
    );

    std::fs::remove_file(&png).unwrap();
    let report = exec(&mut s, "check.validate", json!({}));
    let issues = ok(&report)["result"]["issues"].as_array().unwrap().clone();
    assert!(
        issues.iter().any(|i| i["code"] == "texture_unreadable"),
        "{issues:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn embed_stores_the_bytes_in_the_scene() {
    let dir = temp("embed");
    let mut s = session();
    with_brick(&mut s, &dir);
    ok(&exec(&mut s, "material.embed", json!({})));
    let path = dir.join("a.scene.toml");
    ok(&exec(
        &mut s,
        "file.save",
        json!({ "path": path.display().to_string() }),
    ));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("base64 = \"iVBOR"), "{text}");
    std::fs::remove_dir_all(dir.join("tex")).unwrap();
    let m = ok(&exec(
        &mut s,
        "material.get",
        json!({ "material": "brick" }),
    ))
    .clone();
    assert_eq!(m["result"]["texture"]["kind"], "embedded");
    assert_eq!(m["result"]["texture"]["height"], 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rename_and_delete_follow_the_faces() {
    let mut s = session();
    ok(&exec(&mut s, "material.create", json!({ "name": "a" })));
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    ok(&exec(
        &mut s,
        "face.set_material",
        json!({ "faces": [b], "material": "a" }),
    ));
    let r = ok(&exec(
        &mut s,
        "material.rename",
        json!({ "material": "a", "name": "z" }),
    ))
    .clone();
    assert_eq!(r["result"]["faces"], 6);
    assert!(faces(&mut s, &b).iter().all(|f| f["material"] == "z"));
    assert_eq!(
        err_code(&exec(&mut s, "material.create", json!({ "name": "z" }))),
        "InvalidArgument"
    );
    ok(&exec(
        &mut s,
        "material.delete",
        json!({ "materials": ["z"], "replace_with": "other" }),
    ));
    assert!(faces(&mut s, &b).iter().all(|f| f["material"] == "other"));
    let list = ok(&exec(&mut s, "material.list", json!({}))).clone();
    assert_eq!(list["result"]["undefined"], json!(["other"]));
    ok(&exec(&mut s, "history.undo", json!({})));
    assert!(faces(&mut s, &b).iter().all(|f| f["material"] == "z"));
}

#[test]
fn wrapping_continues_the_image_over_an_edge() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [64, 64, 64]);
    let (top, side) = (top_of(&mut s, &b), side_of(&mut s, &b));
    ok(&exec(
        &mut s,
        "face.set_material",
        json!({ "faces": [top], "material": "m" }),
    ));
    ok(&exec(
        &mut s,
        "face.set_uv",
        json!({ "faces": [top], "offset": [3, 5], "scale": [2, 2], "rotation": 15 }),
    ));
    ok(&exec(
        &mut s,
        "face.transfer_uv",
        json!({ "from": top, "to": [side], "mode": "wrap" }),
    ));
    let (ft, fs) = (
        faces(&mut s, &top).remove(0),
        faces(&mut s, &side).remove(0),
    );
    let shared: Vec<_> = ft["corners"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| {
            let p = &c["position"];
            fs["corners"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["position"] == *p)
                .map(|d| (c["texel"].clone(), d["texel"].clone()))
        })
        .collect();
    assert_eq!(shared.len(), 2);
    for (a, b) in shared {
        for axis in 0..2 {
            assert!(
                (a[axis].as_f64().unwrap() - b[axis].as_f64().unwrap()).abs() < 1e-6,
                "{a} vs {b}"
            );
        }
    }
    assert_eq!(fs["material"], "m");
}

#[test]
fn flip_keeps_the_face_showing_the_same_range() {
    let dir = temp("flip");
    let mut s = session();
    with_brick(&mut s, &dir);
    let b = make_box(&mut s, [0, 0, 0], [128, 64, 32]);
    let top = top_of(&mut s, &b);
    ok(&exec(
        &mut s,
        "face.set_material",
        json!({ "faces": [top], "material": "brick" }),
    ));
    ok(&exec(
        &mut s,
        "face.align_uv",
        json!({ "faces": [top], "action": { "type": "fit" } }),
    ));
    let before = faces(&mut s, &top).remove(0);
    ok(&exec(
        &mut s,
        "face.align_uv",
        json!({ "faces": [top], "action": { "type": "flip", "axis": "u" } }),
    ));
    let after = faces(&mut s, &top).remove(0);
    assert_eq!(before["texel_min"], after["texel_min"]);
    assert_eq!(before["texel_max"], after["texel_max"]);
    let u_at = |f: &Value, x: f64| {
        f["corners"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["position"][0].as_f64().unwrap() == x)
            .map(|c| c["texel"][0].as_f64().unwrap())
            .unwrap()
    };
    assert!((u_at(&before, 0.0) - (4.0 - u_at(&after, 0.0))).abs() < 1e-6);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn relative_set_uv_nudges() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    ok(&exec(
        &mut s,
        "face.set_uv",
        json!({ "faces": [b], "offset": [4, 0], "rotation": 10 }),
    ));
    ok(&exec(
        &mut s,
        "face.set_uv",
        json!({ "faces": [b], "offset": [1, 2], "rotation": 5, "scale": [2, 2], "mode": "relative" }),
    ));
    let f = &faces(&mut s, &b)[0];
    assert_eq!(f["attrs"]["offset"], json!([5.0, 2.0]));
    assert_eq!(f["attrs"]["rotation"], json!(15.0));
    assert_eq!(f["attrs"]["scale"], json!([2.0, 2.0]));
}

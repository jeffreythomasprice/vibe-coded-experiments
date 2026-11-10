use std::sync::OnceLock;

use image::RgbaImage;
use level_cmd::Session;
use level_core::Document;
use level_render::{BACKGROUND, Renderer, Shot, material_rgb8, screenshot};
use serde_json::{Value, json};

fn renderer() -> &'static Renderer {
    static R: OnceLock<Renderer> = OnceLock::new();
    R.get_or_init(|| Renderer::new().expect("these tests need a wgpu adapter (see CLAUDE.md)"))
}

fn session() -> Session {
    Session::new(Document::new("render"))
}

fn exec(s: &mut Session, op: &str, args: Value) -> Value {
    let v = s
        .execute_value(json!({ "op": op, "args": args }), "test")
        .to_value();
    assert_eq!(v["ok"], true, "{op}: {v}");
    v
}

fn make_box(s: &mut Session, min: [i32; 3], max: [i32; 3], material: &str) -> String {
    exec(
        s,
        "brush.create_box",
        json!({ "min": min, "max": max, "material": material }),
    )["result"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn shoot(s: &Session, args: Value) -> (Shot, RgbaImage) {
    let shot = screenshot(renderer(), s.document(), args).expect("screenshot");
    let img = image::load_from_memory_with_format(&shot.png, image::ImageFormat::Png)
        .unwrap()
        .to_rgba8();
    assert_eq!((img.width(), img.height()), (shot.width, shot.height));
    (shot, img)
}

fn rgb(img: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let p = img.get_pixel(x, y).0;
    [p[0], p[1], p[2]]
}

fn is_background(img: &RgbaImage, x: u32, y: u32) -> bool {
    rgb(img, x, y) == BACKGROUND
}

fn flat(camera: &str) -> Value {
    json!({ "camera": camera, "width": 400, "height": 400, "mode": "flat" })
}

#[test]
fn top_view_box_has_expected_colour_and_extent() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    let (_, img) = shoot(&s, flat("viewport.top"));

    assert_eq!(rgb(&img, 200, 200), material_rgb8("m/a"));
    for (x, y) in [(0, 0), (399, 0), (0, 399), (399, 399)] {
        assert!(is_background(&img, x, y), "corner {x},{y}");
    }
    let row: Vec<bool> = (0..400).map(|x| !is_background(&img, x, 200)).collect();
    let first = row.iter().position(|&b| b).unwrap() as i32;
    let last = row.iter().rposition(|&b| b).unwrap() as i32;
    let zoom = 400.0 / (64.0 * 1.1);
    let expect_first = (200.0 - 32.0 * zoom) as i32;
    let expect_last = (200.0 + 32.0 * zoom) as i32;
    assert!(
        (first - expect_first).abs() <= 2,
        "{first} vs {expect_first}"
    );
    assert!((last - expect_last).abs() <= 2, "{last} vs {expect_last}");
}

#[test]
fn view_orientation_matches_the_docs() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "left");
    make_box(&mut s, [64, 0, 0], [128, 64, 64], "right");
    let (_, top) = shoot(&s, flat("viewport.top"));
    assert_eq!(rgb(&top, 100, 200), material_rgb8("left"));
    assert_eq!(rgb(&top, 300, 200), material_rgb8("right"));

    let (_, front) = shoot(&s, flat("viewport.front"));
    assert_eq!(rgb(&front, 100, 200), material_rgb8("left"));
    assert_eq!(rgb(&front, 300, 200), material_rgb8("right"));

    let (_, side) = shoot(&s, flat("viewport.side"));
    assert!(!is_background(&side, 200, 200));
}

#[test]
fn nearer_objects_hide_farther_ones() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "near");
    make_box(&mut s, [0, 100, 0], [64, 164, 64], "far");
    let (_, front) = shoot(&s, flat("viewport.front"));
    assert_eq!(rgb(&front, 200, 200), material_rgb8("near"));
    let (_, back) = shoot(
        &s,
        json!({
            "camera": { "ortho": "front" }, "width": 400, "height": 400, "mode": "flat",
        }),
    );
    assert_eq!(rgb(&back, 200, 200), material_rgb8("near"));

    let (_, side) = shoot(&s, flat("viewport.side"));
    assert_eq!(rgb(&side, 90, 200), material_rgb8("near"));
    assert_eq!(rgb(&side, 310, 200), material_rgb8("far"));
}

#[test]
fn perspective_default_frames_the_scene() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    let (shot, img) = shoot(&s, json!({ "width": 320, "height": 240 }));
    assert_eq!((img.width(), img.height()), (320, 240));
    assert!(!is_background(&img, 160, 120));
    assert!(is_background(&img, 2, 2));
    assert!(is_background(&img, 317, 237));
    assert_eq!(shot.camera["name"], "viewport.persp");
    assert!(shot.camera["id"].is_null());
}

#[test]
fn donut_has_a_hole() {
    let mut s = session();
    let outer = exec(
        &mut s,
        "brush.create_prism",
        json!({
            "center": [0, 0, 0], "radius": 64, "sides": 16, "height": 32, "material": "ring",
        }),
    )["result"]["id"]
        .clone();
    let inner = exec(
        &mut s,
        "brush.create_prism",
        json!({
            "center": [0, 0, 0], "radius": 24, "sides": 16, "height": 64,
        }),
    )["result"]["id"]
        .clone();
    exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [outer], "cutters": [inner.clone()], "output": "mesh" }),
    );
    exec(&mut s, "object.delete", json!({ "ids": [inner] }));

    let (_, img) = shoot(&s, flat("viewport.top"));
    assert!(is_background(&img, 200, 200), "hole");
    assert_eq!(rgb(&img, 200 + 70, 200), material_rgb8("ring"));
    assert_eq!(rgb(&img, 200, 200 - 90), material_rgb8("ring"));
}

#[test]
fn wireframe_draws_edges_only() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    let (_, img) = shoot(
        &s,
        json!({ "camera": "viewport.top", "width": 400, "height": 400, "mode": "wireframe" }),
    );
    assert!(is_background(&img, 200, 200));
    let row = (0..400).filter(|&x| !is_background(&img, x, 200)).count();
    assert!(row >= 2, "edges on the box outline");
}

#[test]
fn shaded_mode_darkens_edges_and_shades_faces() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    let (_, shaded) = shoot(
        &s,
        json!({ "camera": "viewport.persp", "width": 400, "height": 400 }),
    );
    let (_, flat_img) = shoot(
        &s,
        json!({ "camera": "viewport.persp", "width": 400, "height": 400, "mode": "flat" }),
    );
    assert_ne!(shaded.as_raw(), flat_img.as_raw());
    let lit = |img: &RgbaImage| (0..400).filter(|&x| !is_background(img, x, 200)).count();
    assert!(lit(&shaded) > 20 && lit(&flat_img) > 20);
}

#[test]
fn grid_axes_and_bounds_overlays_draw_something() {
    let s = session();
    let base = json!({ "camera": "viewport.top", "width": 300, "height": 300 });
    let (_, empty) = shoot(&s, base.clone());
    assert!(empty.pixels().all(|p| [p[0], p[1], p[2]] == BACKGROUND));

    for overlay in ["grid", "axes"] {
        let mut args = base.clone();
        args["overlays"] = json!([overlay]);
        let (_, img) = shoot(&s, args);
        assert!(
            img.pixels().any(|p| [p[0], p[1], p[2]] != BACKGROUND),
            "{overlay}"
        );
    }

    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    let mut args = base.clone();
    args["mode"] = json!("wireframe");
    let (_, plain) = shoot(&s, args.clone());
    args["overlays"] = json!(["bounds"]);
    let (_, bounded) = shoot(&s, args);
    assert_ne!(plain.as_raw(), bounded.as_raw());
}

#[test]
fn labels_are_reported_and_drawn() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    let b = make_box(&mut s, [128, 0, 0], [192, 64, 64], "m/b");
    let base = json!({ "camera": "viewport.top", "width": 400, "height": 400, "mode": "flat" });
    let (plain, plain_img) = shoot(&s, base.clone());
    assert!(plain.labels.is_none());
    assert!(plain.result().get("labels").is_none());

    let mut args = base.clone();
    args["overlays"] = json!(["labels"]);
    let (shot, img) = shoot(&s, args);
    let labels = shot.labels.as_ref().unwrap();
    let ids: Vec<String> = labels.iter().map(|l| l.id.to_string()).collect();
    assert_eq!(ids, vec![a.clone(), b.clone()]);
    assert_eq!(shot.result()["labels"][0]["id"], a.as_str());

    let la = &labels[0];
    let lb = &labels[1];
    assert!(la.x < lb.x && la.y.abs_diff(lb.y) <= 1);
    let changed = (la.x.saturating_sub(10)..la.x + 10)
        .filter(|&x| rgb(&img, x, la.y) != rgb(&plain_img, x, la.y))
        .count();
    assert!(changed > 5, "label box drawn at the reported position");
}

#[test]
fn labels_skip_objects_outside_the_image() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    make_box(&mut s, [5000, 0, 0], [5064, 64, 64], "m/b");
    let (shot, _) = shoot(
        &s,
        json!({
            "camera": { "ortho": "top", "position": [32, 32, 0], "zoom": 2 },
            "width": 200, "height": 200, "overlays": ["labels"],
        }),
    );
    assert_eq!(shot.labels.as_ref().unwrap().len(), 1);
}

#[test]
fn highlight_changes_the_object_and_checks_ids() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    let (_, plain) = shoot(&s, flat("viewport.top"));
    let mut args = flat("viewport.top");
    args["highlight"] = json!([b]);
    let (_, lit) = shoot(&s, args);
    assert_ne!(rgb(&plain, 200, 200), rgb(&lit, 200, 200));

    let mut args = flat("viewport.top");
    args["highlight"] = json!(["b999"]);
    let err = screenshot(renderer(), s.document(), args).err().unwrap();
    assert_eq!(err.code(), "NotFound");
}

#[test]
fn inline_and_document_cameras() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    exec(
        &mut s,
        "view.camera.create",
        json!({ "name": "closeup", "ortho": "top", "zoom": 50 }),
    );
    let (shot, img) = shoot(&s, flat("closeup"));
    assert_eq!(shot.camera["name"], "closeup");
    assert_eq!(rgb(&img, 5, 5), material_rgb8("m/a"));

    let (shot, _) = shoot(
        &s,
        json!({
            "camera": { "position": [200, -200, 150], "target": [32, 32, 32], "fov": 50 },
            "width": 200, "height": 150,
        }),
    );
    assert_eq!(shot.camera["name"], "inline");
}

#[test]
fn argument_errors() {
    let s = session();
    let cases = [
        json!({ "width": 0 }),
        json!({ "height": 5000 }),
        json!({ "mode": "shiny" }),
        json!({ "overlays": ["sparkles"] }),
        json!({ "bogus": 1 }),
        json!({ "camera": "viewport.nope" }),
        json!({ "camera": { "fov": 40 } }),
    ];
    for args in cases {
        let err = screenshot(renderer(), s.document(), args.clone())
            .err()
            .unwrap();
        assert_eq!(err.code(), "InvalidArgument", "{args}: {err}");
    }
    let err = screenshot(renderer(), s.document(), json!({ "camera": "ghost" }))
        .err()
        .unwrap();
    assert_eq!(err.code(), "NotFound");
}

#[test]
fn invalid_brushes_are_skipped_with_a_warning() {
    let mut doc = Document::new("bad");
    let mut s = Session::new(doc.clone());
    let id = make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    doc = s.into_document();
    let level_core::ObjectId::Brush(bid) = id.parse().unwrap() else {
        unreachable!()
    };
    doc.brushes.get_mut(&bid).unwrap().brush.faces.truncate(3);
    let shot = screenshot(renderer(), &doc, json!({ "width": 64, "height": 64 })).unwrap();
    assert_eq!(shot.warnings.len(), 1);
    assert!(shot.warnings[0].starts_with(&id));
}

#[test]
fn selection_overlay_tints_only_when_requested() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [64, 64, 64], "base/a");
    exec(&mut s, "select.set", json!({ "ids": [b] }));
    let shoot_sel = |args: Value| {
        let shot =
            level_render::screenshot_with_selection(renderer(), s.document(), s.selection(), args)
                .expect("screenshot");
        image::load_from_memory_with_format(&shot.png, image::ImageFormat::Png)
            .unwrap()
            .to_rgba8()
    };
    let plain = shoot_sel(flat("viewport.top"));
    let mut args = flat("viewport.top");
    args["overlays"] = json!(["selection"]);
    let tinted = shoot_sel(args);
    assert_ne!(rgb(&plain, 200, 200), rgb(&tinted, 200, 200));
    assert_eq!(rgb(&plain, 200, 200), material_rgb8("base/a"));
}

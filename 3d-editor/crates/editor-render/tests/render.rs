use std::sync::OnceLock;

use editor_cmd::Session;
use editor_core::Document;
use editor_render::{BACKGROUND, Renderer, Shot, material_rgb8, screenshot};
use image::RgbaImage;
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
    assert!(
        serde_json::to_value(plain.result())
            .unwrap()
            .get("labels")
            .is_none()
    );

    let mut args = base.clone();
    args["overlays"] = json!(["labels"]);
    let (shot, img) = shoot(&s, args);
    let labels = shot.labels.as_ref().unwrap();
    let ids: Vec<String> = labels.iter().map(|l| l.id.to_string()).collect();
    assert_eq!(ids, vec![a.clone(), b.clone()]);
    assert_eq!(
        serde_json::to_value(shot.result()).unwrap()["labels"][0]["id"],
        a.as_str()
    );

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
    let editor_core::ObjectId::Brush(bid) = id.parse().unwrap() else {
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
            editor_render::screenshot_with_selection(renderer(), s.document(), s.selection(), args)
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

fn with(base: Value, extra: Value) -> Value {
    let mut base = base;
    for (k, v) in extra.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

fn label_ids(shot: &Shot) -> Vec<String> {
    shot.labels
        .as_ref()
        .unwrap()
        .iter()
        .map(|l| l.id.to_string())
        .collect()
}

#[test]
fn labels_skip_occluded_objects() {
    let mut s = session();
    let near = make_box(&mut s, [0, 0, 0], [64, 64, 64], "near");
    let far = make_box(&mut s, [8, 100, 8], [56, 164, 56], "far");
    let (shot, _) = shoot(
        &s,
        with(flat("viewport.front"), json!({ "overlays": ["labels"] })),
    );
    let ids = label_ids(&shot);
    assert!(ids.contains(&near), "{ids:?}");
    assert!(!ids.contains(&far), "{ids:?}");
    assert!(shot.labels.as_ref().unwrap().iter().all(|l| l.pixels >= 12));
}

#[test]
fn label_anchor_lies_on_the_object() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "wall");
    make_box(&mut s, [16, -40, 16], [48, 0, 48], "pillar");
    let (shot, img) = shoot(
        &s,
        with(
            flat("viewport.front"),
            json!({ "overlays": ["labels"], "mode": "flat" }),
        ),
    );
    for l in shot.labels.as_ref().unwrap() {
        assert!(!is_background(&img, l.x, l.y) || l.pixels > 0);
    }
    assert_eq!(shot.labels.as_ref().unwrap().len(), 2);
}

#[test]
fn label_ids_and_max_labels_restrict_labels() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [64, 64, 64], "a");
    make_box(&mut s, [100, 0, 0], [164, 64, 64], "b");
    let overlay = json!({ "overlays": ["labels"] });
    let (shot, _) = shoot(
        &s,
        with(
            flat("viewport.front"),
            with(overlay.clone(), json!({ "label_ids": [a] })),
        ),
    );
    assert_eq!(label_ids(&shot), vec![a]);
    let (shot, _) = shoot(
        &s,
        with(
            flat("viewport.front"),
            with(overlay, json!({ "max_labels": 1 })),
        ),
    );
    assert_eq!(shot.labels.as_ref().unwrap().len(), 1);
    assert_eq!(shot.labels_omitted, 1);
}

#[test]
fn hide_removes_objects_without_touching_the_document() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    let (_, img) = shoot(&s, with(flat("viewport.top"), json!({ "hide": [b] })));
    assert!(is_background(&img, 200, 200));
    assert_eq!(s.document().brushes.len(), 1);
}

#[test]
fn clip_above_reveals_the_floor() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 8], "floor");
    make_box(&mut s, [0, 0, 64], [64, 64, 72], "ceiling");
    let (_, img) = shoot(&s, flat("viewport.top"));
    assert_eq!(rgb(&img, 200, 200), material_rgb8("ceiling"));
    let (_, img) = shoot(
        &s,
        with(flat("viewport.top"), json!({ "clip_above": 32.0 })),
    );
    assert_eq!(rgb(&img, 200, 200), material_rgb8("floor"));
}

#[test]
fn legend_and_colour_overrides() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    exec(
        &mut s,
        "entity.create",
        json!({ "origin": [100, 0, 0], "components": { "light": {} } }),
    );
    let (shot, img) = shoot(
        &s,
        with(
            flat("viewport.top"),
            json!({ "material_colors": { "m/a": "#ff0000" } }),
        ),
    );
    assert_eq!(rgb(&img, 200, 200), [255, 0, 0]);
    let legend = serde_json::to_value(shot.result()).unwrap()["legend"].clone();
    assert_eq!(
        legend["faces"][0],
        json!({ "name": "m/a", "color": "#ff0000" })
    );
    assert_eq!(legend["entities"][0]["marker"], "octahedron");

    let (shot, _) = shoot(
        &s,
        with(flat("viewport.top"), json!({ "colors": "orientation" })),
    );
    let names: Vec<String> = serde_json::to_value(shot.result()).unwrap()["legend"]["faces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&"faces up (floors)".to_string()),
        "{names:?}"
    );
    assert_eq!(
        serde_json::to_value(shot.result()).unwrap()["legend"]["background"],
        "#292b30"
    );
    assert!(
        screenshot(
            renderer(),
            s.document(),
            with(
                flat("viewport.top"),
                json!({ "material_colors": { "m/a": "red" } })
            )
        )
        .is_err()
    );
}

#[test]
fn orientation_legend_names_floors_ceilings_and_walls() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "m/a");
    let (shot, _) = shoot(
        &s,
        with(
            with(flat("viewport.persp"), json!({ "colors": "orientation" })),
            json!({ "camera": { "position": [-80, -120, 90], "target": [32, 32, 32] } }),
        ),
    );
    let names: Vec<String> = serde_json::to_value(shot.result()).unwrap()["legend"]["faces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&"faces up (floors)".to_string()),
        "{names:?}"
    );
    assert!(names.contains(&"vertical (walls)".to_string()), "{names:?}");
}

#[test]
fn clip_above_gives_a_section_view_of_a_closed_room() {
    let mut s = session();
    exec(
        &mut s,
        "room.create",
        json!({ "min": [0, 0, 0], "max": [96, 96, 64], "wall": 8,
                "floor_material": "floor", "ceiling_material": "ceiling" }),
    );
    let (_, closed) = shoot(&s, flat("viewport.top"));
    assert_eq!(rgb(&closed, 200, 200), material_rgb8("ceiling"));
    let (_, open) = shoot(
        &s,
        with(flat("viewport.top"), json!({ "clip_above": 63.0 })),
    );
    assert_eq!(rgb(&open, 200, 200), material_rgb8("floor"));
}

#[test]
fn label_kinds_and_min_label_pixels_filter_labels() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [64, 64, 64], "a");
    exec(
        &mut s,
        "entity.create",
        json!({ "origin": [100, 32, 32], "components": { "light": {} } }),
    );
    let overlay = json!({ "overlays": ["labels"] });
    let (shot, _) = shoot(&s, with(flat("viewport.front"), overlay.clone()));
    assert_eq!(shot.labels.as_ref().unwrap().len(), 2);
    assert_eq!(
        serde_json::to_value(shot.result()).unwrap()["labels_drawn"],
        2
    );

    let (shot, _) = shoot(
        &s,
        with(
            flat("viewport.front"),
            with(overlay.clone(), json!({ "label_kinds": ["brush"] })),
        ),
    );
    assert_eq!(label_ids(&shot), vec![b.clone()]);
    assert_eq!(shot.labels_omitted, 0);

    let (shot, _) = shoot(
        &s,
        with(
            flat("viewport.front"),
            with(overlay.clone(), json!({ "min_label_pixels": 1_000_000 })),
        ),
    );
    assert!(shot.labels.as_ref().unwrap().is_empty());

    for bad in [
        json!({ "label_kinds": ["group"] }),
        json!({ "min_label_pixels": 0 }),
    ] {
        assert!(
            screenshot(
                renderer(),
                s.document(),
                with(flat("viewport.front"), with(overlay.clone(), bad))
            )
            .is_err()
        );
    }
}

#[test]
fn nudged_labels_get_leader_lines() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [400, 20, 4], "a");
    let b = make_box(&mut s, [0, 0, 4], [400, 20, 8], "b");
    let yellow = |img: &RgbaImage| img.pixels().filter(|p| p.0[..3] == [255, 235, 90]).count();
    let shoot_labels = |ids: Value| {
        shoot(
            &s,
            with(
                flat("viewport.front"),
                json!({ "overlays": ["labels"], "label_ids": ids }),
            ),
        )
    };
    let (shot, img) = shoot_labels(json!([a, b]));
    assert_eq!(shot.labels.as_ref().unwrap().len(), 2);
    assert!(yellow(&img) > 0);
    let (shot, img) = shoot_labels(json!([a]));
    assert_eq!(shot.labels.as_ref().unwrap().len(), 1);
    assert_eq!(yellow(&img), 0);
}

fn png_base64(width: u32, height: u32, color: impl Fn(u32, u32) -> [u8; 4]) -> String {
    use base64::Engine;
    let mut img = RgbaImage::new(width, height);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = image::Rgba(color(x, y));
    }
    let mut bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    )
    .unwrap();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 3)
}

fn textured_box(s: &mut Session) -> String {
    let data = png_base64(4, 2, |x, _| {
        if x < 2 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        }
    });
    exec(
        s,
        "material.create",
        json!({ "name": "flag", "texture": { "source": "base64", "format": "png", "data": data }, "filter": "nearest" }),
    );
    let b = make_box(s, [0, 0, 0], [64, 64, 64], "flag");
    exec(
        s,
        "face.align_uv",
        json!({ "faces": [b], "action": { "type": "fit" } }),
    );
    b
}

#[test]
fn textured_faces_show_the_image() {
    let mut s = session();
    textured_box(&mut s);
    let (shot, img) = shoot(&s, flat("viewport.top"));
    assert!(shot.warnings.is_empty(), "{:?}", shot.warnings);
    assert!(
        near(rgb(&img, 150, 200), [255, 0, 0]),
        "{:?}",
        rgb(&img, 150, 200)
    );
    assert!(
        near(rgb(&img, 250, 200), [0, 255, 0]),
        "{:?}",
        rgb(&img, 250, 200)
    );
}

#[test]
fn textures_can_be_turned_off() {
    let mut s = session();
    textured_box(&mut s);
    let (_, img) = shoot(
        &s,
        json!({ "camera": "viewport.top", "width": 400, "height": 400, "mode": "flat", "textures": false }),
    );
    assert_eq!(rgb(&img, 150, 200), material_rgb8("flag"));
    assert_eq!(rgb(&img, 250, 200), material_rgb8("flag"));
}

#[test]
fn rotated_images_follow_the_geometry() {
    let mut s = session();
    let b = textured_box(&mut s);
    exec(
        &mut s,
        "transform.rotate",
        json!({ "ids": [b], "axis": "z", "degrees": 180 }),
    );
    let (_, img) = shoot(&s, flat("viewport.top"));
    assert!(
        near(rgb(&img, 150, 200), [0, 255, 0]),
        "{:?}",
        rgb(&img, 150, 200)
    );
    assert!(
        near(rgb(&img, 250, 200), [255, 0, 0]),
        "{:?}",
        rgb(&img, 250, 200)
    );
}

#[test]
fn emissive_adds_light_and_a_missing_image_shows_a_checker() {
    let mut s = session();
    exec(
        &mut s,
        "material.create",
        json!({ "name": "glow", "color": [0, 0, 0, 1], "emissive": [1, 0, 0], "emissive_strength": 1 }),
    );
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "glow");
    let (_, img) = shoot(&s, flat("viewport.top"));
    assert!(
        near(rgb(&img, 200, 200), [255, 0, 0]),
        "{:?}",
        rgb(&img, 200, 200)
    );

    let dir = std::env::temp_dir().join(format!("editor-render-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("gone.png");
    RgbaImage::new(2, 2).save(&file).unwrap();
    exec(
        &mut s,
        "material.create",
        json!({ "name": "gone", "texture": { "source": "file", "path": file.display().to_string(), "store": "absolute" } }),
    );
    std::fs::remove_dir_all(&dir).unwrap();
    make_box(&mut s, [128, 0, 0], [192, 64, 64], "gone");
    let (shot, _) = shoot(&s, flat("viewport.top"));
    assert!(
        shot.warnings.iter().any(|w| w.contains("gone")),
        "{:?}",
        shot.warnings
    );
}

fn linked_pair(s: &mut Session) -> (String, String) {
    exec(
        s,
        "component_defs.add",
        json!({ "source": "embedded", "defs": { "version": 2, "components": [{
            "name": "relay", "color": [1, 0, 1],
            "size": { "min": [-4, -4, -4], "max": [4, 4, 4] },
            "schema": { "type": "object", "properties": {
                "link": { "type": "string", "editor": { "linkType": "entityID" } }
            } }
        }] } }),
    );
    let b = exec(
        s,
        "entity.create",
        json!({ "origin": [100, 0, 0], "components": { "relay": {} } }),
    )["result"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let a = exec(
        s,
        "entity.create",
        json!({ "origin": [-100, 0, 0], "components": { "relay": { "link": b } } }),
    )["result"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (a, b)
}

fn lit_pixels(img: &RgbaImage, x0: u32, x1: u32, y: u32) -> usize {
    (x0..x1).filter(|&x| !is_background(img, x, y)).count()
}

#[test]
fn class_colour_and_size_override_the_built_in_marker() {
    let mut s = session();
    linked_pair(&mut s);
    let (shot, img) = shoot(&s, flat("viewport.top"));
    assert!(
        shot.legend["entities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["component"] == "relay" && e["color"] == "#ff00ff"),
        "{:?}",
        shot.legend
    );
    assert!(
        (0..img.width()).any(|x| (0..img.height()).any(|y| rgb(&img, x, y) == [255, 0, 255])),
        "magenta marker is drawn"
    );
}

#[test]
fn links_overlay_draws_an_arrow_between_linked_entities() {
    let mut s = session();
    linked_pair(&mut s);
    let (_, plain) = shoot(&s, flat("viewport.top"));
    let (_, linked) = shoot(
        &s,
        with(flat("viewport.top"), json!({ "overlays": ["links"] })),
    );
    let mid = plain.height() / 2;
    let (x0, x1) = (plain.width() / 2 - 30, plain.width() / 2 + 30);
    assert_eq!(lit_pixels(&plain, x0, x1, mid), 0, "nothing between them");
    assert!(
        lit_pixels(&linked, x0, x1, mid) > 20,
        "the arrow crosses the gap"
    );
}

#[test]
fn dangling_links_draw_no_arrow() {
    let mut s = session();
    let (_, b) = linked_pair(&mut s);
    exec(&mut s, "object.delete", json!({ "ids": [b] }));
    let (_, plain) = shoot(&s, flat("viewport.top"));
    let (_, linked) = shoot(
        &s,
        with(flat("viewport.top"), json!({ "overlays": ["links"] })),
    );
    assert_eq!(plain.as_raw(), linked.as_raw());
}

fn animated_box(s: &mut Session) {
    make_box(s, [0, 0, 0], [64, 64, 64], "mover");
    make_box(s, [-200, 0, 0], [-136, 64, 64], "wall");
    let ents = exec(s, "scene.list", json!({ "kind": "brush" }));
    let mover = ents["result"]["objects"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let e = exec(
        s,
        "entity.create_brush_entity",
        json!({ "objects": [mover], "components": { "keyframe_animation": {
            "keyframes": [{ "time": 0 }, { "time": 2, "offset": [200, 0, 0] }, { "time": 4, "offset": [200, 0, 0] }]
        } } }),
    )["result"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let _ = e;
}

fn sim(time: f64) -> Value {
    json!({
        "camera": { "ortho": "top", "position": [0, 32, 0], "zoom": 1.0 },
        "width": 600, "height": 200, "mode": "flat",
        "simulate": { "time": time },
    })
}

#[test]
fn simulate_poses_animated_entities_and_leaves_the_rest() {
    let mut s = session();
    animated_box(&mut s);
    let (_, rest) = shoot(&s, sim(0.0));
    let (_, moved) = shoot(&s, sim(2.0));
    let (_, half) = shoot(&s, sim(1.0));

    let at = |img: &RgbaImage, wx: i32| rgb(img, (300 + wx) as u32, 100);
    assert_eq!(at(&rest, 32), material_rgb8("mover"));
    assert!(is_background(&moved, (300 + 32) as u32, 100));
    assert_eq!(at(&moved, 232), material_rgb8("mover"));
    assert_eq!(at(&half, 132), material_rgb8("mover"));
    for img in [&rest, &moved, &half] {
        assert_eq!(at(img, -168), material_rgb8("wall"));
    }
}

#[test]
fn editor_view_never_animates() {
    let mut s = session();
    animated_box(&mut s);
    let mut plain = sim(2.0);
    plain.as_object_mut().unwrap().remove("simulate");
    let (_, img) = shoot(&s, plain);
    assert_eq!(rgb(&img, 332, 100), material_rgb8("mover"));
}

#[test]
fn simulate_draws_no_markers_or_hidden_flags() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [64, 64, 64], "a");
    exec(
        &mut s,
        "entity.create",
        json!({ "origin": [100, 32, 32], "components": { "light": {} } }),
    );
    let (editor, _) = shoot(&s, flat("viewport.front"));
    assert!(!editor.legend["entities"].as_array().unwrap().is_empty());
    let mut args = flat("viewport.front");
    args["simulate"] = json!({});
    let (played, _) = shoot(&s, args);
    assert!(played.legend["entities"].as_array().unwrap().is_empty());
}

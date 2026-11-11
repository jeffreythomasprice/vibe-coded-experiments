mod common;

use common::*;
use editor_cmd::args::{Attach, CreateBox, NoArgs, Save, SelectIds, SelectKind, Translate};
use editor_core::ObjectKind;
use glam::DVec3;
use serde::Serialize;
use serde_json::to_value;

fn run(s: &mut editor_cmd::Session, op: &str, args: &impl Serialize) -> serde_json::Value {
    exec(s, op, to_value(args).unwrap())
}

#[test]
fn typed_args_are_accepted_by_the_ops() {
    let mut s = session();
    let created = run(
        &mut s,
        "brush.create_box",
        &CreateBox {
            min: DVec3::ZERO,
            max: DVec3::splat(8.0),
            layer: None,
            material: String::new(),
        },
    );
    let id = id_of(&created);
    let ids = vec![id.parse().unwrap()];

    ok(&run(&mut s, "select.set", &SelectIds { ids: ids.clone() }));
    ok(&run(
        &mut s,
        "select.all",
        &SelectKind {
            kind: Some(ObjectKind::Brush),
        },
    ));
    ok(&run(&mut s, "select.clear", &NoArgs::default()));
    ok(&run(
        &mut s,
        "transform.translate",
        &Translate {
            ids,
            delta: DVec3::new(16.0, 0.0, 0.0),
        },
    ));
}

#[test]
fn server_and_file_args_round_trip_through_the_strict_parser() {
    let save = to_value(Save {
        path: Some("a.scene.toml".into()),
        overwrite: true,
    })
    .unwrap();
    let back: Save = editor_cmd::parse_args(save.clone()).unwrap();
    assert_eq!(to_value(back).unwrap(), save);

    let attach = to_value(Attach {
        id: None,
        path: Some("a.scene.toml".into()),
        create: false,
    })
    .unwrap();
    let back: Attach = editor_cmd::parse_args(attach.clone()).unwrap();
    assert_eq!(to_value(back).unwrap(), attach);
}

#[test]
fn creating_a_box_and_selecting_it_is_one_undo_step() {
    let mut s = session();
    let created = run(
        &mut s,
        "brush.create_box",
        &CreateBox {
            min: DVec3::ZERO,
            max: DVec3::splat(8.0),
            layer: None,
            material: String::new(),
        },
    );
    let ids = vec![id_of(&created).parse().unwrap()];
    ok(&run(&mut s, "select.set", &SelectIds { ids }));

    let history = exec(&mut s, "history.list", serde_json::json!({}));
    let entries = ok(&history)["result"]["entries"].as_array().unwrap().len();
    assert_eq!(entries, 1, "{history}");

    ok(&run(&mut s, "history.undo", &NoArgs::default()));
    let summary = exec(&mut s, "scene.summary", serde_json::json!({}));
    assert_eq!(ok(&summary)["result"]["counts"]["brush"], 0, "{summary}");
}

#[test]
fn inspector_args_are_accepted_by_the_ops() {
    use editor_cmd::args::{
        Rename, SetClassname, SetLayer, SetMaterial, SetProperties, SetTransform,
    };
    use editor_core::{ElementRef, Part, PropertyValue};
    use serde_json::json;

    let mut s = session();
    let brush = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let entity = id_of(&exec(
        &mut s,
        "entity.create",
        json!({ "classname": "light", "origin": [0, 0, 0] }),
    ));
    let layer = id_of(&exec(&mut s, "layer.create", json!({ "name": "extra" })));
    let group = id_of(&exec(
        &mut s,
        "group.create",
        json!({ "objects": [&brush] }),
    ));

    ok(&run(
        &mut s,
        "object.rename",
        &Rename {
            id: group.parse().unwrap(),
            name: "walls".into(),
        },
    ));
    ok(&run(
        &mut s,
        "object.set_layer",
        &SetLayer {
            ids: vec![entity.parse().unwrap()],
            layer: layer.parse().unwrap(),
        },
    ));
    ok(&run(
        &mut s,
        "entity.set_classname",
        &SetClassname {
            ids: vec![entity.parse().unwrap()],
            classname: "spot".into(),
        },
    ));
    ok(&run(
        &mut s,
        "entity.set_properties",
        &SetProperties {
            ids: vec![entity.parse().unwrap()],
            set: [("intensity".to_string(), PropertyValue::Number(3.0))].into(),
            remove: vec![],
        },
    ));
    ok(&run(
        &mut s,
        "entity.set_transform",
        &SetTransform {
            id: entity.parse().unwrap(),
            origin: Some(DVec3::new(1.0, 2.0, 3.0)),
            angles: None,
        },
    ));
    ok(&run(
        &mut s,
        "face.set_material",
        &SetMaterial {
            faces: vec![ElementRef {
                object: brush.parse().unwrap(),
                part: Part::All,
            }],
            material: "base/brick".into(),
        },
    ));
}

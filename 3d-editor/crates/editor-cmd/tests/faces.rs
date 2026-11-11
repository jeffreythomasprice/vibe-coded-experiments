#![allow(clippy::cloned_ref_to_slice_refs)]
mod common;

use common::*;
use serde_json::json;

fn face_materials(s: &mut editor_cmd::Session, id: &str) -> Vec<String> {
    let v = full(s, id);
    let faces = v["brush"]["faces"]
        .as_array()
        .or_else(|| v["mesh"]["faces"].as_array())
        .unwrap();
    faces
        .iter()
        .map(|f| f["attrs"]["material"].as_str().unwrap().to_string())
        .collect()
}

fn unit_square_pyramid() -> serde_json::Value {
    json!({
        "vertices": [[0,0,0],[8,0,0],[8,8,0],[0,8,0],[4,4,8]],
        "faces": [
            { "verts": [0,3,2,1], "material": "floor" },
            { "verts": [0,1,4] }, { "verts": [1,2,4] }, { "verts": [2,3,4] }, { "verts": [3,0,4] },
        ],
    })
}

#[test]
fn set_material_on_one_face_and_on_all() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let faces = full(&mut s, &b)["brush"]["faces"].clone();
    let first = faces[0]["id"].as_u64().unwrap();

    ok(&exec(
        &mut s,
        "face.set_material",
        json!({ "faces": [format!("{b}/f{first}")], "material": "base/wall" }),
    ));
    let mats = face_materials(&mut s, &b);
    assert_eq!(mats.iter().filter(|m| *m == "base/wall").count(), 1);
    assert_eq!(mats.len(), 6);

    let r = exec(
        &mut s,
        "face.set_material",
        json!({ "faces": [&b], "material": "base/all" }),
    );
    assert_eq!(ok(&r)["result"]["faces"], 6);
    assert!(face_materials(&mut s, &b).iter().all(|m| m == "base/all"));

    exec(&mut s, "history.undo", json!({}));
    assert_eq!(
        face_materials(&mut s, &b)
            .iter()
            .filter(|m| *m == "base/wall")
            .count(),
        1
    );
}

#[test]
fn set_material_errors() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    assert_eq!(
        err_code(&exec(
            &mut s,
            "face.set_material",
            json!({ "faces": [format!("{b}/f99")], "material": "x" })
        )),
        "NotFound"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "face.set_material",
            json!({ "faces": [format!("{b}/v1")], "material": "x" })
        )),
        "InvalidArgument"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "face.set_material",
            json!({ "faces": [], "material": "x" })
        )),
        "InvalidArgument"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "face.set_material",
            json!({ "faces": ["b99"], "material": "x" })
        )),
        "NotFound"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "face.set_material",
            json!({ "faces": ["not-an-id"], "material": "x" })
        )),
        "InvalidArgument"
    );
    let layer = layer_id(&mut s);
    assert_eq!(
        err_code(&exec(
            &mut s,
            "face.set_material",
            json!({ "faces": [layer], "material": "x" })
        )),
        "WrongKind"
    );
}

#[test]
fn set_uv_is_partial_and_validated() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    ok(&exec(
        &mut s,
        "face.set_uv",
        json!({ "faces": [&b], "offset": [4, 2], "rotation": 45 }),
    ));
    let f = &full(&mut s, &b)["brush"]["faces"][0]["attrs"];
    assert_eq!(f["offset"], json!([4.0, 2.0]));
    assert_eq!(f["rotation"], 45.0);
    assert_eq!(f["scale"], json!([1.0, 1.0]));
    assert_eq!(
        err_code(&exec(&mut s, "face.set_uv", json!({ "faces": [&b] }))),
        "InvalidArgument"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "face.set_uv",
            json!({ "faces": [&b], "scale": [0, 1] })
        )),
        "InvalidArgument"
    );
}

#[test]
fn per_face_materials_survive_save_and_reload() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let first = full(&mut s, &b)["brush"]["faces"][0]["id"]
        .as_u64()
        .unwrap();
    ok(&exec(
        &mut s,
        "face.set_material",
        json!({ "faces": [format!("{b}/f{first}")], "material": "base/wall" }),
    ));
    ok(&exec(
        &mut s,
        "face.set_uv",
        json!({ "faces": [format!("{b}/f{first}")], "scale": [0.5, 2] }),
    ));
    let text = editor_fmt::write(s.document());
    let back = editor_fmt::read(&text).unwrap();
    assert_eq!(&back, s.document());
}

#[test]
fn mesh_create_validates() {
    let mut s = session();
    let m = id_of(&exec(&mut s, "mesh.create", unit_square_pyramid()));
    assert_eq!(
        full(&mut s, &m)["mesh"]["faces"].as_array().unwrap().len(),
        5
    );
    assert_eq!(
        exec(&mut s, "check.validate", json!({}))["result"]["errors"],
        0
    );

    let bad_index =
        json!({ "vertices": [[0,0,0],[1,0,0],[0,1,0]], "faces": [{ "verts": [0,1,7] }] });
    assert_eq!(
        err_code(&exec(&mut s, "mesh.create", bad_index)),
        "InvalidArgument"
    );
    let too_small = json!({ "vertices": [[0,0,0],[1,0,0],[0,1,0]], "faces": [{ "verts": [0,1] }] });
    assert_eq!(
        err_code(&exec(&mut s, "mesh.create", too_small)),
        "InvalidGeometry"
    );
    let open = json!({ "vertices": [[0,0,0],[8,0,0],[0,8,0]], "faces": [{ "verts": [0,1,2] }] });
    let r = exec(&mut s, "mesh.create", open);
    assert!(!ok(&r)["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn mesh_vertex_and_face_translate() {
    let mut s = session();
    let m = id_of(&exec(&mut s, "mesh.create", unit_square_pyramid()));
    ok(&exec(
        &mut s,
        "vertex.translate",
        json!({ "vertices": [format!("{m}/v5")], "delta": [0, 0, 8] }),
    ));
    assert_eq!(bounds(&mut s, &m).1[2], 16.0);

    let r = exec(
        &mut s,
        "face.translate",
        json!({ "faces": [format!("{m}/f1")], "delta": [0, 0, -1] }),
    );
    assert_eq!(ok(&r)["result"]["vertices"], 4);
    assert_eq!(bounds(&mut s, &m).0[2], -1.0);
    exec(&mut s, "history.undo", json!({}));
    assert_eq!(bounds(&mut s, &m).0[2], 0.0);

    assert_eq!(
        err_code(&exec(
            &mut s,
            "vertex.translate",
            json!({ "vertices": [format!("{m}/v99")], "delta": [1, 0, 0] })
        )),
        "NotFound"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "vertex.translate",
            json!({ "vertices": [format!("{m}/f1")], "delta": [1, 0, 0] })
        )),
        "InvalidArgument"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "vertex.translate",
            json!({ "delta": [1, 0, 0] })
        )),
        "InvalidArgument"
    );
}

#[test]
fn brush_corner_moves_by_position() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let r = exec(
        &mut s,
        "vertex.translate",
        json!({ "brush_vertices": [{ "brush": &b, "at": [8, 8, 8] }], "delta": [0, 0, 4] }),
    );
    assert_eq!(ok(&r)["result"]["vertices"], 1);
    assert_eq!(bounds(&mut s, &b).1[2], 12.0);
    assert_eq!(
        full(&mut s, &b)["brush"]["faces"].as_array().unwrap().len(),
        7
    );
    ok(&exec(&mut s, "check.validate", json!({})));

    exec(&mut s, "history.undo", json!({}));
    assert_eq!(bounds(&mut s, &b).1[2], 8.0);
    assert_eq!(
        full(&mut s, &b)["brush"]["faces"].as_array().unwrap().len(),
        6
    );
}

#[test]
fn brush_vertex_errors() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let r = exec(
        &mut s,
        "vertex.translate",
        json!({ "brush_vertices": [{ "brush": &b, "at": [8, 8, 8] }], "delta": [-4, -4, -4] }),
    );
    assert_eq!(err_code(&r), "InvalidGeometry");
    let r = exec(
        &mut s,
        "vertex.translate",
        json!({ "brush_vertices": [{ "brush": &b, "at": [3, 3, 3] }], "delta": [1, 0, 0] }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    let r = exec(
        &mut s,
        "vertex.translate",
        json!({ "vertices": [format!("{b}/v1")], "delta": [1, 0, 0] }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    assert_eq!(bounds(&mut s, &b).1, [8.0, 8.0, 8.0]);
}

#[test]
fn bare_brush_vertex_ref_moves_every_corner() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let r = exec(
        &mut s,
        "vertex.translate",
        json!({ "vertices": [&b], "delta": [2, 0, 0] }),
    );
    assert_eq!(ok(&r)["result"]["vertices"], 8);
    assert_eq!(bounds(&mut s, &b).0[0], 2.0);
}

#[test]
fn brush_face_translate_stretches_the_brush() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let faces = full(&mut s, &b)["brush"]["faces"].clone();
    let top = faces
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["plane"][0][2] == 8.0 && f["plane"][1][2] == 8.0 && f["plane"][2][2] == 8.0)
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let r = exec(
        &mut s,
        "face.translate",
        json!({ "faces": [format!("{b}/f{top}")], "delta": [0, 0, 4] }),
    );
    assert_eq!(ok(&r)["result"]["vertices"], 4);
    assert_eq!(bounds(&mut s, &b).1[2], 12.0);
    assert_eq!(
        full(&mut s, &b)["brush"]["faces"].as_array().unwrap().len(),
        6
    );

    let r = exec(
        &mut s,
        "face.translate",
        json!({ "faces": [format!("{b}/f99")], "delta": [0, 0, 1] }),
    );
    assert_eq!(err_code(&r), "NotFound");
}

#[test]
fn nonplanar_move_is_a_warning() {
    let mut s = session();
    let m = id_of(&exec(&mut s, "mesh.create", unit_square_pyramid()));
    let r = exec(
        &mut s,
        "vertex.translate",
        json!({ "vertices": [format!("{m}/v1")], "delta": [0, 0, 4] }),
    );
    assert!(
        ok(&r)["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("NonPlanar")),
        "{r}"
    );
}

#[test]
fn mesh_delete_faces() {
    let mut s = session();
    let m = id_of(&exec(&mut s, "mesh.create", unit_square_pyramid()));
    let r = exec(
        &mut s,
        "mesh.delete_faces",
        json!({ "faces": [format!("{m}/f1")] }),
    );
    assert_eq!(ok(&r)["result"]["removed"], 1);
    assert!(
        r["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("no longer closed"))
    );
    assert_eq!(
        err_code(&exec(&mut s, "mesh.delete_faces", json!({ "faces": [&m] }))),
        "InvalidArgument"
    );
    assert_eq!(
        err_code(&exec(
            &mut s,
            "mesh.delete_faces",
            json!({ "faces": [format!("{m}/f77")] })
        )),
        "NotFound"
    );
    let all: Vec<String> = (2..=4).map(|i| format!("{m}/f{i}")).collect();
    ok(&exec(&mut s, "mesh.delete_faces", json!({ "faces": all })));
    assert_eq!(
        err_code(&exec(
            &mut s,
            "mesh.delete_faces",
            json!({ "faces": [format!("{m}/f1")] })
        )),
        "NotFound"
    );
}

#[test]
fn brush_mesh_round_trip_keeps_parents() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let g = id_of(&exec(&mut s, "group.create", json!({ "objects": [&b] })));
    let r = exec(&mut s, "brush.to_mesh", json!({ "ids": [&b] }));
    let m = ids_of(&r)[0].clone();
    assert_eq!(r["changes"]["deleted"], json!([b]));
    assert_eq!(full(&mut s, &m)["group"], json!(g));

    let r = exec(&mut s, "mesh.to_brush", json!({ "ids": [&m] }));
    let b2 = ids_of(&r)[0].clone();
    assert_eq!(bounds(&mut s, &b2), ([0.0; 3], [8.0; 3]));
    assert_eq!(full(&mut s, &b2)["group"], json!(g));
}

#[test]
fn concave_mesh_does_not_become_a_brush() {
    let mut s = session();
    let a = make_box(&mut s, [0, 0, 0], [32, 32, 8]);
    let c = make_box(&mut s, [8, 8, -4], [24, 24, 12]);
    let r = exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [&a], "cutters": [&c], "output": "mesh" }),
    );
    let m = ids_of(&r)[0].clone();
    assert_eq!(
        err_code(&exec(&mut s, "mesh.to_brush", json!({ "ids": [&m] }))),
        "InvalidGeometry"
    );
    let d = make_box(&mut s, [100, 0, 0], [108, 8, 8]);
    assert_eq!(
        err_code(&exec(&mut s, "mesh.to_brush", json!({ "ids": [&d] }))),
        "WrongKind"
    );
}

#[test]
fn create_from_planes() {
    let mut s = session();
    // a box from six planes; normal = (c - a) x (b - a) points outward
    let planes = json!([
        { "plane": [[0,0,0],[0,8,0],[0,0,8]], "material": "m" },
        { "plane": [[8,0,0],[8,0,8],[8,8,0]] },
        { "plane": [[0,0,0],[0,0,8],[8,0,0]] },
        { "plane": [[0,8,0],[8,8,0],[0,8,8]] },
        { "plane": [[0,0,0],[8,0,0],[0,8,0]] },
        { "plane": [[0,0,8],[0,8,8],[8,0,8]] },
    ]);
    let r = exec(
        &mut s,
        "brush.create_from_planes",
        json!({ "faces": planes }),
    );
    let b = id_of(&r);
    assert_eq!(bounds(&mut s, &b), ([0.0; 3], [8.0; 3]));
    assert_eq!(
        err_code(&exec(
            &mut s,
            "brush.create_from_planes",
            json!({ "faces": [] })
        )),
        "InvalidGeometry"
    );
    let one = json!({ "plane": [[0,0,0],[0,8,0],[0,0,8]] });
    let open = json!([one, one, one, one]);
    assert_eq!(
        err_code(&exec(
            &mut s,
            "brush.create_from_planes",
            json!({ "faces": open })
        )),
        "InvalidGeometry"
    );
}

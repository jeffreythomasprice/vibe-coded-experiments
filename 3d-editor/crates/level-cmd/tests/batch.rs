mod common;

use common::*;
use serde_json::json;

#[test]
fn batch_accepts_underscored_names() {
    let mut s = session();
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "brush_create_box", "args": { "min": [0,0,0], "max": [8,8,8] } },
            { "op": "brush.create_box", "args": { "min": [16,0,0], "max": [24,8,8] } },
        ]}),
    );
    assert_eq!(ok(&r)["result"]["results"].as_array().unwrap().len(), 2);
    assert_eq!(s.document().brushes.len(), 2);
}

#[test]
fn batch_named_refs() {
    let mut s = session();
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "brush.create_box", "as": "wall", "args": { "min": [0,0,0], "max": [64,8,64] } },
            { "op": "brush.create_box", "as": "cutter", "args": { "min": [16,-4,0], "max": [32,12,32] } },
            { "op": "csg.subtract", "args": { "targets": ["$wall"], "cutters": ["$cutter"], "consume_cutters": true } },
        ]}),
    );
    ok(&r);
    let cutter = r["result"]["results"][1]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        !s.document()
            .brushes
            .keys()
            .any(|k| format!("b{}", k.0) == cutter)
    );
}

#[test]
fn batch_name_errors() {
    let mut s = session();
    let args = json!({ "min": [0,0,0], "max": [8,8,8] });
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "brush.create_box", "as": "a", "args": args },
            { "op": "brush.create_box", "as": "a", "args": args },
        ]}),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    assert_eq!(r["field"], "as");
    assert_eq!(r["index"], 1);

    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "brush.create_box", "as": "a", "args": args },
            { "op": "object.delete", "args": { "ids": ["$wall"] } },
        ]}),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    assert!(r["hint"].as_str().unwrap().contains("a"));
}

#[test]
fn batch_dollar_escape() {
    let mut s = session();
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "entity.create", "args": { "classname": "info_x", "origin": [0,0,0], "properties": { "note": "$$price" } } },
        ]}),
    );
    ok(&r);
    let e = full(&mut s, r["result"]["results"][0]["id"].as_str().unwrap());
    assert_eq!(e["properties"]["note"], "$price");
}

#[test]
fn batch_composite_refs() {
    let mut s = session();
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "mesh.create", "as": "m", "args": {
                "vertices": [[0,0,0],[8,0,0],[0,8,0],[0,0,8]],
                "faces": [
                    { "verts": [0,2,1] }, { "verts": [0,1,3] },
                    { "verts": [1,2,3] }, { "verts": [0,3,2] },
                ] } },
            { "op": "vertex.translate", "args": { "vertices": ["$m/v2"], "delta": [0,0,0] } },
            { "op": "face.set_material", "args": { "faces": ["$0/f1"], "material": "x" } },
        ]}),
    );
    ok(&r);
}

#[test]
fn clip_reports_modified_and_map() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [16, 16, 16]);
    let r = exec(
        &mut s,
        "brush.clip",
        json!({ "ids": [b], "plane": [[8,0,0],[8,1,0],[8,0,1]], "keep": "front" }),
    );
    ok(&r);
    assert_eq!(r["changes"]["modified"], json!([b]));
    assert_eq!(r["result"]["map"][&b], json!([b]));
}

#[test]
fn clip_both_map_lists_new_piece() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [16, 16, 16]);
    let r = exec(
        &mut s,
        "brush.clip",
        json!({ "ids": [b], "plane": [[8,0,0],[8,1,0],[8,0,1]], "keep": "both" }),
    );
    ok(&r);
    let new = ids_of(&r);
    assert_eq!(r["result"]["map"][&b], json!([b, new[0]]));
}

#[test]
fn subtract_reports_map_and_consumes() {
    let mut s = session();
    let t = make_box(&mut s, [0, 0, 0], [32, 8, 32]);
    let c = make_box(&mut s, [8, -4, 8], [16, 12, 16]);
    let r = exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [t], "cutters": [c], "consume_cutters": true }),
    );
    ok(&r);
    let created = ids_of(&r);
    assert!(!created.is_empty());
    assert_eq!(
        r["result"]["map"][&t].as_array().unwrap().len(),
        created.len()
    );
    assert!(
        r["changes"]["deleted"]
            .as_array()
            .unwrap()
            .contains(&json!(c))
    );
}

#[test]
fn to_mesh_reports_map() {
    let mut s = session();
    let b = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let r = exec(&mut s, "brush.to_mesh", json!({ "ids": [b] }));
    let m = id_of(&r);
    assert_eq!(r["result"]["map"][&b], json!([m]));
}

#[test]
fn parse_errors_name_the_expected_shape() {
    let mut s = session();
    let r = exec(
        &mut s,
        "mesh.create",
        json!({ "vertices": [[0,0,0],[1,0,0],[0,1,0]], "faces": [[0,1,2]] }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    assert!(r["field"].as_str().unwrap().starts_with("faces[0]"));
    let hint = r["hint"].as_str().unwrap();
    assert!(hint.contains("verts"), "{hint}");
    assert!(hint.contains("e.g."), "{hint}");
}

#[test]
fn validate_open_mesh_uses_element_refs() {
    let mut s = session();
    let r = exec(
        &mut s,
        "mesh.create",
        json!({ "vertices": [[0,0,0],[8,0,0],[0,8,0]], "faces": [{ "verts": [0,1,2] }] }),
    );
    let m = id_of(&r);
    let v = exec(&mut s, "check.validate", json!({}));
    let issue = &ok(&v)["result"]["issues"][0];
    assert_eq!(issue["code"], "mesh_open");
    let msg = issue["message"].as_str().unwrap();
    assert!(!msg.contains("VertexId"), "{msg}");
    assert!(msg.contains(&format!("{m}/v")), "{msg}");
    assert!(
        issue["elements"][0]
            .as_str()
            .unwrap()
            .starts_with(&format!("{m}/v"))
    );
}

#[test]
fn revision_advances_inside_transactions() {
    let mut s = session();
    ok(&exec(&mut s, "history.begin", json!({})));
    let r = exec(
        &mut s,
        "brush.create_box",
        json!({ "min": [0,0,0], "max": [8,8,8] }),
    );
    assert_eq!(ok(&r)["revision"], 1);
    let r = s
        .execute_value(
            json!({ "op": "brush.create_box", "args": { "min": [16,0,0], "max": [24,8,8] }, "expect_revision": 1 }),
            "test",
        )
        .to_value();
    assert_eq!(ok(&r)["revision"], 2);
    let r = exec(&mut s, "history.commit", json!({}));
    assert_eq!(ok(&r)["revision"], 2);
    assert_eq!(s.revision(), 2);
}

#[test]
fn rollback_never_rewinds_revision() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let before = s.revision();
    ok(&exec(&mut s, "history.begin", json!({})));
    make_box(&mut s, [16, 0, 0], [24, 8, 8]);
    ok(&exec(&mut s, "history.rollback", json!({})));
    assert!(s.revision() > before);
    assert_eq!(s.document().brushes.len(), 1);
}

#[test]
fn underscored_top_level_ops() {
    let mut s = session();
    let r = exec(
        &mut s,
        "brush_create_box",
        json!({ "min": [0,0,0], "max": [8,8,8] }),
    );
    ok(&r);
}

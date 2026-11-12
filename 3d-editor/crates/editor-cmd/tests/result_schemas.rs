mod common;

use std::collections::BTreeSet;

use common::*;
use editor_cmd::{Exec, OPS, OpInfo, Session};
use jsonschema::Validator;
use serde_json::{Value, json};

struct Run {
    s: Session,
    seen: BTreeSet<&'static str>,
}

impl Run {
    fn call(&mut self, op: &str, args: Value) -> Value {
        let info = OpInfo::find(op).unwrap_or_else(|| panic!("unknown op {op}"));
        let response = exec(&mut self.s, op, args);
        ok(&response);
        let schema = serde_json::to_value((info.response_schema)()).unwrap();
        let validator: Validator = jsonschema::validator_for(&schema)
            .unwrap_or_else(|e| panic!("{op}: bad response schema: {e}"));
        let errors: Vec<String> = validator
            .iter_errors(&response)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();
        assert!(errors.is_empty(), "{op}: {errors:#?}\nresponse: {response}");
        self.seen.insert(info.name);
        response
    }

    fn id(&mut self, op: &str, args: Value) -> String {
        id_of(&self.call(op, args))
    }
}

#[test]
fn every_document_and_session_op_returns_its_declared_result() {
    let dir = std::env::temp_dir().join(format!("editor-schemas-{}", std::process::id()));
    let mut r = Run {
        s: session(),
        seen: BTreeSet::new(),
    };
    r.call("file.info", json!({}));
    r.call("scene.summary", json!({}));
    r.call("select.get", json!({}));
    r.call("history.list", json!({}));
    r.call("view.camera.list", json!({}));
    r.call("view.camera.get", json!({ "camera": "viewport.persp" }));
    r.call("check.validate", json!({}));

    let a = r.id(
        "brush.create_box",
        json!({ "min": [0, 0, 0], "max": [64, 64, 64] }),
    );
    let b = r.id(
        "brush.create_box",
        json!({ "min": [32, 32, 0], "max": [96, 96, 64] }),
    );
    let wedge = r.id(
        "brush.create_wedge",
        json!({ "min": [200, 0, 0], "max": [264, 64, 64], "direction": "+x" }),
    );
    r.id(
        "brush.create_prism",
        json!({ "center": [400, 0, 0], "radius": 32, "sides": 6, "height": 64 }),
    );
    r.call(
        "brush.create_from_planes",
        json!({ "faces": [
            { "plane": [[0,0,0],[0,8,0],[0,0,8]] },
            { "plane": [[8,0,0],[8,0,8],[8,8,0]] },
            { "plane": [[0,0,0],[0,0,8],[8,0,0]] },
            { "plane": [[0,8,0],[8,8,0],[0,8,8]] },
            { "plane": [[0,0,0],[8,0,0],[0,8,0]] },
            { "plane": [[0,0,8],[0,8,8],[8,0,8]] },
        ] }),
    );
    let mesh_src = r.id(
        "brush.create_box",
        json!({ "min": [600, 0, 0], "max": [632, 32, 32] }),
    );
    let mesh = ids_of(&r.call("brush.to_mesh", json!({ "ids": [mesh_src] })))[0].clone();
    r.call("mesh.to_brush", json!({ "ids": [mesh] }));
    let mesh = ids_of(&r.call("brush.to_mesh", json!({ "ids": [wedge] })))[0].clone();
    r.call(
        "face.set_material",
        json!({ "faces": [format!("{mesh}/f1")], "material": "brick" }),
    );
    r.call(
        "face.set_uv",
        json!({ "faces": [format!("{mesh}/f1")], "scale": [2, 2] }),
    );
    let tex = dir.join("tex").join("brick.png");
    write_png(&tex, 4, 2, |x, _| {
        if x < 2 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        }
    });
    let brick = r.id(
        "material.create",
        json!({
            "name": "brick",
            "texture": { "source": "file", "path": tex.display().to_string(), "store": "absolute" },
            "color": [1, 0.9, 0.8, 1],
            "roughness": 0.5,
        }),
    );
    let png_b64 = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(std::fs::read(&tex).unwrap())
    };
    let inline = r.id(
        "material.create",
        json!({ "name": "inline", "texture": { "source": "base64", "format": "png", "data": png_b64 } }),
    );
    r.call("material.list", json!({}));
    r.call("material.get", json!({ "material": "brick" }));
    r.call(
        "material.set",
        json!({ "material": brick, "metallic": 0.25, "filter": "nearest" }),
    );
    r.call("material.embed", json!({}));
    r.call(
        "material.rename",
        json!({ "material": "inline", "name": "inline2" }),
    );
    r.call(
        "face.get_uv",
        json!({ "faces": [format!("{mesh}/f1"), format!("{a}/f1")] }),
    );
    r.call(
        "face.align_uv",
        json!({ "faces": [format!("{a}/f1")], "action": { "type": "fit", "repeat": [2, 1] } }),
    );
    r.call(
        "face.transfer_uv",
        json!({ "from": format!("{a}/f1"), "to": [format!("{a}/f2")], "mode": "wrap" }),
    );
    r.call("material.delete", json!({ "materials": [inline] }));
    r.call(
        "vertex.translate",
        json!({ "vertices": [format!("{mesh}/v1")], "delta": [0, 0, 0] }),
    );
    r.call(
        "face.translate",
        json!({ "faces": [format!("{mesh}/f1")], "delta": [0, 0, 0] }),
    );
    r.call(
        "mesh.delete_faces",
        json!({ "faces": [format!("{mesh}/f1")] }),
    );
    r.call(
        "mesh.create",
        json!({
            "vertices": [[0, 0, 900], [32, 0, 900], [0, 32, 900], [0, 0, 932]],
            "faces": [{ "verts": [0, 2, 1] }, { "verts": [0, 1, 3] }, { "verts": [1, 2, 3] }, { "verts": [2, 0, 3] }],
        }),
    );

    r.call(
        "brush.clip",
        json!({ "ids": [b], "plane": [[64, 0, 0], [64, 1, 0], [64, 0, 1]], "keep": "both" }),
    );
    let cutter = r.id(
        "brush.create_box",
        json!({ "min": [8, 8, 8], "max": [24, 24, 24] }),
    );
    r.call(
        "csg.subtract",
        json!({ "targets": [a], "cutters": [cutter] }),
    );
    let u1 = r.id(
        "brush.create_box",
        json!({ "min": [2000, 0, 0], "max": [2016, 16, 16] }),
    );
    let u2 = r.id(
        "brush.create_box",
        json!({ "min": [2008, 0, 0], "max": [2024, 16, 16] }),
    );
    r.call("csg.intersect", json!({ "ids": [u1, u2] }));
    let h1 = r.id(
        "brush.create_box",
        json!({ "min": [3000, 0, 0], "max": [3032, 32, 32] }),
    );
    r.call("csg.hollow", json!({ "ids": [h1], "thickness": 4 }));
    let w1 = r.id(
        "brush.create_box",
        json!({ "min": [4000, 0, 0], "max": [4016, 16, 16] }),
    );
    let w2 = r.id(
        "brush.create_box",
        json!({ "min": [4016, 0, 0], "max": [4032, 16, 16] }),
    );
    r.call("csg.union", json!({ "ids": [w1, w2] }));
    r.call(
        "room.create",
        json!({ "min": [1000, 0, 0], "max": [1128, 128, 96], "wall": 8, "name": "hall" }),
    );
    r.call(
        "room.cut_opening",
        json!({ "min": [1120, 32, 0], "max": [1140, 64, 64] }),
    );
    r.call(
        "stairs.create",
        json!({ "origin": [2000, 0, 0], "direction": "+x", "steps": 4, "rise": 8, "run": 16, "width": 32, "name": "stairs" }),
    );

    let t = r.id(
        "brush.create_box",
        json!({ "min": [3000, 0, 0], "max": [3032, 32, 32] }),
    );
    r.call(
        "transform.translate",
        json!({ "ids": [t], "delta": [8, 0, 0] }),
    );
    r.call(
        "transform.rotate",
        json!({ "ids": [t], "axis": "z", "degrees": 90 }),
    );
    r.call("transform.scale", json!({ "ids": [t], "factors": 1.0 }));
    r.call("transform.flip", json!({ "ids": [t], "axis": "x" }));
    let dup = ids_of(&r.call(
        "object.duplicate",
        json!({ "ids": [t], "offset": [64, 0, 0] }),
    ));
    r.call(
        "object.array",
        json!({ "ids": [t], "count": 2, "offset": [0, 64, 0] }),
    );
    let layer = r.id("layer.create", json!({ "name": "extra" }));
    r.call("object.set_layer", json!({ "ids": [t], "layer": layer }));
    r.call(
        "layer.set_omit_from_export",
        json!({ "id": layer, "omit": true }),
    );
    r.call("object.set_hidden", json!({ "ids": [t], "hidden": true }));
    r.call("object.isolate", json!({ "ids": [t] }));
    r.call("object.show_all", json!({}));
    r.call("object.set_locked", json!({ "ids": [t], "locked": true }));
    r.call("object.unlock_all", json!({}));
    let g = r.id("group.create", json!({ "objects": dup, "name": "pair" }));
    r.call("object.rename", json!({ "id": g, "name": "pair2" }));
    let m = r.id(
        "brush.create_box",
        json!({ "min": [6000, 0, 0], "max": [6008, 8, 8] }),
    );
    r.call("group.move", json!({ "objects": [m], "group": g }));
    r.call("group.ungroup", json!({ "ids": [g] }));

    let e = r.id("entity.create", json!({ "origin": [0, 0, 128] }));
    r.call(
        "entity.add_component",
        json!({ "ids": [e], "name": "lamp", "data": { "color": "1 1 1" } }),
    );
    r.call(
        "entity.update_component",
        json!({ "ids": [e], "name": "lamp", "set": { "intensity": 2 } }),
    );
    r.call(
        "entity.set_component",
        json!({ "ids": [e], "name": "lamp", "data": { "link": "e1" } }),
    );
    r.call(
        "entity.add_component",
        json!({ "ids": [e], "name": "keyframe_animation" }),
    );
    r.call(
        "entity.remove_component",
        json!({ "ids": [e], "name": "keyframe_animation" }),
    );
    let defs = r.id(
        "component_defs.add",
        json!({ "source": "embedded", "defs": { "version": 2, "components": [{
            "name": "lamp",
            "schema": { "type": "object", "properties": {
                "link": { "type": "string", "editor": { "linkType": "entityID" } }
            } }
        }] } }),
    );
    r.call("component_defs.list", json!({}));
    r.call("entity.links", json!({}));
    r.call("component_defs.embed", json!({}));
    r.call("component_defs.remove", json!({ "ids": [defs] }));
    r.call(
        "entity.set_transform",
        json!({ "id": e, "origin": [0, 0, 256] }),
    );
    let brush = r.id(
        "brush.create_box",
        json!({ "min": [4000, 0, 0], "max": [4032, 32, 32] }),
    );
    let be = r.id("entity.create_brush_entity", json!({ "objects": [brush] }));
    r.call("entity.release_objects", json!({ "ids": [brush] }));
    let _ = be;

    let k1 = r.id(
        "brush.create_box",
        json!({ "min": [7000, 0, 0], "max": [7064, 64, 64] }),
    );
    let k2 = r.id(
        "brush.create_box",
        json!({ "min": [7100, 0, 0], "max": [7164, 64, 64] }),
    );
    r.call("scene.list", json!({ "kind": "brush", "limit": 3 }));
    r.call("scene.get", json!({ "ids": [k1], "detail": "geometry" }));
    r.call("scene.get", json!({ "ids": [e], "detail": "full" }));
    r.call("scene.bounds", json!({ "ids": [k1, e] }));
    r.call(
        "scene.raycast",
        json!({ "origin": [-100, 32, 32], "dir": [1, 0, 0] }),
    );
    r.call(
        "scene.raycast",
        json!({ "origin": [-100, 5000, 32], "dir": [1, 0, 0] }),
    );
    r.call(
        "scene.query_box",
        json!({ "min": [-10, -10, -10], "max": [100, 100, 100] }),
    );
    r.call("scene.overlaps", json!({}));
    r.call("check.validate", json!({}));

    let cam = r.id(
        "view.camera.create",
        json!({ "name": "cam_one", "position": [0, -200, 100], "target": [0, 0, 0] }),
    );
    r.call(
        "view.camera.set",
        json!({ "camera": cam, "name": "cam_two" }),
    );
    r.call("view.frame", json!({ "ids": [k1], "camera": "cam_two" }));
    r.call("view.camera.get", json!({ "camera": "cam_two" }));
    r.call("view.camera.delete", json!({ "cameras": [cam] }));

    r.call("select.set", json!({ "ids": [k1] }));
    r.call("select.add", json!({ "ids": [k2] }));
    r.call("select.remove", json!({ "ids": [k2] }));
    r.call("select.invert", json!({}));
    r.call("select.all", json!({}));
    r.call("select.clear", json!({}));
    r.call("select.faces.set", json!({ "faces": [format!("{k1}/f1")] }));
    r.call("select.faces.add", json!({ "faces": [k2] }));
    r.call("select.faces.get", json!({}));
    r.call("select.faces.remove", json!({ "faces": [k2] }));
    r.call("select.faces.clear", json!({}));

    r.call("history.begin", json!({ "label": "x" }));
    r.call("object.delete", json!({ "ids": [dup[0]] }));
    r.call("history.commit", json!({}));
    r.call("history.begin", json!({}));
    r.call("history.rollback", json!({}));
    r.call("history.undo", json!({}));
    r.call("history.redo", json!({}));
    r.call(
        "batch",
        json!({ "commands": [
            { "op": "brush.create_box", "args": { "min": [0, 0, 5000], "max": [8, 8, 5008] }, "as": "x" },
            { "op": "layer.create", "args": { "name": "in_batch" } },
        ] }),
    );

    let spare = r.id("layer.create", json!({ "name": "spare" }));
    r.call("layer.delete", json!({ "id": spare }));

    let path = dir.join("a.scene.toml");
    r.call("file.save", json!({ "path": path.display().to_string() }));
    r.call("file.new", json!({ "discard": true }));
    r.call(
        "file.open",
        json!({ "path": path.display().to_string(), "discard": true }),
    );
    let _ = std::fs::remove_dir_all(&dir);

    let missing: Vec<_> = OPS
        .iter()
        .filter(|o| matches!(o.exec, Exec::Document | Exec::Session))
        .filter(|o| !r.seen.contains(o.name))
        .map(|o| o.name)
        .collect();
    assert!(
        missing.is_empty(),
        "ops without a result-schema fixture: {missing:?}"
    );
}

#[test]
fn registry_schemas_are_objects_and_closed() {
    for op in OPS {
        let result = serde_json::to_value((op.result_schema)()).unwrap();
        assert!(
            jsonschema::validator_for(&result).is_ok(),
            "{}: result schema does not compile",
            op.name
        );
        let response = serde_json::to_value((op.response_schema)()).unwrap();
        assert_eq!(response["type"], "object", "{}", op.name);
        assert_eq!(response["additionalProperties"], false, "{}", op.name);
    }
}

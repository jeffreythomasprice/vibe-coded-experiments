use editor_fmt::{LoadError, read, write};

const FIXTURE: &str = include_str!("fixtures/example.scene.toml");

#[test]
fn fixture_loads() {
    let doc = read(FIXTURE).unwrap();
    assert_eq!(doc.name, "example");
    assert_eq!(doc.next_id(), 22);
    assert_eq!(doc.component_defs.len(), 2);
    assert_eq!(doc.brushes.len(), 2);
    assert_eq!(doc.entities.len(), 3);
}

#[test]
fn fixture_is_canonical() {
    let doc = read(FIXTURE).unwrap();
    assert_eq!(write(&doc), FIXTURE);
}

#[test]
fn load_save_load() {
    let a = read(FIXTURE).unwrap();
    let b = read(&write(&a)).unwrap();
    assert_eq!(a, b);
}

#[test]
fn fixture_brushes_have_volume() {
    let doc = read(FIXTURE).unwrap();
    for b in doc.brushes.values() {
        let geo = editor_geom::brush_geometry(&b.brush).unwrap();
        assert!(geo.volume() > 0.0, "{} has volume {}", b.id, geo.volume());
        assert_eq!(
            geo.faces.len(),
            b.brush.faces.len(),
            "{} dropped faces",
            b.id
        );
    }
}

fn doc(body: &str) -> String {
    format!(
        "format = \"scene\"\nversion = 3\n[meta]\nname = \"t\"\nnext_id = 10\n[[layer]]\nid = \"l1\"\nname = \"main\"\n{body}"
    )
}

#[test]
fn unknown_keys_are_preserved() {
    let text =
        doc("[[entity]]\nid = \"e2\"\norigin = [0, 0, 0]\nlayer = \"l1\"\ncustom = { a = 1 }\n");
    let d = read(&text).unwrap();
    assert!(
        d.entities
            .values()
            .next()
            .unwrap()
            .extra
            .contains_key("custom")
    );
    assert!(write(&d).contains("custom = { a = 1 }"));
}

#[test]
fn next_id_is_repaired() {
    let text = doc("[[entity]]\nid = \"e50\"\norigin = [0, 0, 0]\nlayer = \"l1\"\n");
    assert_eq!(read(&text).unwrap().next_id(), 51);
}

#[test]
fn rejects_newer_version() {
    let err = read("format = \"scene\"\nversion = 99\n").unwrap_err();
    assert_eq!(
        err,
        LoadError::UnsupportedVersion {
            found: 99,
            supported: 3
        }
    );
}

#[test]
fn rejects_bad_toml() {
    assert!(matches!(
        read("format = ").unwrap_err(),
        LoadError::Parse { .. }
    ));
}

#[test]
fn rejects_duplicate_record_id() {
    let text = doc(
        "[[entity]]\nid = \"e2\"\norigin = [0, 0, 0]\nlayer = \"l1\"\n[[entity]]\nid = \"e2\"\norigin = [1, 1, 1]\nlayer = \"l1\"\n",
    );
    assert_eq!(
        read(&text).unwrap_err(),
        LoadError::DuplicateId { id: "e2".into() }
    );
}

#[test]
fn rejects_number_shared_across_kinds() {
    let text = doc(
        "[[entity]]\nid = \"e2\"\norigin = [0, 0, 0]\nlayer = \"l1\"\n[[group]]\nid = \"g2\"\nname = \"g\"\nlayer = \"l1\"\n",
    );
    assert!(matches!(
        read(&text).unwrap_err(),
        LoadError::DuplicateId { .. }
    ));
}

#[test]
fn rejects_duplicate_face_id() {
    let text = doc(
        "[[mesh]]\nid = \"m2\"\nlayer = \"l1\"\nvertices = [{ id = 1, p = [0, 0, 0] }]\nfaces = [{ id = 1, verts = [1] }, { id = 1, verts = [1] }]\n",
    );
    assert_eq!(
        read(&text).unwrap_err(),
        LoadError::DuplicateId { id: "m2/f1".into() }
    );
}

#[test]
fn rejects_dangling_reference() {
    let text = doc("[[entity]]\nid = \"e2\"\norigin = [0, 0, 0]\nlayer = \"l9\"\n");
    let err = read(&text).unwrap_err();
    assert!(
        matches!(err, LoadError::DanglingReference { field: "layer", .. }),
        "{err:?}"
    );
}

#[test]
fn rejects_group_cycle() {
    let text = doc(
        "[[group]]\nid = \"g2\"\nname = \"a\"\nlayer = \"l1\"\ngroup = \"g3\"\n[[group]]\nid = \"g3\"\nname = \"b\"\nlayer = \"l1\"\ngroup = \"g2\"\n",
    );
    assert!(matches!(
        read(&text).unwrap_err(),
        LoadError::GroupCycle { .. }
    ));
}

#[test]
fn rejects_invalid_brush() {
    let text = doc(
        "[[brush]]\nid = \"b2\"\nlayer = \"l1\"\nfaces = [\n { id = 1, plane = [[0,0,0],[0,1,0],[1,0,0]] },\n { id = 2, plane = [[0,0,1],[1,0,1],[0,1,1]] },\n { id = 3, plane = [[0,0,0],[0,0,1],[0,1,0]] },\n]\n",
    );
    assert!(matches!(
        read(&text).unwrap_err(),
        LoadError::InvalidGeometry { .. }
    ));
}

#[test]
fn rejects_mesh_with_missing_vertex() {
    let text = doc(
        "[[mesh]]\nid = \"m2\"\nlayer = \"l1\"\nvertices = [{ id = 1, p = [0, 0, 0] }]\nfaces = [{ id = 1, verts = [1, 2, 3] }]\n",
    );
    assert!(matches!(
        read(&text).unwrap_err(),
        LoadError::InvalidGeometry { .. }
    ));
}

#[test]
fn hidden_and_locked_flags_round_trip() {
    let text = "format = \"scene\"\nversion = 3\n[meta]\nname = \"t\"\nnext_id = 10\n[[layer]]\nid = \"l1\"\nname = \"main\"\nhidden = true\nomit_from_export = true\n\n[[group]]\nid = \"g2\"\nname = \"g\"\nlayer = \"l1\"\nlocked = true\n\n[[entity]]\nid = \"e3\"\norigin = [0, 0, 0]\nlayer = \"l1\"\ngroup = \"g2\"\nhidden = true\nlocked = true\n";
    let d = read(text).unwrap();
    let layer = d.layers.values().next().unwrap();
    assert!(layer.flags.hidden && !layer.flags.locked && layer.omit_from_export);
    assert!(d.groups.values().next().unwrap().flags.locked);
    let e = d.entities.values().next().unwrap();
    assert!(e.flags.hidden && e.flags.locked);
    assert!(e.extra.is_empty());

    let out = write(&d);
    assert!(out.contains("hidden = true"));
    assert_eq!(read(&out).unwrap(), d);
}

#[test]
fn unflagged_objects_write_no_flag_keys() {
    let out = write(&read(FIXTURE).unwrap());
    assert!(
        !out.contains("hidden") && !out.contains("locked") && !out.contains("omit_from_export")
    );
}

#[test]
fn flags_must_be_booleans() {
    let text =
        doc("[[entity]]\nid = \"e2\"\norigin = [0, 0, 0]\nlayer = \"l1\"\nhidden = \"yes\"\n");
    assert!(matches!(read(&text), Err(LoadError::Parse { .. })));
}

#[test]
fn fixture_materials_load() {
    let doc = read(FIXTURE).unwrap();
    assert_eq!(doc.materials.len(), 2);
    let trim = doc.material_by_name("base/trim").unwrap();
    assert_eq!(
        trim.texture,
        Some(editor_core::TextureSource::Path {
            path: "textures/trim.png".into()
        })
    );
    assert_eq!(trim.filter, editor_core::Filter::Nearest);
    let dot = doc.material_by_name("base/dot").unwrap();
    assert!(matches!(
        dot.texture,
        Some(editor_core::TextureSource::Embedded { .. })
    ));
    let face = &doc.meshes.values().next().unwrap().mesh.faces[0];
    assert!(face.attrs.axes.is_some());
}

#[test]
fn old_versions_are_rejected() {
    let err =
        read("format = \"scene\"\nversion = 2\n[meta]\nname = \"old\"\nnext_id = 2\n").unwrap_err();
    assert_eq!(
        err,
        LoadError::VersionTooOld {
            found: 2,
            oldest: 3
        }
    );
}

#[test]
fn duplicate_material_names_are_rejected() {
    let text =
        doc("[[material]]\nid = \"t2\"\nname = \"a\"\n[[material]]\nid = \"t3\"\nname = \"a\"\n");
    assert!(matches!(
        read(&text),
        Err(LoadError::DuplicateMaterialName { .. })
    ));
}

#[test]
fn lone_axis_is_rejected() {
    let text = doc(
        "[[brush]]\nid = \"b5\"\nlayer = \"l1\"\nfaces = [\n  { id = 1, plane = [[0,0,0],[0,1,0],[1,0,0]], u_axis = [1, 0, 0] },\n]\n",
    );
    assert!(matches!(read(&text), Err(LoadError::Parse { .. })));
}

#[test]
fn nested_properties_round_trip_exactly() {
    let doc = read(FIXTURE).unwrap();
    let door = &doc.entities[&editor_core::EntityId(7)];
    assert_eq!(
        door.components["button"]["targets"],
        serde_json::json!(["e5"])
    );
    let keys = &door.components["keyframe_animation"]["keyframes"];
    assert_eq!(keys[1]["time"], serde_json::json!(1.5));
    assert!(keys[0]["time"].is_i64());
}

#[test]
fn dates_in_properties_are_rejected() {
    let text = FIXTURE.replace("intensity = 300", "intensity = 1979-05-27");
    assert!(matches!(read(&text), Err(LoadError::Parse { .. })));
}

#[test]
fn defs_need_exactly_one_source() {
    let text = FIXTURE.replace(
        "path = \"defs/game.components.json\"",
        "path = \"a\"\njson = \"{}\"",
    );
    assert!(matches!(read(&text), Err(LoadError::Parse { .. })));
}

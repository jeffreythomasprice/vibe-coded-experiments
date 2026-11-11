use editor_fmt::{LoadError, read, write};

const FIXTURE: &str = include_str!("fixtures/example.scene.toml");

#[test]
fn fixture_loads() {
    let doc = read(FIXTURE).unwrap();
    assert_eq!(doc.name, "example");
    assert_eq!(doc.next_id(), 20);
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
        "format = \"scene\"\nversion = 1\n[meta]\nname = \"t\"\nnext_id = 10\n[[layer]]\nid = \"l1\"\nname = \"main\"\n{body}"
    )
}

#[test]
fn unknown_keys_are_preserved() {
    let text =
        doc("[[entity]]\nid = \"e2\"\nclassname = \"x\"\nlayer = \"l1\"\ncustom = { a = 1 }\n");
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
    let text = doc("[[entity]]\nid = \"e50\"\nclassname = \"x\"\nlayer = \"l1\"\n");
    assert_eq!(read(&text).unwrap().next_id(), 51);
}

#[test]
fn rejects_newer_version() {
    let err = read("format = \"scene\"\nversion = 99\n").unwrap_err();
    assert_eq!(
        err,
        LoadError::UnsupportedVersion {
            found: 99,
            supported: 1
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
        "[[entity]]\nid = \"e2\"\nclassname = \"x\"\nlayer = \"l1\"\n[[entity]]\nid = \"e2\"\nclassname = \"y\"\nlayer = \"l1\"\n",
    );
    assert_eq!(
        read(&text).unwrap_err(),
        LoadError::DuplicateId { id: "e2".into() }
    );
}

#[test]
fn rejects_number_shared_across_kinds() {
    let text = doc(
        "[[entity]]\nid = \"e2\"\nclassname = \"x\"\nlayer = \"l1\"\n[[group]]\nid = \"g2\"\nname = \"g\"\nlayer = \"l1\"\n",
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
    let text = doc("[[entity]]\nid = \"e2\"\nclassname = \"x\"\nlayer = \"l9\"\n");
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

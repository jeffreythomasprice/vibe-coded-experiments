use serde_json::Value;

fn def_names() -> Vec<String> {
    editor_schema::bundled()["$defs"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect()
}

#[test]
fn every_definition_compiles() {
    for name in def_names() {
        let schema = editor_schema::schema_value(&name).unwrap();
        jsonschema::validator_for(&schema).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn every_ref_resolves() {
    fn walk(value: &Value, defs: &serde_json::Map<String, Value>, at: &str) {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    if k == "$ref" {
                        let r = v.as_str().unwrap();
                        let name = r
                            .strip_prefix("#/$defs/")
                            .unwrap_or_else(|| panic!("{at}: non-local $ref {r}"));
                        assert!(defs.contains_key(name), "{at}: unknown $ref {r}");
                    } else {
                        walk(v, defs, at);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|v| walk(v, defs, at)),
            _ => {}
        }
    }
    let defs = editor_schema::bundled()["$defs"].as_object().unwrap();
    for (name, def) in defs {
        walk(def, defs, name);
    }
}

#[test]
fn objects_with_properties_are_closed() {
    fn walk(value: &Value, at: &str) {
        match value {
            Value::Object(map) => {
                if map.contains_key("properties") && map.get("type").is_some_and(|t| t == "object")
                {
                    assert!(
                        map.contains_key("additionalProperties"),
                        "{at}: object with properties must say additionalProperties"
                    );
                }
                for (k, v) in map {
                    walk(v, &format!("{at}/{k}"));
                }
            }
            Value::Array(items) => items
                .iter()
                .enumerate()
                .for_each(|(i, v)| walk(v, &format!("{at}/{i}"))),
            _ => {}
        }
    }
    for (name, def) in editor_schema::bundled()["$defs"].as_object().unwrap() {
        walk(def, name);
    }
}

#[test]
fn unknown_definition_is_an_error() {
    assert!(matches!(
        editor_schema::schema_value("Nope"),
        Err(editor_schema::BundleError::UnknownDef { .. })
    ));
}

#[test]
fn duplicate_definitions_are_rejected() {
    let a = r##"{"$defs":{"X":{"type":"string"}}}"##;
    let err = editor_schema::bundle(&[("a.json", a), ("b.json", a)]).unwrap_err();
    assert!(matches!(
        err,
        editor_schema::BundleError::DuplicateDef { .. }
    ));
}

#[test]
fn cross_file_refs_become_local() {
    let a = r##"{"$defs":{"X":{"$ref":"b.schema.json#/$defs/Y"}}}"##;
    let b = r##"{"$defs":{"Y":{"type":"string"}}}"##;
    let bundled = editor_schema::bundle(&[("a.schema.json", a), ("b.schema.json", b)]).unwrap();
    assert_eq!(bundled["$defs"]["X"]["$ref"], "#/$defs/Y");
}

use editor_core::ComponentDefsFile;
use editor_schema::{ParseError, parse_str};

#[test]
fn minimal_file_parses() {
    let defs =
        parse_str::<ComponentDefsFile>(r#"{"version":2,"components":[{"name":"a"}]}"#).unwrap();
    assert_eq!(defs.components[0].name.as_str(), "a");
    assert!(
        defs.components[0]
            .schema
            .as_object()
            .is_some_and(|s| s.is_empty())
    );
}

#[test]
fn bad_name_points_at_the_field() {
    let err = parse_str::<ComponentDefsFile>(r#"{"version":2,"components":[{"name":"a b"}]}"#)
        .unwrap_err();
    assert_eq!(err.pointer(), Some("/components/0/name"), "{err}");
}

#[test]
fn unknown_key_is_rejected() {
    let err = parse_str::<ComponentDefsFile>(r#"{"version":2,"components":[],"extra":true}"#)
        .unwrap_err();
    assert!(matches!(err, ParseError::Invalid(_)), "{err}");
}

#[test]
fn wrong_size_arity_is_rejected() {
    let err = parse_str::<ComponentDefsFile>(
        r#"{"version":2,"components":[{"name":"a","size":{"min":[0,0],"max":[1,1,1]}}]}"#,
    )
    .unwrap_err();
    assert_eq!(err.pointer(), Some("/components/0/size/min"), "{err}");
}

#[test]
fn syntax_error_is_not_a_violation() {
    assert!(matches!(
        parse_str::<ComponentDefsFile>("{"),
        Err(ParseError::Syntax { .. })
    ));
}

#[test]
fn round_trips_through_serde() {
    let text = r#"{"version":2,"components":[{"name":"a","color":[0.5,0.25,1.0],"size":{"min":[-1,-1,-1],"max":[1,1,1]}}]}"#;
    let defs = parse_str::<ComponentDefsFile>(text).unwrap();
    let again = parse_str::<ComponentDefsFile>(&serde_json::to_string(&defs).unwrap()).unwrap();
    assert_eq!(defs, again);
}

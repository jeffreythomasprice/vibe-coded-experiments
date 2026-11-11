#![allow(dead_code)]

use editor_cmd::Session;
use editor_core::Document;
use serde_json::{Value, json};

pub fn session() -> Session {
    Session::new(Document::new("test"))
}

pub fn exec(s: &mut Session, op: &str, args: Value) -> Value {
    s.execute_value(json!({ "op": op, "args": args }), "test")
        .to_value()
}

pub fn ok(v: &Value) -> &Value {
    assert_eq!(v["ok"], true, "expected success, got {v}");
    v
}

pub fn err_code(v: &Value) -> &str {
    assert_eq!(v["ok"], false, "expected failure, got {v}");
    v["code"].as_str().unwrap()
}

pub fn id_of(v: &Value) -> String {
    ok(v)["result"]["id"].as_str().unwrap().to_string()
}

pub fn ids_of(v: &Value) -> Vec<String> {
    ok(v)["result"]["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

pub fn make_box(s: &mut Session, min: [i32; 3], max: [i32; 3]) -> String {
    id_of(&exec(
        s,
        "brush.create_box",
        json!({ "min": min, "max": max }),
    ))
}

pub fn bounds(s: &mut Session, id: &str) -> ([f64; 3], [f64; 3]) {
    let r = exec(s, "scene.get", json!({ "ids": [id] }));
    let b = &ok(&r)["result"]["objects"][0]["bounds"];
    let v = |k: &str| {
        let a = b[k].as_array().unwrap();
        [
            a[0].as_f64().unwrap(),
            a[1].as_f64().unwrap(),
            a[2].as_f64().unwrap(),
        ]
    };
    (v("min"), v("max"))
}

pub fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
}

pub fn summary(s: &mut Session) -> Value {
    exec(s, "scene.summary", json!({}))["result"].clone()
}

pub fn layer_id(s: &mut Session) -> String {
    summary(s)["layers"][0]["id"].as_str().unwrap().to_string()
}

pub fn full(s: &mut Session, id: &str) -> Value {
    exec(s, "scene.get", json!({ "ids": [id], "detail": "full" }))["result"]["objects"][0].clone()
}

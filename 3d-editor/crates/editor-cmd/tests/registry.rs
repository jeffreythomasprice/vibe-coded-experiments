mod common;

use common::*;
use editor_cmd::{Exec, OPS};
use serde_json::json;

#[test]
fn every_op_has_an_object_schema() {
    for op in OPS {
        let schema = (op.args_schema)();
        let v = serde_json::to_value(&schema).unwrap();
        assert!(v.is_object(), "{}: schema is not an object", op.name);
        assert!(!op.summary.is_empty(), "{}: missing summary", op.name);
    }
}

#[test]
fn names_are_unique() {
    let mut names: Vec<_> = OPS.iter().map(|o| o.name).collect();
    names.sort();
    let before = names.len();
    names.dedup();
    assert_eq!(before, names.len());
}

#[test]
fn every_op_is_reachable() {
    for op in OPS.iter().filter(|o| o.exec != Exec::Client) {
        let mut s = session();
        let r = exec(&mut s, op.name, json!({}));
        if r["ok"] == false {
            assert!(
                !r["message"].as_str().unwrap().starts_with("unknown op"),
                "{}: {r}",
                op.name
            );
        }
    }
}

#[test]
fn session_and_client_ops_are_not_batchable() {
    for op in OPS.iter().filter(|o| o.exec != Exec::Document) {
        let mut s = session();
        let r = exec(
            &mut s,
            "batch",
            json!({ "commands": [{ "op": op.name, "args": {} }] }),
        );
        assert_eq!(err_code(&r), "InvalidArgument", "{}", op.name);
    }
}

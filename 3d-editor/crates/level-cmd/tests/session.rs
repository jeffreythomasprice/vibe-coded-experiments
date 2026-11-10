mod common;

use common::*;
use level_cmd::Session;
use level_core::Document;
use proptest::prelude::*;
use serde_json::json;

#[test]
fn dry_run_predicts_real_run_and_changes_nothing() {
    let mut s = session();
    let before = s.document().clone();
    let args = json!({ "min": [0, 0, 0], "max": [8, 8, 8] });
    let dry = s
        .execute_value(
            json!({ "op": "brush.create_box", "args": args, "dry_run": true }),
            "t",
        )
        .to_value();
    ok(&dry);
    assert_eq!(s.document(), &before);
    assert_eq!(s.revision(), 0);

    let real = exec(&mut s, "brush.create_box", args);
    assert_eq!(dry["changes"], real["changes"]);
    assert_eq!(dry["result"], real["result"]);
    assert_eq!(real["revision"], 1);
}

#[test]
fn expect_revision_mismatch() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let r = s
        .execute_value(
            json!({ "op": "brush.create_box", "args": { "min": [0,0,0], "max": [1,1,1] }, "expect_revision": 0 }),
            "t",
        )
        .to_value();
    assert_eq!(err_code(&r), "RevisionMismatch");
    let r = s
        .execute_value(json!({ "op": "scene.summary", "expect_revision": 1 }), "t")
        .to_value();
    ok(&r);
}

#[test]
fn malformed_request() {
    let mut s = session();
    let r = s.execute_value(json!({ "args": {} }), "t").to_value();
    assert_eq!(err_code(&r), "InvalidArgument");
    let r = s
        .execute_value(json!({ "op": "scene.summary", "extra": 1 }), "t")
        .to_value();
    assert_eq!(err_code(&r), "InvalidArgument");
}

#[test]
fn explicit_transaction_groups_into_one_undo_step() {
    let mut s = session();
    ok(&exec(
        &mut s,
        "history.begin",
        json!({ "label": "two boxes" }),
    ));
    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let bad = exec(
        &mut s,
        "brush.create_box",
        json!({ "min": [5, 5, 5], "max": [1, 1, 1] }),
    );
    assert_eq!(err_code(&bad), "InvalidArgument");
    make_box(&mut s, [10, 0, 0], [18, 8, 8]);
    assert_eq!(s.revision(), 2);

    let r = exec(&mut s, "history.begin", json!({}));
    assert_eq!(err_code(&r), "TransactionOpen");
    let r = exec(&mut s, "history.undo", json!({}));
    assert_eq!(err_code(&r), "TransactionOpen");

    let c = exec(&mut s, "history.commit", json!({}));
    assert_eq!(c["changes"]["created"].as_array().unwrap().len(), 2);
    assert_eq!(s.revision(), 2);
    assert_eq!(s.document().brushes.len(), 2);

    let l = exec(&mut s, "history.list", json!({}));
    assert_eq!(l["result"]["entries"][0]["label"], "two boxes");
    assert_eq!(l["result"]["entries"][0]["client"], "test");

    ok(&exec(&mut s, "history.undo", json!({})));
    assert!(s.document().brushes.is_empty());
    ok(&exec(&mut s, "history.redo", json!({})));
    assert_eq!(s.document().brushes.len(), 2);
}

#[test]
fn transaction_rollback_restores_everything() {
    let mut s = session();
    let before = s.document().clone();
    ok(&exec(&mut s, "history.begin", json!({})));
    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    ok(&exec(&mut s, "history.rollback", json!({})));
    assert_eq!(s.document(), &before);
    assert_eq!(
        err_code(&exec(&mut s, "history.commit", json!({}))),
        "NoTransaction"
    );
    assert_eq!(
        err_code(&exec(&mut s, "history.rollback", json!({}))),
        "NoTransaction"
    );
}

#[test]
fn batch_references_and_atomicity() {
    let mut s = session();
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "brush.create_box", "args": { "min": [0,0,0], "max": [64,64,64] } },
            { "op": "brush.create_box", "args": { "min": [16,16,16], "max": [48,48,48] } },
            { "op": "csg.subtract", "args": { "targets": ["$0"], "cutters": ["$1"] } },
            { "op": "object.delete", "args": { "ids": ["$1"] } },
        ] }),
    );
    ok(&r);
    assert_eq!(r["result"]["results"].as_array().unwrap().len(), 4);
    assert_eq!(s.revision(), 1);
    assert_eq!(s.document().brushes.len(), 6);

    let before = s.document().clone();
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "brush.create_box", "args": { "min": [0,0,0], "max": [8,8,8] } },
            { "op": "object.delete", "args": { "ids": ["b9999"] } },
        ] }),
    );
    assert_eq!(err_code(&r), "NotFound");
    assert_eq!(r["index"], 1);
    assert_eq!(s.document(), &before);
    assert_eq!(s.revision(), 1);
}

#[test]
fn batch_reference_forms_and_rejections() {
    let mut s = session();
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [
            { "op": "brush.create_box", "args": { "min": [0,0,0], "max": [64,64,64] } },
            { "op": "brush.clip", "args": { "ids": ["$0"], "plane": [[32,0,0],[32,64,0],[32,0,64]], "keep": "both" } },
            { "op": "scene.get", "args": { "ids": ["$1.ids[0]"] } },
        ] }),
    );
    ok(&r);

    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [{ "op": "scene.get", "args": { "ids": ["$3"] } }] }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    assert_eq!(r["index"], 0);

    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [{ "op": "history.undo" }] }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
    let r = exec(
        &mut s,
        "batch",
        json!({ "commands": [{ "op": "batch", "args": { "commands": [] } }] }),
    );
    assert_eq!(err_code(&r), "InvalidArgument");
}

fn same_ignoring_next_id(actual: &Document, expected: &Document) {
    let mut actual = actual.clone();
    actual.set_next_id(expected.next_id());
    assert_eq!(actual.all_ids(), expected.all_ids(), "ids differ");
    for id in expected.all_ids() {
        assert!(actual.get(id) == expected.get(id), "{id} differs");
    }
    assert!(actual == *expected, "documents differ outside objects");
}

#[test]
fn undo_redo_restore_exact_documents() {
    let mut s = session();
    let mut snapshots: Vec<Document> = vec![s.document().clone()];

    let a = make_box(&mut s, [0, 0, 0], [64, 64, 64]);
    snapshots.push(s.document().clone());
    let b = make_box(&mut s, [16, 16, 16], [48, 48, 48]);
    snapshots.push(s.document().clone());
    ok(&exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [a], "cutters": [b] }),
    ));
    snapshots.push(s.document().clone());
    let any = s.document().brushes.keys().next().unwrap().to_string();
    ok(&exec(
        &mut s,
        "brush.clip",
        json!({ "ids": [any], "plane": [[0,0,32],[64,0,32],[0,64,32]], "keep": "both" }),
    ));
    snapshots.push(s.document().clone());
    let last = s.document().brushes.keys().last().unwrap().to_string();
    ok(&exec(
        &mut s,
        "csg.subtract",
        json!({ "targets": [b.clone()], "cutters": [last], "output": "mesh" }),
    ));
    snapshots.push(s.document().clone());

    for expected in snapshots.iter().rev().skip(1) {
        ok(&exec(&mut s, "history.undo", json!({})));
        same_ignoring_next_id(s.document(), expected);
    }
    assert_eq!(
        err_code(&exec(&mut s, "history.undo", json!({}))),
        "InvalidArgument"
    );
    for expected in snapshots.iter().skip(1) {
        ok(&exec(&mut s, "history.redo", json!({})));
        same_ignoring_next_id(s.document(), expected);
    }
    assert_eq!(
        err_code(&exec(&mut s, "history.redo", json!({}))),
        "InvalidArgument"
    );
}

#[test]
fn new_command_clears_redo() {
    let mut s = session();
    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    ok(&exec(&mut s, "history.undo", json!({})));
    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    assert_eq!(
        err_code(&exec(&mut s, "history.redo", json!({}))),
        "InvalidArgument"
    );
}

#[derive(Debug, Clone)]
enum Step {
    Box([i32; 3], [i32; 3]),
    Prism([i32; 3], u8, u8),
    Clip(usize, [i32; 9], u8),
    Subtract(usize, usize, bool),
    Delete(usize),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (
            prop::array::uniform3(-32i32..32),
            prop::array::uniform3(1i32..48)
        )
            .prop_map(|(a, b)| Step::Box(a, b)),
        (prop::array::uniform3(-32i32..32), 3u8..9, 4u8..40)
            .prop_map(|(c, n, r)| Step::Prism(c, n, r)),
        (any::<usize>(), prop::array::uniform9(-40i32..40), 0u8..3)
            .prop_map(|(i, p, k)| Step::Clip(i, p, k)),
        (any::<usize>(), any::<usize>(), any::<bool>())
            .prop_map(|(a, b, m)| Step::Subtract(a, b, m)),
        any::<usize>().prop_map(Step::Delete),
    ]
}

fn pick(s: &Session, i: usize) -> Option<String> {
    let ids = s.document().all_ids();
    let ids: Vec<_> = ids
        .into_iter()
        .filter(|id| {
            matches!(
                id,
                level_core::ObjectId::Brush(_) | level_core::ObjectId::Mesh(_)
            )
        })
        .collect();
    (!ids.is_empty()).then(|| ids[i % ids.len()].to_string())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]
    #[test]
    fn undo_all_then_redo_all(steps in prop::collection::vec(step(), 1..12)) {
        let mut s = session();
        let initial = s.document().clone();
        for st in steps {
            let (op, args) = match st {
                Step::Box(min, size) => ("brush.create_box", json!({
                    "min": min,
                    "max": [min[0] + size[0], min[1] + size[1], min[2] + size[2]],
                })),
                Step::Prism(c, n, r) => ("brush.create_prism", json!({
                    "center": c, "radius": r, "sides": n, "height": 32,
                })),
                Step::Clip(i, p, k) => {
                    let Some(id) = pick(&s, i) else { continue };
                    let keep = ["front", "back", "both"][k as usize];
                    ("brush.clip", json!({
                        "ids": [id],
                        "plane": [[p[0],p[1],p[2]],[p[3],p[4],p[5]],[p[6],p[7],p[8]]],
                        "keep": keep,
                    }))
                }
                Step::Subtract(i, j, mesh) => {
                    let (Some(a), Some(b)) = (pick(&s, i), pick(&s, j)) else { continue };
                    ("csg.subtract", json!({
                        "targets": [a], "cutters": [b],
                        "output": if mesh { "mesh" } else { "brush" },
                    }))
                }
                Step::Delete(i) => {
                    let Some(id) = pick(&s, i) else { continue };
                    ("object.delete", json!({ "ids": [id] }))
                }
            };
            let before = s.document().clone();
            let r = exec(&mut s, op, args);
            if !r["ok"].as_bool().unwrap() {
                prop_assert_eq!(s.document(), &before, "failed command changed the document: {}", r);
            }
            prop_assert!(level_core::hierarchy::check(s.document()).is_ok());
        }
        let last = s.document().clone();
        let revision = s.revision();
        let mut undone = 0;
        while exec(&mut s, "history.undo", json!({}))["ok"] == true {
            undone += 1;
        }
        prop_assert!(undone <= revision);
        prop_assert_eq!(s.document().all_ids(), initial.all_ids());
        prop_assert_eq!(&s.document().brushes, &initial.brushes);
        prop_assert_eq!(&s.document().meshes, &initial.meshes);
        for _ in 0..undone {
            ok(&exec(&mut s, "history.redo", json!({})));
        }
        prop_assert_eq!(&s.document().brushes, &last.brushes);
        prop_assert_eq!(&s.document().meshes, &last.meshes);
    }
}

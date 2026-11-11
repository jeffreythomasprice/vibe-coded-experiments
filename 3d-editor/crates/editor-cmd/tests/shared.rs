mod common;

use common::*;
use editor_cmd::{Actor, Request, Response, Session};
use serde_json::{Value, json};

fn as_actor(s: &mut Session, actor: &Actor, op: &str, args: Value, key: Option<&str>) -> Value {
    let mut r = Request::new(op, args);
    r.merge_key = key.map(String::from);
    s.execute_as(r, actor).to_value()
}

fn translate(s: &mut Session, actor: &Actor, id: &str, key: Option<&str>) -> Value {
    as_actor(
        s,
        actor,
        "transform.translate",
        json!({ "ids": [id], "delta": [16, 0, 0] }),
        key,
    )
}

fn undo_count(s: &mut Session) -> usize {
    let r = exec(s, "history.list", json!({}));
    ok(&r)["result"]["entries"].as_array().unwrap().len()
}

#[test]
fn merge_key_folds_consecutive_edits_into_one_undo_step() {
    let mut s = session();
    let id = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let gui = Actor::new(2, "gui");
    let before = undo_count(&mut s);
    for _ in 0..3 {
        ok(&translate(&mut s, &gui, &id, Some("drag-1")));
    }
    assert_eq!(undo_count(&mut s), before + 1);
    assert_eq!(s.revision(), 4);
    ok(&exec(&mut s, "history.undo", json!({})));
    let (min, _) = bounds(&mut s, &id);
    assert_eq!(min, [0.0, 0.0, 0.0]);
}

#[test]
fn different_key_or_conn_does_not_merge() {
    let mut s = session();
    let id = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let gui = Actor::new(2, "gui");
    let other = Actor::new(3, "mcp");
    let base = undo_count(&mut s);
    translate(&mut s, &gui, &id, Some("a"));
    translate(&mut s, &gui, &id, Some("b"));
    translate(&mut s, &other, &id, Some("b"));
    translate(&mut s, &gui, &id, None);
    translate(&mut s, &gui, &id, None);
    assert_eq!(undo_count(&mut s), base + 5);
}

#[test]
fn merge_that_nets_to_nothing_pops_the_entry() {
    let mut s = session();
    let id = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let gui = Actor::new(2, "gui");
    let base = undo_count(&mut s);
    translate(&mut s, &gui, &id, Some("drag"));
    as_actor(
        &mut s,
        &gui,
        "transform.translate",
        json!({ "ids": [id], "delta": [-16, 0, 0] }),
        Some("drag"),
    );
    assert_eq!(undo_count(&mut s), base);
}

#[test]
fn other_connections_are_busy_during_a_transaction() {
    let mut s = session();
    let id = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let agent = Actor::new(2, "mcp");
    let gui = Actor::new(3, "gui");
    ok(&as_actor(
        &mut s,
        &agent,
        "history.begin",
        json!({ "label": "build" }),
        None,
    ));
    let busy = translate(&mut s, &gui, &id, None);
    assert_eq!(err_code(&busy), "SessionBusy");
    assert_eq!(
        err_code(&as_actor(&mut s, &gui, "history.undo", json!({}), None)),
        "SessionBusy"
    );
    assert_eq!(
        err_code(&as_actor(
            &mut s,
            &gui,
            "file.open",
            json!({"path": "x"}),
            None
        )),
        "SessionBusy"
    );
    ok(&as_actor(&mut s, &gui, "scene.summary", json!({}), None));
    ok(&as_actor(
        &mut s,
        &gui,
        "select.set",
        json!({ "ids": [id] }),
        None,
    ));
    ok(&translate(&mut s, &agent, &id, None));
    ok(&as_actor(&mut s, &agent, "history.commit", json!({}), None));
    ok(&translate(&mut s, &gui, &id, None));
}

#[test]
fn abandon_rolls_back_only_the_owners_transaction() {
    let mut s = session();
    let id = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let agent = Actor::new(2, "mcp");
    ok(&as_actor(&mut s, &agent, "history.begin", json!({}), None));
    ok(&translate(&mut s, &agent, &id, None));
    assert!(!s.abandon(9));
    assert!(s.abandon(2));
    let (min, _) = bounds(&mut s, &id);
    assert_eq!(min, [0.0, 0.0, 0.0]);
    assert!(s.history_state().transaction.is_none());
    assert!(!s.abandon(2));
}

#[test]
fn feed_reports_edits_rollback_undo_and_replacement() {
    let mut s = session();
    s.take_feed();
    let id = make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    let feed = s.take_feed();
    assert_eq!(feed.ids.len(), 1);
    assert!(feed.history);
    assert!(feed.file);

    assert!(s.take_feed().is_empty());

    let dry = s
        .execute_value(
            json!({ "op": "transform.translate", "args": { "ids": [id], "delta": [1, 0, 0] }, "dry_run": true }),
            "t",
        )
        .to_value();
    ok(&dry);
    assert!(s.take_feed().ids.is_empty());

    let agent = Actor::new(2, "mcp");
    as_actor(&mut s, &agent, "history.begin", json!({}), None);
    translate(&mut s, &agent, &id, None);
    s.take_feed();
    let rolled = as_actor(&mut s, &agent, "history.rollback", json!({}), None);
    assert_eq!(rolled["changes"]["modified"].as_array().unwrap().len(), 1);
    assert_eq!(s.take_feed().ids.len(), 1);

    ok(&exec(&mut s, "history.undo", json!({})));
    assert_eq!(s.take_feed().ids.len(), 1);

    ok(&exec(&mut s, "file.new", json!({ "discard": true })));
    let feed = s.take_feed();
    assert!(feed.replaced);

    make_box(&mut s, [0, 0, 0], [8, 8, 8]);
    s.take_feed();
    ok(&exec(&mut s, "select.all", json!({})));
    assert!(s.take_feed().selection);
}

#[test]
fn server_ops_are_rejected_by_a_bare_session() {
    let mut s = session();
    let r: Response = s.execute(Request::new("session.list", json!({})), "t");
    assert_eq!(err_code(&r.to_value()), "InvalidArgument");
}

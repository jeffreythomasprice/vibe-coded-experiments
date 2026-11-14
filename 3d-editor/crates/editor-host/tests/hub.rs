use editor_cmd::{Request, Response};
use editor_host::{ConnId, Handled, Hub, Policy};
use editor_proto::{PROTOCOL, ServerMsg};
use serde_json::{Value, json};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("editor-hub-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn connect(hub: &mut Hub, name: &str) -> ConnId {
    hub.connect(name, PROTOCOL, Policy::allow_all()).unwrap()
}

fn run(hub: &mut Hub, conn: ConnId, op: &str, args: Value) -> (Value, Vec<(ConnId, ServerMsg)>) {
    let Handled { events, reply } = hub.handle(conn, Request::new(op, args));
    (reply.response.to_value(), events)
}

fn attach_new(hub: &mut Hub, conn: ConnId, path: &std::path::Path) -> Value {
    let (v, _) = run(
        hub,
        conn,
        "session.attach",
        json!({ "path": path.display().to_string(), "create": true }),
    );
    assert_eq!(v["ok"], true, "{v}");
    v
}

fn make_box(hub: &mut Hub, conn: ConnId) -> (Value, Vec<(ConnId, ServerMsg)>) {
    run(
        hub,
        conn,
        "brush.create_box",
        json!({ "min": [0, 0, 0], "max": [8, 8, 8] }),
    )
}

#[test]
fn requests_need_a_session() {
    let mut hub = Hub::new();
    let a = connect(&mut hub, "cli");
    let (v, _) = run(&mut hub, a, "scene.summary", json!({}));
    assert_eq!(v["code"], "InvalidArgument");
    assert!(v["hint"].as_str().unwrap().contains("session.attach"));
}

#[test]
fn protocol_mismatch_is_refused() {
    let mut hub = Hub::new();
    let e = hub
        .connect("cli", PROTOCOL + 1, Policy::allow_all())
        .unwrap_err();
    assert_eq!(e.code(), "Protocol");
}

#[test]
fn edits_reach_every_attached_connection() {
    let dir = temp_dir("fanout");
    let path = dir.join("a.scene.toml");
    let mut hub = Hub::new();
    let gui = connect(&mut hub, "gui");
    let agent = connect(&mut hub, "mcp");
    attach_new(&mut hub, gui, &path);
    let joined = attach_new(&mut hub, agent, &path);
    assert_eq!(joined["result"]["created"], false);

    let (v, events) = make_box(&mut hub, agent);
    assert_eq!(v["ok"], true, "{v}");
    let targets: Vec<ConnId> = events.iter().map(|(c, _)| *c).collect();
    assert!(targets.contains(&gui) && targets.contains(&agent));
    let (_, msg) = events.iter().find(|(c, _)| *c == gui).unwrap();
    let ServerMsg::Changed(changed) = msg else {
        panic!("expected a changed event, got {msg:?}")
    };
    assert_eq!(changed.upsert.len(), 1);
    assert!(changed.deleted.is_empty());
    assert!(changed.file.as_ref().unwrap().dirty);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn attach_without_args_joins_the_focused_session() {
    let dir = temp_dir("focus");
    let path = dir.join("a.scene.toml");
    let mut hub = Hub::new();
    let gui = connect(&mut hub, "gui");
    attach_new(&mut hub, gui, &path);
    let agent = connect(&mut hub, "mcp");
    let (v, _) = run(&mut hub, agent, "session.attach", json!({}));
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["result"]["created"], false);
    assert_eq!(hub.session_count(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn busy_and_disconnect_rollback() {
    let dir = temp_dir("busy");
    let path = dir.join("a.scene.toml");
    let mut hub = Hub::new();
    let gui = connect(&mut hub, "gui");
    let agent = connect(&mut hub, "mcp");
    attach_new(&mut hub, gui, &path);
    attach_new(&mut hub, agent, &path);

    let (v, _) = run(
        &mut hub,
        agent,
        "history.begin",
        json!({ "label": "build" }),
    );
    assert_eq!(v["ok"], true);
    make_box(&mut hub, agent);
    let (busy, _) = make_box(&mut hub, gui);
    assert_eq!(busy["code"], "SessionBusy");

    let events = hub.disconnect(agent);
    let (_, msg) = events.iter().find(|(c, _)| *c == gui).unwrap();
    let ServerMsg::Changed(changed) = msg else {
        panic!("expected a changed event")
    };
    assert_eq!(changed.deleted.len(), 1);
    assert!(changed.history.as_ref().unwrap().transaction.is_none());

    let (ok, _) = make_box(&mut hub, gui);
    assert_eq!(ok["ok"], true, "{ok}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn merge_keys_do_not_fold_across_connections() {
    let dir = temp_dir("merge");
    let path = dir.join("a.scene.toml");
    let mut hub = Hub::new();
    let gui = connect(&mut hub, "gui");
    let agent = connect(&mut hub, "mcp");
    attach_new(&mut hub, gui, &path);
    attach_new(&mut hub, agent, &path);
    let (v, _) = make_box(&mut hub, gui);
    let id = v["result"]["id"].as_str().unwrap().to_string();

    let step = |hub: &mut Hub, conn| {
        let mut r = Request::new(
            "transform.translate",
            json!({ "ids": [id], "delta": [8, 0, 0] }),
        );
        r.merge_key = Some("drag".into());
        hub.handle(conn, r).reply.response.to_value()
    };
    step(&mut hub, gui);
    step(&mut hub, gui);
    step(&mut hub, agent);
    let (list, _) = run(&mut hub, gui, "history.list", json!({}));
    assert_eq!(list["result"]["entries"].as_array().unwrap().len(), 3);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn files_open_in_another_session_are_in_use() {
    let dir = temp_dir("inuse");
    let a = dir.join("a.scene.toml");
    let b = dir.join("b.scene.toml");
    let mut hub = Hub::new();
    let one = connect(&mut hub, "cli");
    let two = connect(&mut hub, "mcp");
    attach_new(&mut hub, one, &a);
    attach_new(&mut hub, two, &b);
    let (v, _) = run(
        &mut hub,
        two,
        "file.open",
        json!({ "path": a.display().to_string(), "discard": true }),
    );
    assert_eq!(v["code"], "FileInUse", "{v}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn clean_sessions_close_with_their_last_client_but_dirty_ones_stay() {
    let dir = temp_dir("lifecycle");
    let path = dir.join("a.scene.toml");
    let mut hub = Hub::new();
    let c = connect(&mut hub, "cli");
    attach_new(&mut hub, c, &path);
    hub.disconnect(c);
    assert_eq!(hub.session_count(), 0);

    let c = connect(&mut hub, "cli");
    attach_new(&mut hub, c, &path);
    make_box(&mut hub, c);
    hub.disconnect(c);
    assert_eq!(hub.session_count(), 1);
    assert_eq!(hub.dirty_sessions(), 1);

    let c = connect(&mut hub, "cli");
    let (v, _) = run(&mut hub, c, "session.attach", json!({}));
    assert_eq!(v["result"]["dirty"], true);
    let (saved, _) = run(&mut hub, c, "file.save", json!({}));
    assert_eq!(saved["ok"], true);
    hub.disconnect(c);
    assert_eq!(hub.session_count(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn policy_is_per_connection() {
    let dir = temp_dir("policy");
    let path = dir.join("a.scene.toml");
    let mut hub = Hub::new();
    let rw = connect(&mut hub, "gui");
    let ro = hub.connect("mcp", PROTOCOL, Policy::read_only()).unwrap();
    attach_new(&mut hub, rw, &path);
    let (v, _) = run(&mut hub, ro, "session.attach", json!({}));
    assert_eq!(v["ok"], true);
    let (denied, _) = make_box(&mut hub, ro);
    assert_eq!(denied["code"], "NotPermitted");
    let (create, _) = run(
        &mut hub,
        ro,
        "session.attach",
        json!({ "path": dir.join("new.scene.toml").display().to_string(), "create": true }),
    );
    assert_eq!(create["code"], "NotPermitted");
    let _: Response = hub
        .handle(rw, Request::new("scene.summary", json!({})))
        .reply
        .response;
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn control_stop_refuses_dirty_sessions_without_force() {
    let dir = temp_dir("stop");
    let path = dir.join("a.scene.toml");
    let mut hub = Hub::new();
    let c = connect(&mut hub, "cli");
    attach_new(&mut hub, c, &path);
    make_box(&mut hub, c);
    let (v, stop) = hub.control(c, editor_proto::ControlAction::Stop, false);
    assert_eq!(v.to_value()["code"], "UnsavedChanges");
    assert!(!stop);
    let (_, stop) = hub.control(c, editor_proto::ControlAction::Stop, true);
    assert!(stop);
    let _ = std::fs::remove_dir_all(dir);
}

fn untitled(hub: &mut Hub, conn: ConnId) {
    let (v, _) = run(hub, conn, "session.attach", json!({}));
    assert_eq!(v["ok"], true, "{v}");
}

#[test]
fn screenshot_path_writes_the_png_and_needs_mutate() {
    let dir = temp_dir("shot");
    let path = dir.join("shot.png");
    let shot = |path: &std::path::Path| {
        Request::new(
            "view.screenshot",
            json!({ "width": 64, "height": 64, "path": path.display().to_string() }),
        )
    };

    let mut hub = Hub::new();
    let ro = hub.connect("mcp", PROTOCOL, Policy::read_only()).unwrap();
    untitled(&mut hub, ro);
    let denied = hub.handle(ro, shot(&path)).reply.response.to_value();
    assert_eq!(denied["code"], "NotPermitted");
    assert!(!path.exists());

    let rw = hub
        .connect("cli", PROTOCOL, Policy::new(true, false))
        .unwrap();
    untitled(&mut hub, rw);
    let reply = hub.handle(rw, shot(&path)).reply;
    let value = reply.response.to_value();
    if value["code"] == "Render" {
        eprintln!("skipping: no graphics adapter ({})", value["message"]);
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    assert_eq!(value["ok"], true, "{value}");
    assert_eq!(value["result"]["path"], path.display().to_string());
    assert!(std::fs::read(&path).unwrap().starts_with(b"\x89PNG"));
    assert!(reply.png.is_some());

    let inline = hub
        .handle(
            ro,
            Request::new("view.screenshot", json!({ "width": 64, "height": 64 })),
        )
        .reply;
    assert!(inline.response.is_ok() && inline.png.is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn consume_cutters_needs_destructive_even_in_a_batch() {
    let mut hub = Hub::new();
    let c = hub
        .connect("cli", PROTOCOL, Policy::new(true, false))
        .unwrap();
    untitled(&mut hub, c);
    let (v, _) = run(
        &mut hub,
        c,
        "batch",
        json!({ "commands": [
            { "op": "csg_subtract", "args": { "targets": ["b1"], "cutters": ["b2"], "consume_cutters": true } },
        ]}),
    );
    assert_eq!(v["code"], "NotPermitted");
    assert!(v["message"].as_str().unwrap().contains("destructive"));
}

#[test]
fn relative_paths_resolve_against_the_clients_directory() {
    let dir = temp_dir("cwd");
    let mut hub = Hub::new();
    let c = connect(&mut hub, "cli");
    hub.set_cwd(c, Some(dir.clone()));
    let (v, _) = run(
        &mut hub,
        c,
        "session.attach",
        json!({ "path": "sub/rel.scene.toml", "create": true }),
    );
    assert_eq!(v["ok"], true, "{v}");
    assert!(dir.join("sub/rel.scene.toml").exists());
    let _ = std::fs::remove_dir_all(dir);
}

fn assert_conforms(op: &str, value: &Value) {
    let info = editor_cmd::OpInfo::find(op).unwrap();
    let schema = serde_json::to_value((info.response_schema)()).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{op}: {errors:?}\n{value}");
}

#[test]
fn server_and_client_ops_conform_to_their_result_schemas() {
    let dir = temp_dir("schemas");
    let mut hub = Hub::new();
    let c = connect(&mut hub, "cli");

    let (v, _) = run(
        &mut hub,
        c,
        "session.attach",
        json!({ "path": dir.join("a.scene.toml").display().to_string(), "create": true }),
    );
    assert_conforms("session.attach", &v);
    for op in ["session.list", "session.info"] {
        let (v, _) = run(&mut hub, c, op, json!({}));
        assert_eq!(v["ok"], true, "{v}");
        assert_conforms(op, &v);
    }

    let reply = hub
        .handle(
            c,
            Request::new(
                "view.screenshot",
                json!({ "width": 64, "height": 64, "overlays": ["labels"] }),
            ),
        )
        .reply;
    let value = reply.response.to_value();
    if value["code"] == "Render" {
        eprintln!(
            "skipping screenshot: no graphics adapter ({})",
            value["message"]
        );
    } else {
        assert_conforms("view.screenshot", &value);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_clipboard_is_shared_across_connections_and_sessions() {
    let dir = temp_dir("clipboard");
    let mut hub = Hub::new();
    let gui = connect(&mut hub, "gui");
    let agent = connect(&mut hub, "mcp");
    attach_new(&mut hub, gui, &dir.join("a.scene.toml"));
    attach_new(&mut hub, agent, &dir.join("b.scene.toml"));

    let (empty, _) = run(&mut hub, agent, "clipboard.get", json!({}));
    assert_eq!(empty["result"]["text"], Value::Null);
    assert_eq!(empty["result"]["seq"], 0);
    let (v, _) = run(&mut hub, agent, "clipboard.paste", json!({}));
    assert_eq!(v["field"], "text");

    let (made, _) = make_box(&mut hub, gui);
    let id = made["result"]["id"].as_str().unwrap().to_string();
    let (copied, _) = run(&mut hub, gui, "clipboard.copy", json!({ "ids": [id] }));
    assert_eq!(copied["ok"], true, "{copied}");
    assert_eq!(copied["result"]["count"], 1);
    assert_eq!(copied["result"]["seq"], 1);

    let (pasted, _) = run(
        &mut hub,
        agent,
        "clipboard.paste",
        json!({ "at": [100, 0, 0] }),
    );
    assert_eq!(pasted["ok"], true, "{pasted}");
    let (summary, _) = run(&mut hub, agent, "scene.summary", json!({}));
    assert_eq!(summary["result"]["counts"]["brush"], 1, "{summary}");

    let (batched, _) = run(
        &mut hub,
        agent,
        "batch",
        json!({ "commands": [
            { "op": "clipboard_paste", "args": { "offset": [0, 64, 0] } },
            { "op": "clipboard.paste" },
        ] }),
    );
    assert_eq!(batched["ok"], true, "{batched}");

    let (explicit, _) = run(
        &mut hub,
        agent,
        "clipboard.paste",
        json!({ "text": "not a scene [" }),
    );
    assert_eq!(explicit["code"], "InvalidArgument");

    let (none, _) = run(&mut hub, agent, "clipboard.copy", json!({ "ids": [] }));
    assert_eq!(none["code"], "InvalidArgument");
}

mod recovery {
    use std::time::{Duration, Instant};

    use editor_host::RecoverySettings;

    use super::*;

    fn settings(
        dir: &std::path::Path,
        autosave: u64,
        backup: Option<u64>,
        keep: usize,
    ) -> RecoverySettings {
        RecoverySettings {
            dir: dir.join("state"),
            autosave: Some(Duration::from_secs(autosave)),
            backup: backup.map(Duration::from_secs),
            keep,
        }
    }

    fn files(dir: &std::path::Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(dir)
            .map(|d| {
                d.filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }

    fn tick(hub: &mut Hub, ahead: u64, force: bool) {
        hub.recovery_due(Instant::now() + Duration::from_secs(ahead), force)
            .run()
            .unwrap();
    }

    #[test]
    fn dirty_sessions_are_autosaved_after_the_interval_and_cleaned_up_on_save() {
        let dir = temp_dir("autosave");
        let state = dir.join("state").join("autosave");
        let path = dir.join("a.scene.toml");
        let mut hub = Hub::with_recovery(settings(&dir, 5, None, 3));
        let conn = connect(&mut hub, "mcp");
        attach_new(&mut hub, conn, &path);

        tick(&mut hub, 60, false);
        assert!(files(&state).is_empty(), "clean sessions are not autosaved");

        make_box(&mut hub, conn);
        tick(&mut hub, 1, false);
        assert!(files(&state).is_empty(), "before the interval");
        tick(&mut hub, 60, false);
        assert_eq!(files(&state).len(), 2, "{:?}", files(&state));

        let (list, _) = run(&mut hub, conn, "recovery.list", json!({}));
        let entry = &list["result"]["entries"][0];
        assert_eq!(entry["live"], true, "{list}");
        assert_eq!(entry["source_path"], path.display().to_string());
        assert_eq!(entry["revision"], 1);

        let before = files(&state);
        tick(&mut hub, 120, false);
        assert_eq!(files(&state), before, "nothing changed, nothing rewritten");

        run(&mut hub, conn, "file.save", json!({}));
        tick(&mut hub, 120, false);
        assert!(files(&state).is_empty(), "{:?}", files(&state));
    }

    #[test]
    fn open_transactions_are_not_autosaved() {
        let dir = temp_dir("autosave-txn");
        let mut hub = Hub::with_recovery(settings(&dir, 1, None, 3));
        let conn = connect(&mut hub, "mcp");
        attach_new(&mut hub, conn, &dir.join("a.scene.toml"));
        run(&mut hub, conn, "history.begin", json!({}));
        make_box(&mut hub, conn);
        tick(&mut hub, 60, false);
        assert!(files(&dir.join("state").join("autosave")).is_empty());
        run(&mut hub, conn, "history.commit", json!({}));
        tick(&mut hub, 60, false);
        assert_eq!(files(&dir.join("state").join("autosave")).len(), 2);
    }

    #[test]
    fn closing_a_clean_session_removes_its_autosave() {
        let dir = temp_dir("autosave-close");
        let state = dir.join("state").join("autosave");
        let mut hub = Hub::with_recovery(settings(&dir, 1, None, 3));
        let conn = connect(&mut hub, "mcp");
        attach_new(&mut hub, conn, &dir.join("a.scene.toml"));
        make_box(&mut hub, conn);
        tick(&mut hub, 60, false);
        assert_eq!(files(&state).len(), 2);
        run(&mut hub, conn, "file.save", json!({}));
        hub.disconnect(conn);
        tick(&mut hub, 120, false);
        assert!(files(&state).is_empty());
    }

    #[test]
    fn backups_are_taken_per_interval_and_pruned() {
        let dir = temp_dir("backups");
        let path = dir.join("a.scene.toml");
        let mut hub = Hub::with_recovery(settings(&dir, 1000, Some(10), 2));
        let conn = connect(&mut hub, "mcp");
        attach_new(&mut hub, conn, &path);
        let backups = dir.join("state").join("backups");

        tick(&mut hub, 60, false);
        assert!(
            files(&backups).is_empty(),
            "an untouched document is not backed up"
        );

        for round in 1..=4u64 {
            make_box(&mut hub, conn);
            tick(&mut hub, 100 * round, false);
        }
        let key = files(&backups);
        assert_eq!(key.len(), 1);
        assert_eq!(files(&backups.join(&key[0])).len(), 2);
        assert!(
            files(&backups.join(&key[0]))
                .iter()
                .all(|f| f.ends_with(".scene.toml"))
        );
    }

    #[test]
    fn a_crashed_sessions_autosave_can_be_restored_or_discarded() {
        let dir = temp_dir("restore");
        let state = dir.join("state").join("autosave");
        let path = dir.join("a.scene.toml");
        let cfg = settings(&dir, 1, None, 3);

        let mut first = Hub::with_recovery(cfg.clone());
        let conn = connect(&mut first, "mcp");
        attach_new(&mut first, conn, &path);
        make_box(&mut first, conn);
        make_box(&mut first, conn);
        tick(&mut first, 60, false);
        assert_eq!(files(&state).len(), 2);
        drop(first);

        let mut hub = Hub::with_recovery(cfg);
        let conn = connect(&mut hub, "gui");
        attach_new(&mut hub, conn, &path);
        let (list, _) = run(&mut hub, conn, "recovery.list", json!({}));
        let entry = list["result"]["entries"][0].clone();
        assert_eq!(entry["live"], false, "{list}");
        assert_eq!(entry["newer_than_source"], true);
        let id = entry["id"].as_str().unwrap().to_string();

        let (bad, _) = run(&mut hub, conn, "recovery.restore", json!({ "id": "../x" }));
        assert_eq!(bad["code"], "InvalidArgument");
        let (missing, _) = run(&mut hub, conn, "recovery.restore", json!({ "id": "nope" }));
        assert_eq!(missing["code"], "NotFound");

        let (restored, events) = run(&mut hub, conn, "recovery.restore", json!({ "id": id }));
        assert_eq!(restored["ok"], true, "{restored}");
        assert_eq!(restored["result"]["path"], path.display().to_string());
        assert!(
            events
                .iter()
                .any(|(_, m)| matches!(m, ServerMsg::Snapshot(_)))
        );
        let (info, _) = run(&mut hub, conn, "file.info", json!({}));
        assert_eq!(info["result"]["dirty"], true);
        let (summary, _) = run(&mut hub, conn, "scene.summary", json!({}));
        assert_eq!(summary["result"]["counts"]["brush"], 2);

        let (again, _) = run(&mut hub, conn, "recovery.restore", json!({ "id": id }));
        assert_eq!(
            again["code"], "InvalidArgument",
            "owned by this session now: {again}"
        );

        tick(&mut hub, 60, false);
        assert_eq!(
            files(&state).len(),
            2,
            "its own autosave replaced the old one"
        );
        let (list, _) = run(&mut hub, conn, "recovery.list", json!({}));
        assert_eq!(list["result"]["entries"].as_array().unwrap().len(), 1);
        assert_eq!(list["result"]["entries"][0]["live"], true);
        assert_ne!(list["result"]["entries"][0]["id"], id.as_str());
    }

    #[test]
    fn restore_needs_discard_over_unsaved_work_and_discard_deletes() {
        let dir = temp_dir("restore-guard");
        let state = dir.join("state").join("autosave");
        let path = dir.join("a.scene.toml");
        let cfg = settings(&dir, 1, None, 3);
        let mut first = Hub::with_recovery(cfg.clone());
        let conn = connect(&mut first, "mcp");
        attach_new(&mut first, conn, &path);
        make_box(&mut first, conn);
        tick(&mut first, 60, false);
        drop(first);

        let mut hub = Hub::with_recovery(cfg);
        let conn = connect(&mut hub, "mcp");
        attach_new(&mut hub, conn, &path);
        make_box(&mut hub, conn);
        let (list, _) = run(&mut hub, conn, "recovery.list", json!({}));
        let id = list["result"]["entries"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let (refused, _) = run(&mut hub, conn, "recovery.restore", json!({ "id": id }));
        assert_eq!(refused["code"], "UnsavedChanges", "{refused}");
        let (forced, _) = run(
            &mut hub,
            conn,
            "recovery.restore",
            json!({ "id": id, "discard": true }),
        );
        assert_eq!(forced["ok"], true, "{forced}");

        let mut other = Hub::with_recovery(settings(&dir, 1, None, 3));
        let conn = connect(&mut other, "mcp");
        let (gone, _) = run(&mut other, conn, "recovery.discard", json!({ "id": id }));
        assert_eq!(gone["ok"], true, "{gone}");
        let (list, _) = run(&mut other, conn, "recovery.list", json!({}));
        assert!(list["result"]["entries"].as_array().unwrap().is_empty());
        let (again, _) = run(&mut other, conn, "recovery.discard", json!({ "id": id }));
        assert_eq!(again["code"], "NotFound");
        assert!(files(&state).is_empty(), "{:?}", files(&state));
    }

    #[test]
    fn clipboard_and_recovery_results_conform_to_their_schemas() {
        let dir = temp_dir("conform");
        let path = dir.join("a.scene.toml");
        let cfg = settings(&dir, 1, None, 3);
        let mut first = Hub::with_recovery(cfg.clone());
        let c = connect(&mut first, "cli");
        attach_new(&mut first, c, &path);
        let (made, _) = make_box(&mut first, c);
        let id = made["result"]["id"].as_str().unwrap().to_string();
        let (v, _) = run(&mut first, c, "clipboard.get", json!({}));
        assert_conforms("clipboard.get", &v);
        let (v, _) = run(&mut first, c, "clipboard.copy", json!({ "ids": [id] }));
        assert_conforms("clipboard.copy", &v);
        let (v, _) = run(&mut first, c, "recovery.list", json!({}));
        assert_conforms("recovery.list", &v);
        tick(&mut first, 60, false);
        drop(first);

        let mut hub = Hub::with_recovery(cfg);
        let c = connect(&mut hub, "cli");
        attach_new(&mut hub, c, &path);
        let (list, _) = run(&mut hub, c, "recovery.list", json!({}));
        assert_conforms("recovery.list", &list);
        let id = list["result"]["entries"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let (v, _) = run(&mut hub, c, "recovery.restore", json!({ "id": id }));
        assert_conforms("recovery.restore", &v);
        let mut other = Hub::with_recovery(settings(&dir, 1, None, 3));
        let c = connect(&mut other, "cli");
        let (v, _) = run(&mut other, c, "recovery.discard", json!({ "id": id }));
        assert_conforms("recovery.discard", &v);
    }

    #[test]
    fn a_forced_flush_writes_even_before_the_interval() {
        let dir = temp_dir("flush");
        let mut hub = Hub::with_recovery(settings(&dir, 3600, None, 3));
        let conn = connect(&mut hub, "mcp");
        attach_new(&mut hub, conn, &dir.join("a.scene.toml"));
        make_box(&mut hub, conn);
        tick(&mut hub, 1, false);
        assert!(files(&dir.join("state").join("autosave")).is_empty());
        tick(&mut hub, 1, true);
        assert_eq!(files(&dir.join("state").join("autosave")).len(), 2);
    }

    #[test]
    fn recovery_ops_need_a_configured_directory() {
        let mut hub = Hub::new();
        let conn = connect(&mut hub, "mcp");
        let (v, _) = run(&mut hub, conn, "recovery.list", json!({}));
        assert_eq!(v["code"], "InvalidArgument");
    }
}

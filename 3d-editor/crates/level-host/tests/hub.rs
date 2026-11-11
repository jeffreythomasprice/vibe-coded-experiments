use level_cmd::{Request, Response};
use level_host::{ConnId, Handled, Hub, Policy};
use level_proto::{PROTOCOL, ServerMsg};
use serde_json::{Value, json};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("level-hub-{tag}-{}", std::process::id()));
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
    let e = hub.connect("cli", PROTOCOL + 1, Policy::allow_all()).unwrap_err();
    assert_eq!(e.code(), "Protocol");
}

#[test]
fn edits_reach_every_attached_connection() {
    let dir = temp_dir("fanout");
    let path = dir.join("a.level.toml");
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
    let path = dir.join("a.level.toml");
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
    let path = dir.join("a.level.toml");
    let mut hub = Hub::new();
    let gui = connect(&mut hub, "gui");
    let agent = connect(&mut hub, "mcp");
    attach_new(&mut hub, gui, &path);
    attach_new(&mut hub, agent, &path);

    let (v, _) = run(&mut hub, agent, "history.begin", json!({ "label": "build" }));
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
    let path = dir.join("a.level.toml");
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
    let a = dir.join("a.level.toml");
    let b = dir.join("b.level.toml");
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
    let path = dir.join("a.level.toml");
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
    let path = dir.join("a.level.toml");
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
        json!({ "path": dir.join("new.level.toml").display().to_string(), "create": true }),
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
    let path = dir.join("a.level.toml");
    let mut hub = Hub::new();
    let c = connect(&mut hub, "cli");
    attach_new(&mut hub, c, &path);
    make_box(&mut hub, c);
    let (v, stop) = hub.control(c, level_proto::ControlAction::Stop, false);
    assert_eq!(v.to_value()["code"], "UnsavedChanges");
    assert!(!stop);
    let (_, stop) = hub.control(c, level_proto::ControlAction::Stop, true);
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

    let rw = hub.connect("cli", PROTOCOL, Policy::new(true, false)).unwrap();
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
    let c = hub.connect("cli", PROTOCOL, Policy::new(true, false)).unwrap();
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
        json!({ "path": "sub/rel.level.toml", "create": true }),
    );
    assert_eq!(v["ok"], true, "{v}");
    assert!(dir.join("sub/rel.level.toml").exists());
    let _ = std::fs::remove_dir_all(dir);
}

fn assert_conforms(op: &str, value: &Value) {
    let info = level_cmd::OpInfo::find(op).unwrap();
    let schema = serde_json::to_value((info.response_schema)()).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator.iter_errors(value).map(|e| e.to_string()).collect();
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
        json!({ "path": dir.join("a.level.toml").display().to_string(), "create": true }),
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
            Request::new("view.screenshot", json!({ "width": 64, "height": 64, "overlays": ["labels"] })),
        )
        .reply;
    let value = reply.response.to_value();
    if value["code"] == "Render" {
        eprintln!("skipping screenshot: no graphics adapter ({})", value["message"]);
    } else {
        assert_conforms("view.screenshot", &value);
    }
    let _ = std::fs::remove_dir_all(dir);
}

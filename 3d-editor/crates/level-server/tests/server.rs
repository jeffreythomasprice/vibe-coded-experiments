use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use level_proto::{Client, Config, ServerEvent};
use serde_json::{Value, json};

struct Env {
    dir: PathBuf,
    config: Config,
}

impl Env {
    fn new(tag: &str, idle_secs: u64) -> Self {
        let dir = std::env::temp_dir().join(format!("level-server-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.toml");
        std::fs::write(
            &config_path,
            format!(
                "socket = {:?}\n[server]\nidle_exit_secs = {idle_secs}\nbinary = {:?}\n",
                dir.join("s.sock").display().to_string(),
                env!("CARGO_BIN_EXE_level-server"),
            ),
        )
        .unwrap();
        let config = Config::load_from(&config_path).unwrap();
        Self { dir, config }
    }

    fn client(&self, name: &str) -> Client {
        Client::connect(&self.config, name, true, true).unwrap()
    }

    fn level(&self) -> String {
        self.dir.join("a.level.toml").display().to_string()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = Client::connect_with(&self.config, "cleanup", true, true, false, None)
            .and_then(|c| c.control(level_proto::ControlAction::Stop, true));
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn call(c: &Client, op: &str, args: Value) -> Value {
    c.request_value(json!({ "op": op, "args": args })).unwrap().value()
}

fn attach(c: &Client, path: &str) -> Value {
    call(c, "session.attach", json!({ "path": path, "create": true }))
}

fn next_event(c: &Client) -> ServerEvent {
    c.next_event(Duration::from_secs(5)).unwrap()
}

#[test]
fn clients_share_a_session_and_see_each_others_edits() {
    let env = Env::new("share", 30);
    let gui = env.client("gui");
    let agent = env.client("mcp");
    assert_eq!(attach(&gui, &env.level())["ok"], true);
    assert!(matches!(next_event(&gui), ServerEvent::Snapshot(_)));
    let joined = attach(&agent, &env.level());
    assert_eq!(joined["result"]["created"], false, "{joined}");
    assert!(matches!(next_event(&agent), ServerEvent::Snapshot(_)));

    let made = call(&agent, "brush.create_box", json!({ "min": [0,0,0], "max": [8,8,8] }));
    assert_eq!(made["ok"], true, "{made}");
    let id = made["result"]["id"].as_str().unwrap().to_string();
    let ServerEvent::Changed(changed) = next_event(&gui) else {
        panic!("expected a changed event")
    };
    assert_eq!(changed.upsert.len(), 1);
    assert_eq!(changed.upsert[0].id().to_string(), id);

    call(&agent, "history.begin", json!({ "label": "build" }));
    let busy = call(&gui, "transform.translate", json!({ "ids": [id], "delta": [8,0,0] }));
    assert_eq!(busy["code"], "SessionBusy", "{busy}");
    drop(agent);

    let mut rolled_back = false;
    while let Some(ev) = gui.next_event(Duration::from_secs(5)) {
        if let ServerEvent::Changed(c) = ev
            && c.history.as_ref().is_some_and(|h| h.transaction.is_none())
        {
            rolled_back = true;
            break;
        }
    }
    assert!(rolled_back, "the dropped client's transaction was not abandoned");
    let ok = call(&gui, "transform.translate", json!({ "ids": [id], "delta": [8,0,0] }));
    assert_eq!(ok["ok"], true, "{ok}");
}

#[test]
fn autospawn_starts_the_server_and_idle_exit_removes_the_socket() {
    let env = Env::new("idle", 1);
    let socket: &Path = &env.config.socket_path();
    assert!(!socket.exists());
    let c = env.client("cli");
    assert!(socket.exists());
    assert_eq!(call(&c, "session.list", json!({}))["ok"], true);
    drop(c);
    let deadline = Instant::now() + Duration::from_secs(8);
    while socket.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!socket.exists(), "server did not exit when idle");
}

#[test]
fn dirty_sessions_keep_the_server_alive() {
    let env = Env::new("dirty", 1);
    let c = env.client("cli");
    assert_eq!(attach(&c, &env.level())["ok"], true);
    call(&c, "brush.create_box", json!({ "min": [0,0,0], "max": [8,8,8] }));
    drop(c);
    std::thread::sleep(Duration::from_millis(2500));
    assert!(env.config.socket_path().exists());
    let again = env.client("cli");
    let info = call(&again, "session.attach", json!({}));
    assert_eq!(info["result"]["dirty"], true, "{info}");
    assert_eq!(call(&again, "file.save", json!({}))["ok"], true);
}

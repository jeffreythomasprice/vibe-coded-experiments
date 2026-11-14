use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Dir(PathBuf);

thread_local! {
    static CONFIG: std::cell::RefCell<PathBuf> = const { std::cell::RefCell::new(PathBuf::new()) };
}

fn config() -> PathBuf {
    CONFIG.with(|c| c.borrow().clone())
}

fn server_binary() -> PathBuf {
    static BUILT: std::sync::Once = std::sync::Once::new();
    BUILT.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = Command::new(cargo)
            .args(["build", "-p", "editor-server"])
            .status()
            .unwrap();
        assert!(status.success());
    });
    Path::new(env!("CARGO_BIN_EXE_editor-cli"))
        .parent()
        .unwrap()
        .join("editor-server")
}

impl Dir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("editor-cli-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        let cfg = p.join("config.toml");
        std::fs::write(
            &cfg,
            format!(
                "socket = {:?}\n[server]\nidle_exit_secs = 1\nbinary = {:?}\n[recovery]\ndir = {:?}\n",
                p.join("s.sock").display().to_string(),
                server_binary().display().to_string(),
                p.join("state").display().to_string(),
            ),
        )
        .unwrap();
        CONFIG.with(|c| *c.borrow_mut() = cfg);
        Dir(p)
    }

    fn file(&self) -> PathBuf {
        self.0.join("t.scene.toml")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        if let Ok(config) = editor_proto::Config::load_from(&self.0.join("config.toml"))
            && let Ok(c) =
                editor_proto::Client::connect_with(&config, "cleanup", true, true, false, None)
        {
            let _ = c.control(editor_proto::ControlAction::Stop, true);
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cli(args: &[&str]) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_editor-cli"))
        .args(args)
        .env("EDITOR_CONFIG", config())
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    let value: Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("bad output {text:?}: {e}; stderr {:?}", out.stderr));
    (out.status.code().unwrap(), value)
}

fn run(file: &Path, op: &str, args: Value) -> (i32, Value) {
    cli(&[
        "--file",
        file.to_str().unwrap(),
        "run",
        op,
        "--args",
        &args.to_string(),
    ])
}

fn load(file: &Path) -> editor_core::Document {
    editor_fmt::read(&std::fs::read_to_string(file).unwrap()).unwrap()
}

#[test]
fn new_then_build_and_reload() {
    let d = Dir::new();
    let f = d.file();
    let (code, v) = cli(&["--file", f.to_str().unwrap(), "new", "--name", "demo"]);
    assert_eq!((code, &v["ok"]), (0, &json!(true)), "{v}");
    assert_eq!(v["result"]["name"], "demo");

    let (code, v) = run(
        &f,
        "brush.create_box",
        json!({ "min": [0, 0, 0], "max": [64, 64, 64] }),
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["result"]["id"], "b2");

    let (_, v) = run(
        &f,
        "brush.create_prism",
        json!({ "center": [32, 32, 32], "radius": 16, "sides": 8, "height": 128 }),
    );
    assert_eq!(v["result"]["id"], "b3");

    let (code, v) = run(
        &f,
        "csg.subtract",
        json!({ "targets": ["b2"], "cutters": ["b3"], "output": "mesh" }),
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["result"]["id"], "m4");

    let doc = load(&f);
    assert_eq!(doc.name, "demo");
    assert_eq!(doc.brushes.len(), 1);
    assert_eq!(doc.meshes.len(), 1);

    let (_, v) = run(&f, "scene.summary", json!({}));
    assert_eq!(v["result"]["counts"]["mesh"], 1);
    let (_, v) = run(&f, "check.validate", json!({}));
    assert_eq!(v["result"]["errors"], 0, "{v}");
}

#[test]
fn new_refuses_to_overwrite() {
    let d = Dir::new();
    let f = d.file();
    assert_eq!(cli(&["--file", f.to_str().unwrap(), "new"]).0, 0);
    let (code, v) = cli(&["--file", f.to_str().unwrap(), "new"]);
    assert_eq!((code, v["code"].as_str()), (2, Some("Io")));
    assert_eq!(cli(&["--file", f.to_str().unwrap(), "new", "--force"]).0, 0);
}

#[test]
fn command_error_exits_1_and_leaves_file_alone() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let before = std::fs::read_to_string(&f).unwrap();
    let (code, v) = run(&f, "object.delete", json!({ "ids": ["b9"] }));
    assert_eq!(code, 1);
    assert_eq!(v["code"], "NotFound");
    assert_eq!(std::fs::read_to_string(&f).unwrap(), before);
}

#[test]
fn dry_run_and_reads_do_not_write() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let before = std::fs::read_to_string(&f).unwrap();
    let (code, v) = cli(&[
        "--file",
        f.to_str().unwrap(),
        "run",
        "brush.create_box",
        "--dry-run",
        "--args",
        r#"{"min":[0,0,0],"max":[8,8,8]}"#,
    ]);
    assert_eq!(code, 0);
    assert_eq!(v["changes"]["created"], json!(["b2"]));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), before);
    run(&f, "scene.summary", json!({}));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), before);
}

#[test]
fn expect_revision_flag() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let (code, v) = cli(&[
        "--file",
        f.to_str().unwrap(),
        "run",
        "scene.summary",
        "--expect-revision",
        "5",
    ]);
    assert_eq!((code, v["code"].as_str()), (1, Some("RevisionMismatch")));
}

#[test]
fn apply_batch_is_atomic() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);

    let good = d.0.join("good.json");
    std::fs::write(
        &good,
        json!([
            { "op": "brush.create_box", "args": { "min": [0,0,0], "max": [64,64,64] } },
            { "op": "brush.clip", "args": { "ids": ["$0"], "plane": [[32,0,0],[32,64,0],[32,0,64]], "keep": "both" } },
        ])
        .to_string(),
    )
    .unwrap();
    let (code, v) = cli(&[
        "--file",
        f.to_str().unwrap(),
        "apply",
        good.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(load(&f).brushes.len(), 2);

    let before = std::fs::read_to_string(&f).unwrap();
    let bad = d.0.join("bad.json");
    std::fs::write(
        &bad,
        json!({ "commands": [
            { "op": "brush.create_box", "args": { "min": [0,0,0], "max": [8,8,8] } },
            { "op": "object.delete", "args": { "ids": ["b777"] } },
        ] })
        .to_string(),
    )
    .unwrap();
    let (code, v) = cli(&[
        "--file",
        f.to_str().unwrap(),
        "apply",
        bad.to_str().unwrap(),
    ]);
    assert_eq!(code, 1);
    assert_eq!(v["index"], 1);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), before);
}

#[test]
fn io_and_format_errors_exit_2() {
    let d = Dir::new();
    let missing = d.0.join("nope.scene.toml");
    let (code, v) = run(&missing, "scene.summary", json!({}));
    assert_eq!((code, v["code"].as_str()), (2, Some("Io")));

    let broken = d.0.join("broken.scene.toml");
    std::fs::write(&broken, "this is not toml = = =").unwrap();
    let (code, v) = run(&broken, "scene.summary", json!({}));
    assert_eq!((code, v["code"].as_str()), (2, Some("Format")));

    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let (code, v) = cli(&[
        "--file",
        f.to_str().unwrap(),
        "run",
        "scene.summary",
        "--args",
        "{oops",
    ]);
    assert_eq!((code, v["code"].as_str()), (2, Some("InvalidArgument")));
}

#[test]
fn saved_file_matches_scene_get() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    run(
        &f,
        "brush.create_box",
        json!({ "min": [-8, 0, 0], "max": [8, 16, 32], "material": "base/x" }),
    );
    let (_, v) = run(
        &f,
        "scene.get",
        json!({ "ids": ["b2"], "detail": "geometry" }),
    );
    let obj = &v["result"]["objects"][0];
    assert_eq!(obj["bounds"]["min"], json!([-8.0, 0.0, 0.0]));
    assert_eq!(obj["geometry"]["vertices"].as_array().unwrap().len(), 8);
    let doc = load(&f);
    assert_eq!(
        doc.brushes.values().next().unwrap().brush.faces[0]
            .attrs
            .material,
        "base/x"
    );
}

fn scene_with_box(d: &Dir) -> PathBuf {
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let (code, _) = run(
        &f,
        "brush.create_box",
        json!({ "min": [0, 0, 0], "max": [64, 64, 64], "material": "m/a" }),
    );
    assert_eq!(code, 0);
    f
}

#[test]
fn screenshot_writes_a_png_and_leaves_the_scene_alone() {
    let d = Dir::new();
    let f = scene_with_box(&d);
    let before = std::fs::read(&f).unwrap();
    let out = d.0.join("shot.png");

    let (code, v) = cli(&[
        "--file",
        f.to_str().unwrap(),
        "screenshot",
        "--camera",
        "viewport.top",
        "--width",
        "200",
        "--height",
        "120",
        "--mode",
        "flat",
        "--overlay",
        "grid,labels",
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["result"]["width"], 200);
    assert_eq!(v["result"]["height"], 120);
    assert_eq!(v["result"]["path"], out.to_str().unwrap());
    assert_eq!(v["result"]["labels"][0]["id"], "b2");
    assert_eq!(v["result"]["camera"]["name"], "viewport.top");

    let img = image::open(&out).unwrap();
    assert_eq!((img.width(), img.height()), (200, 120));
    assert_eq!(std::fs::read(&f).unwrap(), before);
}

#[test]
fn screenshot_flags_override_args_json() {
    let d = Dir::new();
    let f = scene_with_box(&d);
    let out = d.0.join("shot.png");
    let (code, v) = cli(&[
        "--file",
        f.to_str().unwrap(),
        "screenshot",
        "--args",
        r#"{"width": 80, "height": 60, "camera": "viewport.front"}"#,
        "--width",
        "100",
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(
        (v["result"]["width"].clone(), v["result"]["height"].clone()),
        (json!(100), json!(60))
    );
    assert_eq!(v["result"]["camera"]["name"], "viewport.front");
}

#[test]
fn screenshot_errors() {
    let d = Dir::new();
    let f = scene_with_box(&d);
    let out = d.0.join("shot.png");
    let shoot = |extra: &[&str]| {
        let mut args = vec![
            "--file",
            f.to_str().unwrap(),
            "screenshot",
            "-o",
            out.to_str().unwrap(),
        ];
        args.extend_from_slice(extra);
        cli(&args)
    };

    let (code, v) = shoot(&["--camera", "nope"]);
    assert_eq!((code, v["code"].as_str()), (1, Some("NotFound")), "{v}");
    let (code, v) = shoot(&["--width", "0"]);
    assert_eq!(
        (code, v["code"].as_str()),
        (1, Some("InvalidArgument")),
        "{v}"
    );
    let (code, v) = shoot(&["--args", "[1]"]);
    assert_eq!(
        (code, v["code"].as_str()),
        (2, Some("InvalidArgument")),
        "{v}"
    );
    assert!(!out.exists());
}

fn session_run(file: &Path, flags: &[&str], lines: &[Value]) -> (i32, Vec<Value>, String) {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_editor-cli"))
        .arg("--file")
        .arg(file)
        .arg("session")
        .args(flags)
        .env("EDITOR_CONFIG", config())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        for l in lines {
            writeln!(stdin, "{l}").unwrap();
        }
    }
    let out = child.wait_with_output().unwrap();
    let replies = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (
        out.status.code().unwrap(),
        replies,
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn ops_lists_the_registry() {
    let out = Command::new(env!("CARGO_BIN_EXE_editor-cli"))
        .args(["ops", "--json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let ops = v["ops"].as_array().unwrap();
    assert!(
        ops.iter()
            .any(|o| o["name"] == "brush.create_box" && o["safety"] == "mutate")
    );
    assert!(ops.iter().any(|o| o["name"] == "select.set"));

    let out = Command::new(env!("CARGO_BIN_EXE_editor-cli"))
        .args(["ops", "--safety", "destructive"])
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("object.delete"));
    assert!(!text.contains("brush.create_box"));
}

#[test]
fn schema_prints_args_schema() {
    let out = Command::new(env!("CARGO_BIN_EXE_editor-cli"))
        .args(["schema", "brush.create_box"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["properties"]["min"].is_object(), "{v}");

    let out = Command::new(env!("CARGO_BIN_EXE_editor-cli"))
        .args(["schema", "nope"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn schema_prints_result_and_wire_schemas() {
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_editor-cli"))
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}");
        serde_json::from_slice::<Value>(&out.stdout).unwrap()
    };
    let v = run(&["schema", "brush.create_box", "--result"]);
    assert!(
        v["$defs"]["Created"]["properties"]["ids"].is_object(),
        "{v}"
    );
    let wire = run(&["schema", "--wire"]);
    assert!(
        wire["client_msg"].is_object() && wire["server_msg"].is_object(),
        "{wire}"
    );
}

#[test]
fn session_keeps_undo_selection_and_saves_on_request() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let (code, r, stderr) = session_run(
        &f,
        &[],
        &[
            json!({ "op": "brush.create_box", "args": { "min": [0,0,0], "max": [8,8,8] } }),
            json!({ "op": "select.set", "args": { "ids": ["b2"] } }),
            json!({ "op": "scene.summary" }),
            json!({ "op": "history.undo" }),
            json!({ "op": "scene.summary" }),
            json!({ "op": "history.redo" }),
            json!({ "op": "file.save" }),
            json!({ "op": "nonsense" }),
        ],
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(r[0]["result"]["id"], "b2");
    assert_eq!(r[2]["result"]["selection"], 1);
    assert_eq!(r[3]["ok"], true);
    assert_eq!(r[4]["result"]["counts"]["brush"], 0);
    assert_eq!(r[6]["ok"], true, "{}", r[6]);
    assert_eq!(r[7]["ok"], false);
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(load(&f).brushes.len(), 1);
}

#[test]
fn session_warns_about_unsaved_changes_and_does_not_write() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let (_, _, stderr) = session_run(
        &f,
        &[],
        &[json!({ "op": "brush.create_box", "args": { "min": [0,0,0], "max": [8,8,8] } })],
    );
    assert!(stderr.contains("unsaved"), "{stderr}");
    assert_eq!(load(&f).brushes.len(), 0);
}

#[test]
fn session_autosave_and_read_only() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let (_, r, stderr) = session_run(
        &f,
        &["--autosave"],
        &[json!({ "op": "brush.create_box", "args": { "min": [0,0,0], "max": [8,8,8] } })],
    );
    assert_eq!(r[0]["ok"], true);
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(load(&f).brushes.len(), 1);

    let (_, r, _) = session_run(
        &f,
        &["--read-only"],
        &[
            json!({ "op": "scene.summary" }),
            json!({ "op": "object.delete", "args": { "ids": ["b2"] } }),
            json!({ "op": "batch", "args": { "commands": [{ "op": "object.delete", "args": { "ids": ["b2"] } }] } }),
        ],
    );
    assert_eq!(r[0]["ok"], true);
    assert_eq!(r[1]["code"], "NotPermitted");
    assert_eq!(r[2]["code"], "NotPermitted");
    assert_eq!(load(&f).brushes.len(), 1);
}

#[test]
fn session_survives_garbage_lines() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_editor-cli"))
        .arg("--file")
        .arg(&f)
        .arg("session")
        .env("EDITOR_CONFIG", config())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "not json\n\n{{\"op\":\"scene.summary\"}}"
    )
    .unwrap();
    let out = child.wait_with_output().unwrap();
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["ok"], false);
    assert_eq!(lines[1]["ok"], true);
}

#[test]
fn one_shot_without_file_or_session_errors() {
    let d = Dir::new();
    let _ = &d;
    let (code, v) = cli(&["run", "scene.summary"]);
    assert_eq!(
        (code, v["code"].as_str()),
        (1, Some("InvalidArgument")),
        "{v}"
    );
    assert!(v["hint"].as_str().unwrap().contains("--file"));
}

struct Live {
    child: std::process::Child,
    lines: std::io::BufReader<std::process::ChildStdout>,
}

impl Live {
    fn start(file: &Path, flags: &[&str]) -> Self {
        use std::process::Stdio;
        let mut child = Command::new(env!("CARGO_BIN_EXE_editor-cli"))
            .arg("--file")
            .arg(file)
            .arg("session")
            .args(flags)
            .env("EDITOR_CONFIG", config())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let out = child.stdout.take().unwrap();
        let mut live = Live {
            child,
            lines: std::io::BufReader::new(out),
        };
        let first = live.send(json!({ "op": "file.info" }));
        assert_eq!(first["ok"], true, "{first}");
        live
    }

    fn read(&mut self) -> Value {
        use std::io::BufRead;
        let mut line = String::new();
        self.lines.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{line:?}: {e}"))
    }

    fn send(&mut self, request: Value) -> Value {
        use std::io::Write;
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        loop {
            let v = self.read();
            if v.get("event").is_none() {
                return v;
            }
        }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

#[test]
fn one_shot_edits_a_live_session_without_saving() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let mut live = Live::start(&f, &[]);
    let before = std::fs::read_to_string(&f).unwrap();
    let (code, v) = run(
        &f,
        "brush.create_box",
        json!({ "min": [0,0,0], "max": [8,8,8] }),
    );
    assert_eq!(code, 0, "{v}");
    assert_eq!(std::fs::read_to_string(&f).unwrap(), before);
    let seen = live.send(json!({ "op": "scene.summary" }));
    assert_eq!(seen["result"]["counts"]["brush"], 1, "{seen}");

    let (code, v) = run(&f, "scene.summary", json!({}));
    assert_eq!(
        (code, v["result"]["counts"]["brush"].clone()),
        (0, json!(1))
    );
    let saved = live.send(json!({ "op": "file.save" }));
    assert_eq!(saved["ok"], true);
    assert_eq!(load(&f).brushes.len(), 1);
}

#[test]
fn events_show_other_clients_edits() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let mut live = Live::start(&f, &["--events"]);
    let (code, _) = run(
        &f,
        "brush.create_box",
        json!({ "min": [0,0,0], "max": [8,8,8] }),
    );
    assert_eq!(code, 0);
    let event = loop {
        let v = live.read();
        if v["event"] == "changed" {
            break v;
        }
    };
    assert_eq!(event["upsert"][0]["id"], "b2", "{event}");
    assert!(event.get("document_toml").is_none());
}

#[test]
fn sessions_and_server_status() {
    let d = Dir::new();
    let f = d.file();
    cli(&["--file", f.to_str().unwrap(), "new"]);
    let _live = Live::start(&f, &[]);
    let (code, v) = cli(&["sessions"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["result"]["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(
        v["result"]["sessions"][0]["path"],
        f.canonicalize().unwrap().to_str().unwrap()
    );
    let (code, v) = cli(&["server", "status"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["result"]["connections"], 2);
}

#[test]
fn server_stop_and_status_when_not_running() {
    let d = Dir::new();
    let _ = &d;
    let (code, v) = cli(&["server", "status"]);
    assert_eq!(
        (code, v["result"]["running"].clone()),
        (0, json!(false)),
        "{v}"
    );
}

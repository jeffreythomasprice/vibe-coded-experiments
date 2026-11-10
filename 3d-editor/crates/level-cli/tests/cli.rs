use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("level-cli-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    fn file(&self) -> PathBuf {
        self.0.join("t.level.toml")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn cli(args: &[&str]) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_level-cli"))
        .args(args)
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    let value: Value = serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("bad output {text:?}: {e}; stderr {:?}", out.stderr));
    (out.status.code().unwrap(), value)
}

fn run(file: &Path, op: &str, args: Value) -> (i32, Value) {
    cli(&[
        file.to_str().unwrap(),
        "run",
        op,
        "--args",
        &args.to_string(),
    ])
}

fn load(file: &Path) -> level_core::Document {
    level_fmt::read(&std::fs::read_to_string(file).unwrap()).unwrap()
}

#[test]
fn new_then_build_and_reload() {
    let d = Dir::new();
    let f = d.file();
    let (code, v) = cli(&[f.to_str().unwrap(), "new", "--name", "demo"]);
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
    assert_eq!(cli(&[f.to_str().unwrap(), "new"]).0, 0);
    let (code, v) = cli(&[f.to_str().unwrap(), "new"]);
    assert_eq!((code, v["code"].as_str()), (2, Some("Io")));
    assert_eq!(cli(&[f.to_str().unwrap(), "new", "--force"]).0, 0);
}

#[test]
fn command_error_exits_1_and_leaves_file_alone() {
    let d = Dir::new();
    let f = d.file();
    cli(&[f.to_str().unwrap(), "new"]);
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
    cli(&[f.to_str().unwrap(), "new"]);
    let before = std::fs::read_to_string(&f).unwrap();
    let (code, v) = cli(&[
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
    cli(&[f.to_str().unwrap(), "new"]);
    let (code, v) = cli(&[
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
    cli(&[f.to_str().unwrap(), "new"]);

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
    let (code, v) = cli(&[f.to_str().unwrap(), "apply", good.to_str().unwrap()]);
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
    let (code, v) = cli(&[f.to_str().unwrap(), "apply", bad.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert_eq!(v["index"], 1);
    assert_eq!(std::fs::read_to_string(&f).unwrap(), before);
}

#[test]
fn io_and_format_errors_exit_2() {
    let d = Dir::new();
    let missing = d.0.join("nope.level.toml");
    let (code, v) = run(&missing, "scene.summary", json!({}));
    assert_eq!((code, v["code"].as_str()), (2, Some("Io")));

    let broken = d.0.join("broken.level.toml");
    std::fs::write(&broken, "this is not toml = = =").unwrap();
    let (code, v) = run(&broken, "scene.summary", json!({}));
    assert_eq!((code, v["code"].as_str()), (2, Some("Format")));

    let f = d.file();
    cli(&[f.to_str().unwrap(), "new"]);
    let (code, v) = cli(&[
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
    cli(&[f.to_str().unwrap(), "new"]);
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

fn level_with_box(d: &Dir) -> PathBuf {
    let f = d.file();
    cli(&[f.to_str().unwrap(), "new"]);
    let (code, _) = run(
        &f,
        "brush.create_box",
        json!({ "min": [0, 0, 0], "max": [64, 64, 64], "material": "m/a" }),
    );
    assert_eq!(code, 0);
    f
}

#[test]
fn screenshot_writes_a_png_and_leaves_the_level_alone() {
    let d = Dir::new();
    let f = level_with_box(&d);
    let before = std::fs::read(&f).unwrap();
    let out = d.0.join("shot.png");

    let (code, v) = cli(&[
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
    let f = level_with_box(&d);
    let out = d.0.join("shot.png");
    let (code, v) = cli(&[
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
    let f = level_with_box(&d);
    let out = d.0.join("shot.png");
    let shoot = |extra: &[&str]| {
        let mut args = vec![
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

    let (code, v) = run(&f, "view.screenshot", json!({}));
    assert_eq!(
        (code, v["code"].as_str()),
        (1, Some("InvalidArgument")),
        "{v}"
    );
    assert!(v["hint"].as_str().unwrap().contains("screenshot"));
}

fn session_run(file: &Path, flags: &[&str], lines: &[Value]) -> (i32, Vec<Value>, String) {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_level-cli"))
        .arg(file)
        .arg("session")
        .args(flags)
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
    let out = Command::new(env!("CARGO_BIN_EXE_level-cli"))
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

    let out = Command::new(env!("CARGO_BIN_EXE_level-cli"))
        .args(["ops", "--safety", "destructive"])
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("object.delete"));
    assert!(!text.contains("brush.create_box"));
}

#[test]
fn schema_prints_args_schema() {
    let out = Command::new(env!("CARGO_BIN_EXE_level-cli"))
        .args(["schema", "brush.create_box"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["properties"]["min"].is_object(), "{v}");

    let out = Command::new(env!("CARGO_BIN_EXE_level-cli"))
        .args(["schema", "nope"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn session_keeps_undo_selection_and_saves_on_request() {
    let d = Dir::new();
    let f = d.file();
    cli(&[f.to_str().unwrap(), "new"]);
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
    cli(&[f.to_str().unwrap(), "new"]);
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
    cli(&[f.to_str().unwrap(), "new"]);
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
    cli(&[f.to_str().unwrap(), "new"]);
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_level-cli"))
        .arg(&f)
        .arg("session")
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
fn one_shot_run_rejects_session_only_ops() {
    let d = Dir::new();
    let f = d.file();
    cli(&[f.to_str().unwrap(), "new"]);
    let (code, v) = run(&f, "select.clear", json!({}));
    assert_eq!(code, 1);
    assert_eq!(v["code"], "InvalidArgument");
    let (code, _) = run(&f, "history.undo", json!({}));
    assert_eq!(code, 1);
}

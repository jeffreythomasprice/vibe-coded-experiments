use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};

type Client = RunningService<RoleClient, ()>;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

struct Env {
    dir: std::path::PathBuf,
    config: std::path::PathBuf,
}

fn server_binary() -> std::path::PathBuf {
    static BUILT: std::sync::Once = std::sync::Once::new();
    BUILT.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = std::process::Command::new(cargo)
            .args(["build", "-p", "editor-server"])
            .status()
            .unwrap();
        assert!(status.success());
    });
    std::path::Path::new(env!("CARGO_BIN_EXE_editor-mcp"))
        .parent()
        .unwrap()
        .join("editor-server")
}

impl Env {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("editor-mcp-env-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.toml");
        std::fs::write(
            &config,
            format!(
                "socket = {:?}\n[server]\nidle_exit_secs = 1\nbinary = {:?}\n[recovery]\ndir = {:?}\nautosave_secs = 1\n",
                dir.join("s.sock").display().to_string(),
                server_binary().display().to_string(),
                dir.join("state").display().to_string(),
            ),
        )
        .unwrap();
        Self { dir, config }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Ok(config) = editor_proto::Config::load_from(&self.config)
            && let Ok(c) =
                editor_proto::Client::connect_with(&config, "cleanup", true, true, false, None)
        {
            let _ = c.control(editor_proto::ControlAction::Stop, true);
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn connect(env: &Env, args: &[&str]) -> Client {
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_editor-mcp")).configure(|c| {
            c.args(args).env("EDITOR_CONFIG", &env.config);
        }),
    )
    .unwrap();
    ().serve(transport).await.unwrap()
}

async fn names(client: &Client) -> Vec<String> {
    client
        .list_all_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name.to_string())
        .collect()
}

async fn call(client: &Client, tool: &str, args: Value) -> (CallToolResult, Value) {
    let result = client
        .call_tool(
            CallToolRequestParams::new(tool.to_string())
                .with_arguments(args.as_object().cloned().unwrap_or_default()),
        )
        .await
        .unwrap();
    let text = result
        .content
        .iter()
        .find_map(|c| c.as_text())
        .map(|t| t.text.clone())
        .expect("text content");
    let value = serde_json::from_str(&text).unwrap();
    (result, value)
}

#[tokio::test]
async fn tools_follow_the_permission_flags() {
    let env = Env::new();
    let read = connect(&env, &[]).await;
    let list = names(&read).await;
    assert!(list.contains(&"scene_summary".to_string()));
    assert!(list.contains(&"view_screenshot".to_string()));
    assert!(!list.contains(&"brush_create_box".to_string()));
    read.cancel().await.unwrap();

    let mutate = connect(&env, &["--allow-mutate"]).await;
    let list = names(&mutate).await;
    assert!(list.contains(&"brush_create_box".to_string()));
    assert!(!list.contains(&"object_delete".to_string()));
    mutate.cancel().await.unwrap();

    let all = connect(&env, &["--allow-destructive"]).await;
    assert!(names(&all).await.contains(&"object_delete".to_string()));
    all.cancel().await.unwrap();
}

#[tokio::test]
async fn tool_definitions_carry_schemas_and_annotations() {
    let env = Env::new();
    let client = connect(&env, &["--allow-destructive"]).await;
    let tools = client.list_all_tools().await.unwrap();
    let create = tools.iter().find(|t| t.name == "brush_create_box").unwrap();
    assert!(
        create
            .input_schema
            .get("properties")
            .unwrap()
            .get("min")
            .is_some()
    );
    assert_eq!(
        create.annotations.as_ref().unwrap().read_only_hint,
        Some(false)
    );
    let delete = tools.iter().find(|t| t.name == "object_delete").unwrap();
    assert_eq!(
        delete.annotations.as_ref().unwrap().destructive_hint,
        Some(true)
    );
    let summary = tools.iter().find(|t| t.name == "scene_summary").unwrap();
    assert_eq!(
        summary.annotations.as_ref().unwrap().read_only_hint,
        Some(true)
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn create_then_get_round_trip() {
    let env = Env::new();
    let client = connect(&env, &["--allow-mutate"]).await;
    let (r, v) = call(
        &client,
        "brush_create_box",
        json!({ "min": [0,0,0], "max": [8,8,8] }),
    )
    .await;
    assert_eq!(r.is_error, Some(false));
    assert_eq!(v["result"]["id"], "b2");

    let (_, v) = call(&client, "scene_get", json!({ "ids": ["b2"] })).await;
    assert_eq!(v["result"]["objects"][0]["kind"], "brush");

    let (r, v) = call(
        &client,
        "brush_create_box",
        json!({ "min": [0,0,0], "max": [0,8,8] }),
    )
    .await;
    assert_eq!(r.is_error, Some(true));
    assert_eq!(v["code"], "InvalidArgument");

    let (_, v) = call(
        &client,
        "brush_create_box",
        json!({ "min": [0,0,0], "max": [4,4,4], "dry_run": true }),
    )
    .await;
    assert_eq!(v["changes"]["created"].as_array().unwrap().len(), 1);
    let (_, v) = call(&client, "scene_summary", json!({})).await;
    assert_eq!(v["result"]["counts"]["brush"], 1);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn history_and_selection_persist_across_calls() {
    let env = Env::new();
    let client = connect(&env, &["--allow-destructive"]).await;
    call(&client, "history_begin", json!({ "label": "two boxes" })).await;
    call(
        &client,
        "brush_create_box",
        json!({ "min": [0,0,0], "max": [8,8,8] }),
    )
    .await;
    call(
        &client,
        "brush_create_box",
        json!({ "min": [16,0,0], "max": [24,8,8] }),
    )
    .await;
    call(&client, "history_commit", json!({})).await;
    call(&client, "select_set", json!({ "ids": ["b2"] })).await;
    let (_, v) = call(&client, "select_get", json!({})).await;
    assert_eq!(v["result"]["count"], 1);
    let (_, v) = call(&client, "history_undo", json!({})).await;
    assert_eq!(v["ok"], true);
    let (_, v) = call(&client, "scene_summary", json!({})).await;
    assert_eq!(v["result"]["counts"]["brush"], 0);
    assert_eq!(v["result"]["selection"], 0);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn destructive_ops_are_refused_even_inside_a_batch() {
    let env = Env::new();
    let client = connect(&env, &["--allow-mutate"]).await;
    call(
        &client,
        "brush_create_box",
        json!({ "min": [0,0,0], "max": [8,8,8] }),
    )
    .await;
    let (r, v) = call(
        &client,
        "batch",
        json!({ "commands": [{ "op": "object.delete", "args": { "ids": ["b2"] } }] }),
    )
    .await;
    assert_eq!(r.is_error, Some(true));
    assert_eq!(v["code"], "NotPermitted");
    let (r, v) = call(&client, "object_delete", json!({ "ids": ["b2"] })).await;
    assert!(r.is_error == Some(true) || v["code"] == "NotPermitted");
    let (_, v) = call(&client, "scene_summary", json!({})).await;
    assert_eq!(v["result"]["counts"]["brush"], 1);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn file_save_writes_the_document() {
    let env = Env::new();
    let dir = std::env::temp_dir().join(format!("editor-mcp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.scene.toml");
    let client = connect(
        &env,
        &[path.to_str().unwrap(), "--create", "--allow-mutate"],
    )
    .await;
    assert!(path.exists());
    call(
        &client,
        "brush_create_box",
        json!({ "min": [0,0,0], "max": [8,8,8] }),
    )
    .await;
    let (_, v) = call(&client, "file_info", json!({})).await;
    assert_eq!(v["result"]["dirty"], true);
    call(&client, "file_save", json!({})).await;
    let (_, v) = call(&client, "file_info", json!({})).await;
    assert_eq!(v["result"]["dirty"], false);
    client.cancel().await.unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("b2"), "{text}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn missing_file_without_create_fails_to_start() {
    let env = Env::new();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_editor-mcp"))
        .arg("/nonexistent/x.scene.toml")
        .env("EDITOR_CONFIG", &env.config)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--create"));
}

#[tokio::test]
async fn screenshot_returns_image_content() {
    let env = Env::new();
    let client = connect(&env, &["--allow-mutate"]).await;
    call(
        &client,
        "brush_create_box",
        json!({ "min": [0,0,0], "max": [64,64,64] }),
    )
    .await;
    call(&client, "select_set", json!({ "ids": ["b2"] })).await;
    let (r, v) = call(
        &client,
        "view_screenshot",
        json!({ "camera": "viewport.top", "width": 64, "height": 64, "overlays": ["selection", "labels"] }),
    )
    .await;
    if r.is_error == Some(true) && v["code"] == "Render" {
        eprintln!("skipping: no graphics adapter ({})", v["message"]);
        return;
    }
    assert_eq!(r.is_error, Some(false), "{v}");
    assert_eq!(v["result"]["width"], 64);
    let image = r
        .content
        .iter()
        .find_map(|c| match c {
            ContentBlock::Image(i) => Some(i),
            _ => None,
        })
        .expect("image content");
    assert_eq!(image.mime_type, "image/png");
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn results_conform_to_the_declared_output_schema() {
    let env = Env::new();
    let client = connect(&env, &["--allow-mutate"]).await;
    let tools = client.list_all_tools().await.unwrap();
    assert!(tools.iter().all(|t| t.output_schema.is_some()));

    let validate = |tool: &str, result: &CallToolResult| {
        let schema = tools
            .iter()
            .find(|t| t.name == tool)
            .and_then(|t| t.output_schema.clone())
            .unwrap();
        let validator = jsonschema::validator_for(&Value::Object((*schema).clone())).unwrap();
        let structured = result
            .structured_content
            .clone()
            .expect("structured content");
        let errors: Vec<String> = validator
            .iter_errors(&structured)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{tool}: {errors:?}\n{structured}");
        structured
    };

    let (result, _) = call(
        &client,
        "brush_create_box",
        json!({ "min": [0, 0, 0], "max": [8, 8, 8] }),
    )
    .await;
    let structured = validate("brush_create_box", &result);
    assert_eq!(structured["ok"], true);
    assert!(structured["result"]["ids"].is_array());

    let (result, _) = call(&client, "scene_summary", json!({})).await;
    assert_eq!(
        validate("scene_summary", &result)["result"]["counts"]["brush"],
        1
    );

    let (result, value) = call(&client, "scene_get", json!({ "ids": ["b999"] })).await;
    assert_eq!(result.is_error, Some(true));
    assert_eq!(validate("scene_get", &result), value);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn visibility_locks_and_open_groups_work_over_mcp() {
    let env = Env::new();
    let client = connect(&env, &["--allow-destructive"]).await;
    let list = names(&client).await;
    for tool in [
        "object_set_hidden",
        "object_set_locked",
        "object_isolate",
        "object_show_all",
        "object_unlock_all",
        "layer_set_omit_from_export",
        "group_open",
        "group_close",
    ] {
        assert!(list.contains(&tool.to_string()), "{tool}");
    }

    let make = |min: i32| json!({ "min": [min, 0, 0], "max": [min + 8, 8, 8] });
    let (_, a) = call(&client, "brush_create_box", make(0)).await;
    let (_, b) = call(&client, "brush_create_box", make(32)).await;
    let (a, b) = (
        a["result"]["id"].as_str().unwrap().to_string(),
        b["result"]["id"].as_str().unwrap().to_string(),
    );

    let (_, v) = call(
        &client,
        "object_set_hidden",
        json!({ "ids": [&a], "hidden": true }),
    )
    .await;
    assert_eq!(v["result"]["updated"], 1);
    let (_, v) = call(&client, "scene_summary", json!({})).await;
    assert_eq!(v["result"]["hidden"], 1);
    call(&client, "object_show_all", json!({})).await;
    call(&client, "object_isolate", json!({ "ids": [&a] })).await;
    let (_, v) = call(&client, "scene_summary", json!({})).await;
    assert_eq!(v["result"]["hidden"], 1);
    call(&client, "object_show_all", json!({})).await;
    let (_, v) = call(&client, "scene_summary", json!({})).await;
    assert_eq!(v["result"]["hidden"], 0);

    call(
        &client,
        "object_set_locked",
        json!({ "ids": [&a], "locked": true }),
    )
    .await;
    let (r, v) = call(
        &client,
        "transform_translate",
        json!({ "ids": [&a], "delta": [1, 0, 0] }),
    )
    .await;
    assert_eq!(r.is_error, Some(true));
    assert_eq!(v["code"], "Locked");
    call(&client, "object_unlock_all", json!({})).await;

    let (_, g) = call(
        &client,
        "group_create",
        json!({ "objects": [&a], "name": "inside" }),
    )
    .await;
    let g = g["result"]["id"].as_str().unwrap().to_string();
    let (_, v) = call(&client, "group_open", json!({ "id": &g })).await;
    assert_eq!(v["result"]["open"], json!([&g]));
    let (_, v) = call(&client, "scene_summary", json!({})).await;
    assert_eq!(v["result"]["open_group"], json!(&g));

    let (r, v) = call(
        &client,
        "transform_translate",
        json!({ "ids": [&b], "delta": [1, 0, 0] }),
    )
    .await;
    assert_eq!(r.is_error, Some(true));
    assert_eq!(v["code"], "Locked");
    let (_, c) = call(&client, "brush_create_box", make(64)).await;
    let c = c["result"]["id"].as_str().unwrap().to_string();
    let (_, v) = call(&client, "scene_get", json!({ "ids": [&c] })).await;
    assert_eq!(v["result"]["objects"][0]["group"], json!(&g));

    call(&client, "history_undo", json!({})).await;
    call(&client, "history_undo", json!({})).await;
    let (_, v) = call(&client, "scene_summary", json!({})).await;
    assert!(v["result"]["open_group"].is_null());
    let (_, v) = call(
        &client,
        "transform_translate",
        json!({ "ids": [&b], "delta": [1, 0, 0] }),
    )
    .await;
    assert_eq!(v["ok"], true);

    let (_, layers) = call(&client, "scene_summary", json!({})).await;
    let layer = layers["result"]["layers"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, v) = call(
        &client,
        "layer_set_omit_from_export",
        json!({ "id": &layer, "omit": true }),
    )
    .await;
    assert_eq!(v["ok"], true);
    let (_, v) = call(&client, "scene_summary", json!({})).await;
    assert_eq!(v["result"]["layers"][0]["omit_from_export"], true);
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn clipboard_copies_between_clients_and_files() {
    let env = Env::new();
    let dir = env.dir.join("scenes");
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.scene.toml");
    let b = dir.join("b.scene.toml");
    let one = connect(&env, &[a.to_str().unwrap(), "--create", "--allow-mutate"]).await;
    let two = connect(
        &env,
        &[b.to_str().unwrap(), "--create", "--allow-destructive"],
    )
    .await;

    let list = names(&one).await;
    for tool in ["clipboard_copy", "clipboard_get", "clipboard_paste"] {
        assert!(list.contains(&tool.to_string()), "{tool} missing");
    }

    let (_, made) = call(
        &one,
        "brush_create_box",
        json!({ "min": [0, 0, 0], "max": [16, 16, 16] }),
    )
    .await;
    let id = made["result"]["id"].as_str().unwrap().to_string();
    let (_, copied) = call(&one, "clipboard_copy", json!({ "ids": [id] })).await;
    assert_eq!(copied["result"]["count"], 1, "{copied}");
    assert!(
        copied["result"]["text"]
            .as_str()
            .unwrap()
            .contains("format")
    );

    let (_, got) = call(&two, "clipboard_get", json!({})).await;
    assert_eq!(got["result"]["seq"], copied["result"]["seq"]);

    let (_, pasted) = call(&two, "clipboard_paste", json!({ "at": [100, 50, 8] })).await;
    assert_eq!(pasted["ok"], true, "{pasted}");
    assert_eq!(pasted["result"]["top_level"], 1);
    let new = pasted["result"]["id"].as_str().unwrap().to_string();
    let (_, bounds) = call(&two, "scene_bounds", json!({ "ids": [new] })).await;
    let min = &bounds["result"]["combined"]["min"];
    assert_eq!(min[0], 92.0, "{bounds}");
    assert_eq!(min[1], 42.0, "{bounds}");

    let (_, one_summary) = call(&one, "scene_summary", json!({})).await;
    assert_eq!(one_summary["result"]["counts"]["brush"], 1);

    let (_, undone) = call(&two, "history_undo", json!({})).await;
    assert_eq!(undone["ok"], true, "{undone}");
    let (_, two_summary) = call(&two, "scene_summary", json!({})).await;
    assert_eq!(two_summary["result"]["counts"]["brush"], 0);

    let (_, bad) = call(&two, "clipboard_paste", json!({ "text": "garbage [" })).await;
    assert_eq!(bad["code"], "InvalidArgument");

    one.cancel().await.unwrap();
    two.cancel().await.unwrap();
}

fn kill_server(env: &Env) {
    let marker = env.config.display().to_string();
    for entry in std::fs::read_dir("/proc").unwrap().filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let cmdline = String::from_utf8_lossy(&cmdline).replace('\0', " ");
        if cmdline.contains("editor-server") && cmdline.contains(&marker) {
            let status = std::process::Command::new("kill")
                .args(["-9", &name])
                .status()
                .unwrap();
            assert!(status.success());
        }
    }
    let socket = env.dir.join("s.sock");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::os::unix::net::UnixStream::connect(&socket).is_ok()
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[tokio::test]
async fn an_agent_recovers_work_lost_when_the_server_dies() {
    let env = Env::new();
    let path = env.dir.join("scenes").join("r.scene.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let first = connect(
        &env,
        &[path.to_str().unwrap(), "--create", "--allow-mutate"],
    )
    .await;
    for min in [0, 32, 64] {
        call(
            &first,
            "brush_create_box",
            json!({ "min": [min, 0, 0], "max": [min + 8, 8, 8] }),
        )
        .await;
    }
    let autosave = env.dir.join("state").join("autosave");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::fs::read_dir(&autosave).map_or(0, |d| d.count()) < 2
        && std::time::Instant::now() < deadline
    {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    kill_server(&env);
    let _ = first.cancel().await;

    let second = connect(&env, &[path.to_str().unwrap(), "--allow-destructive"]).await;
    let list = names(&second).await;
    for tool in ["recovery_list", "recovery_restore", "recovery_discard"] {
        assert!(list.contains(&tool.to_string()), "{tool} missing");
    }
    let (_, summary) = call(&second, "scene_summary", json!({})).await;
    assert_eq!(summary["result"]["counts"]["brush"], 0);

    let (_, found) = call(&second, "recovery_list", json!({})).await;
    let entry = &found["result"]["entries"][0];
    assert_eq!(entry["newer_than_source"], true, "{found}");
    assert_eq!(entry["live"], false);

    let (_, restored) = call(&second, "recovery_restore", json!({ "id": entry["id"] })).await;
    assert_eq!(restored["ok"], true, "{restored}");
    let (_, summary) = call(&second, "scene_summary", json!({})).await;
    assert_eq!(summary["result"]["counts"]["brush"], 3);
    let (_, info) = call(&second, "file_info", json!({})).await;
    assert_eq!(info["result"]["dirty"], true);

    call(&second, "file_save", json!({})).await;
    let (_, after) = call(&second, "recovery_list", json!({})).await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut entries = after["result"]["entries"].as_array().unwrap().len();
    while entries > 0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let (_, again) = call(&second, "recovery_list", json!({})).await;
        entries = again["result"]["entries"].as_array().unwrap().len();
    }
    assert_eq!(entries, 0, "saving clears the autosave");
    second.cancel().await.unwrap();
}

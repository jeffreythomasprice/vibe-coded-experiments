use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use rmcp::service::RunningService;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};

type Client = RunningService<RoleClient, ()>;

async fn connect(args: &[&str]) -> Client {
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_level-mcp")).configure(|c| {
            c.args(args);
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
    let read = connect(&[]).await;
    let list = names(&read).await;
    assert!(list.contains(&"scene_summary".to_string()));
    assert!(list.contains(&"view_screenshot".to_string()));
    assert!(!list.contains(&"brush_create_box".to_string()));
    read.cancel().await.unwrap();

    let mutate = connect(&["--allow-mutate"]).await;
    let list = names(&mutate).await;
    assert!(list.contains(&"brush_create_box".to_string()));
    assert!(!list.contains(&"object_delete".to_string()));
    mutate.cancel().await.unwrap();

    let all = connect(&["--allow-destructive"]).await;
    assert!(names(&all).await.contains(&"object_delete".to_string()));
    all.cancel().await.unwrap();
}

#[tokio::test]
async fn tool_definitions_carry_schemas_and_annotations() {
    let client = connect(&["--allow-destructive"]).await;
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
    let client = connect(&["--allow-mutate"]).await;
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
    let client = connect(&["--allow-destructive"]).await;
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
    let client = connect(&["--allow-mutate"]).await;
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
    let dir = std::env::temp_dir().join(format!("level-mcp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.level.toml");
    let client = connect(&[path.to_str().unwrap(), "--create", "--allow-mutate"]).await;
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
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_level-mcp"))
        .arg("/nonexistent/x.level.toml")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--create"));
}

#[tokio::test]
async fn screenshot_returns_image_content() {
    let client = connect(&["--allow-mutate"]).await;
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

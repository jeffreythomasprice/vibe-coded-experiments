use level_cmd::{Request, Response, Session};
use level_core::Document;
use level_host::{Host, Policy};
use serde_json::json;

fn shot_request(path: &std::path::Path) -> Request {
    Request::new(
        "view.screenshot",
        json!({ "width": 64, "height": 64, "path": path.display().to_string() }),
    )
}

#[test]
fn screenshot_path_writes_the_png_and_needs_mutate() {
    let dir = std::env::temp_dir().join(format!("level-host-{}", std::process::id()));
    let path = dir.join("shot.png");

    let mut read_only = Host::new(Session::new(Document::new("t")), Policy::read_only());
    let reply = read_only.execute(shot_request(&path), "test");
    let Response::Error(e) = &reply.response else {
        panic!("expected NotPermitted")
    };
    assert_eq!(e.code, "NotPermitted");
    assert!(!path.exists());

    let mut host = Host::new(Session::new(Document::new("t")), Policy::new(true, false));
    let reply = host.execute(shot_request(&path), "test");
    let value = reply.response.to_value();
    assert_eq!(value["ok"], true, "{value}");
    assert_eq!(value["result"]["path"], path.display().to_string());
    assert!(std::fs::read(&path).unwrap().starts_with(b"\x89PNG"));
    assert!(reply.png.is_some());

    let reply = Host::new(Session::new(Document::new("t")), Policy::read_only()).execute(
        Request::new("view.screenshot", json!({ "width": 64, "height": 64 })),
        "test",
    );
    assert!(reply.response.is_ok());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn consume_cutters_needs_destructive_even_in_a_batch() {
    let mut host = Host::new(Session::new(Document::new("t")), Policy::new(true, false));
    let request = Request::new(
        "batch",
        json!({ "commands": [
            { "op": "csg_subtract", "args": { "targets": ["b1"], "cutters": ["b2"], "consume_cutters": true } },
        ]}),
    );
    let value = host.execute(request, "test").response.to_value();
    assert_eq!(value["code"], "NotPermitted");
    assert!(value["message"].as_str().unwrap().contains("destructive"));
}

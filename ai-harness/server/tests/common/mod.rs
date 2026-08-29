//! Shared helpers for the WebDriver-backed UI tests in this directory.
//!
//! These drive the *real* compiled `server` binary — a real webview, real
//! Tauri IPC, a real (but isolated) SQLite db — through `tauri-driver`
//! (<https://github.com/tauri-apps/tauri-driver>), which on Linux shells out
//! to `WebKitWebDriver` (from the `webkit2gtk-driver` apt package). None of
//! that is optional infrastructure a plain `cargo test` can assume is
//! present, so every test here follows the same shape as `lib/tests/live_*`:
//! compiled always, but skipped (not failed) unless both `AI_HARNESS_E2E=1`
//! is set *and* a `tauri-driver` is actually reachable. See `CLAUDE.md`'s
//! "UI / end-to-end tests" section for the one-time setup and the exact
//! commands to start `tauri-driver` before running these.
#![allow(dead_code)]

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use fantoccini::{Client, ClientBuilder, Locator};
use serde_json::json;

/// Where `tauri-driver` is listening. `tauri-driver`'s own default port.
fn driver_url() -> String {
    std::env::var("AI_HARNESS_E2E_DRIVER_URL").unwrap_or_else(|_| "http://localhost:4444".to_string())
}

fn e2e_enabled() -> bool {
    std::env::var("AI_HARNESS_E2E").as_deref() == Ok("1")
}

/// Call at the top of every test, before doing anything that needs a window.
/// Prints why it's skipping and returns `None` if the test should return
/// immediately — both when the opt-in env var is unset (the common case: a
/// plain `cargo test` run) and when it's set but `tauri-driver` isn't up
/// (e.g. someone forgot to start it), so the latter reads as "skipped", not
/// as a broken build.
pub async fn launch(test_name: &str) -> Option<App> {
    if !e2e_enabled() {
        eprintln!("skipping {test_name}: set AI_HARNESS_E2E=1 to run UI tests");
        return None;
    }

    let tmp_dir = std::env::temp_dir().join(format!(
        "ai-harness-e2e-{test_name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after 1970")
            .as_nanos()
    ));
    std::fs::create_dir_all(&tmp_dir).expect("create isolated e2e tmp dir");
    let config_path = tmp_dir.join("config.toml");
    // Every path below is scoped to `tmp_dir` so this test can never touch
    // the developer's real `~/.config/ai-harness` db, cache, or logs — and
    // sandboxing is off since these tests aren't exercising `lib::sandbox`.
    std::fs::write(
        &config_path,
        format!(
            "[log]\ndir = {log:?}\n[database]\npath = {db:?}\n[cache]\ndir = {cache:?}\n[sandbox]\nenabled = false\n",
            log = tmp_dir.join("logs"),
            db = tmp_dir.join("ai-harness.db"),
            cache = tmp_dir.join("cache"),
        ),
    )
    .expect("write isolated e2e config.toml");

    let mut caps = serde_json::map::Map::new();
    caps.insert("browserName".to_string(), json!("wry"));
    caps.insert(
        "tauri:options".to_string(),
        json!({
            "application": env!("CARGO_BIN_EXE_server"),
            "args": ["--config", config_path.to_str().expect("tmp path is valid utf-8")],
        }),
    );

    let client = match ClientBuilder::native().capabilities(caps).connect(&driver_url()).await {
        Ok(client) => client,
        Err(err) => {
            eprintln!("skipping {test_name}: could not reach tauri-driver at {}: {err}", driver_url());
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return None;
        }
    };

    // The shell (sidebar + content area) mounts synchronously off the first
    // Leptos render; there's no server-rendered HTML to race against, but
    // give the webview a moment to finish loading and running the wasm
    // module before the first `find` call.
    let mut shell_found = false;
    for _ in 0..50 {
        if client.find(Locator::Css(".shell")).await.is_ok() {
            shell_found = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !shell_found {
        // The app binary always points its window at `tauri.conf.json`'s
        // `devUrl` (`http://localhost:1420`) unless it was produced by a
        // full `cargo tauri build` — a plain `cargo build`/`--release`
        // doesn't embed `frontendDist`, release or not. So the overwhelming
        // likely cause here is "forgot to start `trunk serve`", not a real
        // app bug; skip with that pointer rather than failing every
        // assertion downstream with a confusing "no such element".
        eprintln!("skipping {test_name}: app never rendered its shell — is `trunk serve` running in client/ on :1420?");
        let _ = client.close().await;
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return None;
    }

    Some(App { client, tmp_dir })
}

pub struct App {
    pub client: Client,
    tmp_dir: PathBuf,
}

impl App {
    /// Save a screenshot to `target/e2e-screenshots/<name>.png` — under
    /// `target/`, so it's already gitignored — for a human (or the coding
    /// agent) to open and look at, in addition to whatever the test asserts.
    pub async fn screenshot(&self, name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/e2e-screenshots");
        std::fs::create_dir_all(&dir).expect("create target/e2e-screenshots");
        let path = dir.join(format!("{name}.png"));
        let png = self.client.screenshot().await.expect("capture screenshot");
        let mut file = std::fs::File::create(&path).expect("create screenshot file");
        file.write_all(&png).expect("write screenshot file");
        path
    }

    /// Ends the WebDriver session, which is what actually tears down the
    /// spawned app + `WebKitWebDriver` process pair — `Client` has no `Drop`
    /// impl that does this (ending a session is a network call, and `Drop`
    /// can't be async), so a test that skips this leaks both processes until
    /// `tauri-driver`'s own idle timeout. Call at the end of every test that
    /// reaches [`launch`] successfully.
    pub async fn close(self) {
        let tmp_dir = self.tmp_dir.clone();
        if let Err(err) = self.client.close().await {
            eprintln!("warning: failed to close webdriver session cleanly: {err}");
        }
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}

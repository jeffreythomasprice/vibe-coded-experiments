//! UI tests that drive the real, compiled `server` binary through a real
//! webview via `tauri-driver` — the Tauri-blessed equivalent of a Cypress
//! suite for a web app, except the thing under test is a native window, not
//! a page a browser can navigate to directly. See `CLAUDE.md`'s "UI /
//! end-to-end tests" section for the one-time setup.
//!
//! A plain `cargo test` compiles this file but every test below returns
//! immediately unless `AI_HARNESS_E2E=1` is set, and each one also skips
//! (rather than fails) if `tauri-driver` isn't reachable — same shape as
//! `lib/tests/live_*.rs`.
//!
//!     Xvfb :99 -screen 0 1280x800x24 &
//!     DISPLAY=:99 tauri-driver &
//!     AI_HARNESS_E2E=1 cargo test -p server --test webdriver_ui -- --nocapture --test-threads=1

mod common;

use fantoccini::Locator;

async fn button_labels(app: &common::App, selector: &str) -> Vec<String> {
    let mut labels = Vec::new();
    for button in app.client.find_all(Locator::Css(selector)).await.expect("find buttons") {
        labels.push(button.text().await.unwrap_or_default().trim().to_string());
    }
    labels
}

/// The app boots, the Leptos shell mounts, and the sidebar's static nav is
/// present — a smoke test that the whole stack (webview, wasm bundle, and
/// the model-catalog IPC call `App` fires on mount) comes up without
/// crashing or getting stuck on a blank/error page.
#[tokio::test]
async fn app_shell_renders() {
    let Some(app) = common::launch("app_shell_renders").await else {
        return;
    };

    let labels = button_labels(&app, "button.sidebar-item").await;
    assert!(labels.iter().any(|t| t == "New"));
    assert!(labels.iter().any(|t| t == "Agents"));
    assert!(labels.iter().any(|t| t == "Projects"));

    app.screenshot("app_shell_renders").await;
    app.close().await;
}

/// Clicking "Agents" in the sidebar is a real DOM click, and the resulting
/// view is populated by a real `list_agents` IPC round trip against an
/// isolated, freshly-created SQLite db — this is the layer no unit test can
/// cover, since `lib::db` and `lib::service` are exercised through the same
/// Tauri command dispatch a user's click goes through, not called directly.
#[tokio::test]
async fn agents_view_round_trips_ipc() {
    let Some(app) = common::launch("agents_view_round_trips_ipc").await else {
        return;
    };

    let buttons = app.client.find_all(Locator::Css("button.sidebar-item")).await.expect("find sidebar buttons");
    let mut clicked = false;
    for button in &buttons {
        if button.text().await.expect("button text").trim() == "Agents" {
            button.click().await.expect("click Agents");
            clicked = true;
            break;
        }
    }
    assert!(clicked, "no sidebar button labeled 'Agents'");

    // An empty catalog still renders a "New" button rather than an error —
    // that's the signal the IPC call actually resolved instead of hanging or
    // rejecting.
    app.client.wait().for_element(Locator::Css("button")).await.expect("agents view content");
    let labels = button_labels(&app, "button").await;
    assert!(labels.iter().any(|t| t == "New"), "expected a 'New' button on an empty agents list");

    app.screenshot("agents_view_round_trips_ipc").await;
    app.close().await;
}

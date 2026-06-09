use leptos::prelude::*;

use crate::ws::{self, ConnStatus};

/// HTTP base URL of the server, baked in at build time from `client/.env`.
pub const SERVER_HTTP_URL: &str = env!("SERVER_HTTP_URL");

#[component]
pub fn App() -> impl IntoView {
    let (status, set_status) = signal(ConnStatus::Connecting);

    // Kick off the WebSocket connection once on mount.
    ws::connect(set_status);

    view! {
        <main>
            <h1>"roll20-clone"</h1>
            <p>"server: " <code>{SERVER_HTTP_URL}</code></p>
            <p>"websocket: " <code>{ws::SERVER_WS_URL}</code></p>
            <p>"status: " <strong>{move || status.get().label()}</strong></p>
        </main>
    }
}

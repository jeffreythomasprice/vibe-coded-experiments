use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::api::SERVER_HTTP_URL;
use crate::views::{MapListView, MapView};
use crate::ws::{self, ConnStatus};

#[component]
pub fn App() -> impl IntoView {
    let (status, set_status) = signal(ConnStatus::Connecting);

    // A background connection purely for the header's status indicator. The map
    // view opens its own connection to follow a specific map.
    ws::connect(set_status);

    view! {
        <Router>
            <header class="topbar">
                <a href="/" class="brand">"roll20-clone"</a>
                <span class="conn">
                    "server: " <code>{SERVER_HTTP_URL}</code>
                    " · ws: " <strong>{move || status.get().label()}</strong>
                </span>
            </header>
            <div class="content">
                <Routes fallback=|| view! { <p class="pad">"Not found"</p> }>
                    <Route path=path!("/") view=MapListView/>
                    <Route path=path!("/maps/:id") view=MapView/>
                </Routes>
            </div>
        </Router>
    }
}

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

mod auth;
mod components;
mod pages;

fn main() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    tracing::info!("chess client starting");

    leptos::mount::mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    view! {
        <Router>
            <nav>
                <a href="/">"Home"</a>" | "<a href="/login">"Login"</a>
            </nav>
            <main>
                <Routes fallback=|| view! { <p>"Not found."</p> }>
                    <Route path=path!("/") view=ProtectedHome />
                    <Route path=path!("/login") view=pages::login::LoginPage />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn ProtectedHome() -> impl IntoView {
    view! {
        <components::auth_guard::AuthGuard>
            <pages::home::HomePage />
        </components::auth_guard::AuthGuard>
    }
}

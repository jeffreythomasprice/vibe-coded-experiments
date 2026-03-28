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
    let auth_state = auth::AuthState::new();
    provide_context(auth_state);

    view! {
        <Router>
            <NavBar />
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
fn NavBar() -> impl IntoView {
    let auth_state = expect_context::<auth::AuthState>();

    move || {
        if !auth_state.0.get() {
            return view! {}.into_any();
        }

        let navigate = leptos_router::hooks::use_navigate();
        let on_logout = move |ev: leptos::ev::MouseEvent| {
            ev.prevent_default();
            auth::remove_token();
            auth_state.set_authenticated(false);
            navigate("/login", Default::default());
        };

        view! {
            <nav>
                <a href="/">"Home"</a>" | "
                <a href="/login" on:click=on_logout>"Log Out"</a>
            </nav>
        }
        .into_any()
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

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

mod api;
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
                    <Route path=path!("/") view=RedirectToGames />
                    <Route path=path!("/login") view=pages::login::LoginPage />
                    <Route path=path!("/games") view=ProtectedGames />
                    <Route path=path!("/users") view=ProtectedUsers />
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn NavBar() -> impl IntoView {
    let auth_state = expect_context::<auth::AuthState>();

    move || {
        if !auth_state.authenticated.get() {
            return view! {}.into_any();
        }

        let navigate = leptos_router::hooks::use_navigate();
        let on_logout = move |ev: leptos::ev::MouseEvent| {
            ev.prevent_default();
            auth::remove_token();
            auth_state.set_authenticated(false);
            navigate("/login", Default::default());
        };

        let is_admin = auth_state.is_admin.get();

        view! {
            <nav>
                <a href="/games">"Games"</a>
                {if is_admin {
                    Some(view! { " | "<a href="/users">"Users"</a> })
                } else {
                    None
                }}
                " | "
                <a href="/login" on:click=on_logout>"Log Out"</a>
            </nav>
        }
        .into_any()
    }
}

#[component]
fn RedirectToGames() -> impl IntoView {
    let navigate = leptos_router::hooks::use_navigate();
    navigate("/games", Default::default());
    view! {}
}

#[component]
fn ProtectedGames() -> impl IntoView {
    view! {
        <components::auth_guard::AuthGuard>
            <pages::games::GamesPage />
        </components::auth_guard::AuthGuard>
    }
}

#[component]
fn ProtectedUsers() -> impl IntoView {
    view! {
        <components::auth_guard::AuthGuard>
            <pages::users::UsersPage />
        </components::auth_guard::AuthGuard>
    }
}

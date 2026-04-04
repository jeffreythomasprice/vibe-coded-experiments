use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

mod api;
mod auth;
mod components;
mod pages;
pub mod theme;

fn main() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    tracing::info!("chess client starting");

    leptos::mount::mount_to_body(App);
}

#[derive(Clone, Copy)]
pub struct ToastState {
    pub message: RwSignal<Option<String>>,
}

#[component]
fn Toast() -> impl IntoView {
    let toast = expect_context::<ToastState>();

    move || {
        toast.message.get().map(|msg| {
            let dismiss = move |_: leptos::ev::MouseEvent| toast.message.set(None);
            let t = toast;
            gloo_timers::callback::Timeout::new(5_000, move || {
                t.message.set(None);
            })
            .forget();
            view! {
                <div
                    on:click=dismiss
                    style="position:fixed; top:1em; right:1em; background:#d32f2f; color:white; \
                           padding:0.75em 1.25em; border-radius:4px; z-index:9999; cursor:pointer; \
                           box-shadow: 0 2px 8px rgba(0,0,0,0.3);"
                >
                    {msg}
                </div>
            }
        })
    }
}

#[component]
fn App() -> impl IntoView {
    let auth_state = auth::AuthState::new();
    provide_context(auth_state);

    let toast_state = ToastState {
        message: RwSignal::new(None),
    };
    provide_context(toast_state);

    let theme_state = theme::ThemeState::new();
    provide_context(theme_state);

    view! {
        <Router>
            <Toast />
            <NavBar />
            <main>
                <Routes fallback=|| view! { <p>"Not found."</p> }>
                    <Route path=path!("/") view=RedirectToGames />
                    <Route path=path!("/login") view=pages::login::LoginPage />
                    <Route path=path!("/games") view=ProtectedGames />
                    <Route path=path!("/game/:id") view=ProtectedGameView />
                    <Route path=path!("/users") view=ProtectedUsers />
                    <Route path=path!("/settings") view=ProtectedSettings />
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
            return ().into_any();
        }

        let navigate = leptos_router::hooks::use_navigate();
        let on_logout = move |ev: leptos::ev::MouseEvent| {
            ev.prevent_default();
            auth::remove_token();
            theme::clear_storage();
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
                <a href="/settings">"Settings"</a>
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
fn ProtectedGameView() -> impl IntoView {
    view! {
        <components::auth_guard::AuthGuard>
            <pages::game_view::GameViewPage />
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

#[component]
fn ProtectedSettings() -> impl IntoView {
    view! {
        <components::auth_guard::AuthGuard>
            <pages::settings::SettingsPage />
        </components::auth_guard::AuthGuard>
    }
}

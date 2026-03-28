use leptos::prelude::*;
use leptos::ev::SubmitEvent;
use leptos_router::hooks::use_navigate;

#[component]
pub fn LoginPage() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);
    let navigate = use_navigate();

    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let username_val = username.get_untracked();
        let password_val = password.get_untracked();

        if username_val.is_empty() || password_val.is_empty() {
            set_error.set(Some("Username and password are required.".into()));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            let body = serde_json::json!({
                "username": username_val,
                "password": password_val,
            });

            let result = gloo_net::http::Request::post("/api/login")
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&body).unwrap())
                .unwrap()
                .send()
                .await;

            set_loading.set(false);

            match result {
                Ok(resp) if resp.ok() => {
                    match resp.json::<chess_shared::LoginResponse>().await {
                        Ok(login_resp) => {
                            crate::auth::set_token(&login_resp.token);
                            navigate("/", Default::default());
                        }
                        Err(_) => {
                            set_error.set(Some("Unexpected response from server.".into()));
                        }
                    }
                }
                Ok(resp) if resp.status() == 401 => {
                    set_error.set(Some("Invalid username or password.".into()));
                }
                _ => {
                    set_error.set(Some("Login failed. Please try again.".into()));
                }
            }
        });
    };

    view! {
        <h1>"Login"</h1>
        {move || error.get().map(|e| view! { <p style="color:red">{e}</p> })}
        <form on:submit=on_submit>
            <div>
                <label for="username">"Username: "</label>
                <input
                    id="username"
                    type="text"
                    on:input=move |ev| set_username.set(event_target_value(&ev))
                    prop:value=username
                />
            </div>
            <div>
                <label for="password">"Password: "</label>
                <input
                    id="password"
                    type="password"
                    on:input=move |ev| set_password.set(event_target_value(&ev))
                    prop:value=password
                />
            </div>
            <button type="submit" disabled=loading>"Log in"</button>
        </form>
    }
}

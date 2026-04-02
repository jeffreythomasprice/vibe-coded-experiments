use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen::JsCast;

#[derive(Clone, Debug, Deserialize)]
struct UserSummary {
    id: String,
    username: String,
    is_admin: bool,
    created_at: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ListUsersResponse {
    users: Vec<UserSummary>,
    next_page_token: Option<String>,
}

#[derive(Clone, PartialEq)]
enum ViewMode {
    List,
    Create,
    Edit(String),
}

#[component]
pub fn UsersPage() -> impl IntoView {
    let (view_mode, set_view_mode) = signal(ViewMode::List);
    let (refresh_counter, set_refresh_counter) = signal(0u32);

    let on_back_to_list = move || {
        set_refresh_counter.update(|c| *c += 1);
        set_view_mode.set(ViewMode::List);
    };

    move || match view_mode.get() {
        ViewMode::List => {
            view! {
                <UsersList
                    refresh_counter=refresh_counter
                    on_create=move || set_view_mode.set(ViewMode::Create)
                    on_edit=move |username: String| set_view_mode.set(ViewMode::Edit(username))
                />
            }
            .into_any()
        }
        ViewMode::Create => {
            let on_back = on_back_to_list;
            view! { <CreateUserForm on_done=move || on_back() /> }.into_any()
        }
        ViewMode::Edit(ref username) => {
            let username = username.clone();
            let on_back = on_back_to_list;
            view! { <EditUserForm username=username on_done=move || on_back() /> }.into_any()
        }
    }
}

fn get_current_username() -> String {
    crate::auth::get_token()
        .and_then(|t| {
            #[derive(Deserialize)]
            struct Claims {
                username: String,
            }
            let payload_b64url = t.split('.').nth(1)?;
            let b64: String = payload_b64url
                .chars()
                .map(|c| match c {
                    '-' => '+',
                    '_' => '/',
                    other => other,
                })
                .collect();
            let padded = match b64.len() % 4 {
                2 => format!("{b64}=="),
                3 => format!("{b64}="),
                _ => b64,
            };
            let decoded = web_sys::window()?.atob(&padded).ok()?;
            let claims: Claims = serde_json::from_str(&decoded).ok()?;
            Some(claims.username)
        })
        .unwrap_or_default()
}

#[component]
fn UsersList(
    refresh_counter: ReadSignal<u32>,
    on_create: impl Fn() + Send + 'static + Clone,
    on_edit: impl Fn(String) + Send + 'static + Clone,
) -> impl IntoView {
    let auth_state = expect_context::<crate::auth::AuthState>();
    let (users, set_users) = signal(Vec::<UserSummary>::new());
    let (next_token, set_next_token) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);
    let (deleting, set_deleting) = signal(Option::<String>::None);
    let sentinel_ref = NodeRef::<leptos::html::Div>::new();

    let fetch_page = move |token: Option<String>, append: bool| {
        set_loading.set(true);
        leptos::task::spawn_local(async move {
            let mut url = "/api/users?page_size=20".to_string();
            if let Some(ref t) = token {
                url.push_str(&format!("&page_token={t}"));
            }
            let Ok(resp) = crate::api::get(&url).build().unwrap().send().await else {
                set_loading.set(false);
                return;
            };
            set_loading.set(false);
            if resp.ok() {
                if let Ok(data) = resp.json::<ListUsersResponse>().await {
                    if append {
                        set_users.update(|u| u.extend(data.users));
                    } else {
                        set_users.set(data.users);
                    }
                    set_next_token.set(data.next_page_token);
                }
            }
        });
    };

    // Initial load and reload on refresh_counter change
    Effect::new(move |_| {
        let _ = refresh_counter.get();
        set_users.set(Vec::new());
        set_next_token.set(None);
        fetch_page(None, false);
    });

    // Infinite scroll via IntersectionObserver
    Effect::new(move |_| {
        let Some(sentinel) = sentinel_ref.get() else {
            return;
        };
        let element: &web_sys::Element = &sentinel;

        let fetch_next = move || {
            if loading.get_untracked() {
                return;
            }
            if let Some(token) = next_token.get_untracked() {
                fetch_page(Some(token), true);
            }
        };

        let closure =
            wasm_bindgen::closure::Closure::<dyn Fn(js_sys::Array)>::new(move |entries: js_sys::Array| {
                for entry in entries.iter() {
                    let entry: web_sys::IntersectionObserverEntry =
                        entry.unchecked_into();
                    if entry.is_intersecting() {
                        fetch_next();
                    }
                }
            });

        let options = web_sys::IntersectionObserverInit::new();
        options.set_threshold(&wasm_bindgen::JsValue::from_f64(0.1));

        let observer =
            web_sys::IntersectionObserver::new_with_options(
                closure.as_ref().unchecked_ref(),
                &options,
            )
            .unwrap();

        observer.observe(element);

        closure.forget();

        on_cleanup(move || {
            observer.disconnect();
        });
    });

    let do_delete = move |username: String| {
        set_deleting.set(None);
        leptos::task::spawn_local(async move {
            let Ok(resp) = crate::api::delete(&format!("/api/users/{username}"))
                .build()
                .unwrap()
                .send()
                .await
            else {
                return;
            };
            if resp.ok() {
                set_users.update(|u| u.retain(|user| user.username != username));
            }
        });
    };

    let on_create_clone = on_create.clone();
    let is_admin = auth_state.is_admin;
    let current_username = get_current_username();

    view! {
        <h1>"Users"</h1>
        {move || is_admin.get().then(|| {
            let on_create = on_create_clone.clone();
            view! { <button on:click=move |_| on_create()>"New User"</button> }
        })}
        <table>
            <thead>
                <tr>
                    <th>"Username"</th>
                    <th>"Admin"</th>
                    <th>"Created At"</th>
                    <th>"Actions"</th>
                </tr>
            </thead>
            <tbody>
                <For
                    each=move || users.get()
                    key=|u| u.id.clone()
                    let:user
                >
                    {
                        let on_edit = on_edit.clone();
                        let current_username = current_username.clone();
                        let user_username = user.username.clone();
                        let user_created = user.created_at.clone();
                        let user_is_admin = user.is_admin;
                        let username_for_edit = user.username.clone();
                        let username_for_delete = user.username.clone();
                        let is_self = user.username == current_username;
                        view! {
                            <tr>
                                <td>{user_username}</td>
                                <td>{if user_is_admin { "Yes" } else { "No" }}</td>
                                <td>{user_created}</td>
                                {move || {
                                    let show_edit = is_admin.get() || is_self;
                                    let show_delete = is_admin.get() && !is_self;
                                    if !show_edit && !show_delete {
                                        return None;
                                    }
                                    let on_edit = on_edit.clone();
                                    let username_for_edit = username_for_edit.clone();
                                    let username_for_delete = username_for_delete.clone();
                                    Some(view! {
                                        <td>
                                            {show_edit.then(|| {
                                                let on_edit = on_edit.clone();
                                                let username_for_edit = username_for_edit.clone();
                                                view! {
                                                    <button on:click=move |_| {
                                                        on_edit(username_for_edit.clone());
                                                    }>"Edit"</button>
                                                }
                                            })}
                                            {show_delete.then(|| {
                                                let uname = username_for_delete.clone();
                                                view! {
                                                    " "
                                                    <button on:click=move |_| {
                                                        set_deleting.set(Some(uname.clone()));
                                                    }>"Delete"</button>
                                                }
                                            })}
                                        </td>
                                    })
                                }}
                            </tr>
                        }
                    }
                </For>
            </tbody>
        </table>
        <div node_ref=sentinel_ref style="height: 1px;"></div>
        {move || loading.get().then(|| view! {
            <div style="text-align: center; padding: 1em;">
                <span>"Loading..."</span>
            </div>
        })}
        {move || deleting.get().map(|username| {
            let username_confirm = username.clone();
            let username_display = username.clone();
            view! {
                <DeleteConfirmModal
                    username=username_display
                    on_confirm=move || do_delete(username_confirm.clone())
                    on_cancel=move || set_deleting.set(None)
                />
            }
        })}
    }
}

#[component]
fn CreateUserForm(on_done: impl Fn() + 'static + Clone) -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (is_admin, set_is_admin) = signal(false);
    let (show_password, set_show_password) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let on_done_clone = on_done.clone();
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let username_val = username.get_untracked();
        let password_val = password.get_untracked();

        if username_val.is_empty() || password_val.is_empty() {
            set_error.set(Some("Username and password are required.".into()));
            return;
        }

        set_loading.set(true);
        set_error.set(None);

        let admin_val = is_admin.get_untracked();
        let on_done = on_done_clone.clone();
        leptos::task::spawn_local(async move {
            let body = serde_json::json!({
                "username": username_val,
                "password": password_val,
                "is_admin": admin_val,
            });

            let result = crate::api::post("/api/users")
                .json(&body)
                .unwrap()
                .send()
                .await;

            set_loading.set(false);

            match result {
                Ok(resp) if resp.ok() => {
                    on_done();
                }
                Ok(resp) if resp.status() == 409 => {
                    set_error.set(Some("Username already taken.".into()));
                }
                _ => {
                    set_error.set(Some("Failed to create user.".into()));
                }
            }
        });
    };

    view! {
        <h2>"New User"</h2>
        {move || error.get().map(|e| view! { <p style="color:red">{e}</p> })}
        <form on:submit=on_submit autocomplete="off">
            <div>
                <label for="new-username">"Username: "</label>
                <input
                    id="new-username"
                    type="text"
                    autocomplete="off"
                    on:input=move |ev| set_username.set(event_target_value(&ev))
                    prop:value=username
                />
            </div>
            <div style="display:flex;align-items:center;gap:0.25em">
                <label for="new-password">"Password: "</label>
                <input
                    id="new-password"
                    type=move || if show_password.get() { "text" } else { "password" }
                    autocomplete="new-password"
                    on:input=move |ev| set_password.set(event_target_value(&ev))
                    prop:value=password
                />
                <button
                    type="button"
                    title="Toggle password visibility"
                    style="background:none;border:none;cursor:pointer;padding:0;font-size:1.1em"
                    on:click=move |_| set_show_password.update(|v| *v = !*v)
                >
                    {move || if show_password.get() { "\u{1F441}" } else { "\u{1F441}\u{200D}\u{1F5E8}" }}
                </button>
            </div>
            <div>
                <label>
                    <input
                        type="checkbox"
                        prop:checked=is_admin
                        on:change=move |ev| set_is_admin.set(event_target_checked(&ev))
                    />
                    " Admin"
                </label>
            </div>
            <button type="submit" disabled=loading>"Create"</button>
            " "
            <button type="button" on:click=move |_| on_done()>"Cancel"</button>
        </form>
    }
}

#[component]
fn EditUserForm(username: String, on_done: impl Fn() + 'static + Clone) -> impl IntoView {
    let auth_state = expect_context::<crate::auth::AuthState>();
    let (is_admin, set_is_admin) = signal(false);
    let (password, set_password) = signal(String::new());
    let (show_password, set_show_password) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(true);
    let (saving, set_saving) = signal(false);
    let username_clone = username.clone();
    // Load current user data
    leptos::task::spawn_local({
        let username = username.clone();
        async move {
            let Ok(resp) = crate::api::get(&format!("/api/users/{username}"))
                .build()
                .unwrap()
                .send()
                .await
            else {
                set_loading.set(false);
                return;
            };
            set_loading.set(false);
            if resp.ok() {
                if let Ok(user) = resp.json::<UserSummary>().await {
                    set_is_admin.set(user.is_admin);
                }
            }
        }
    });

    let on_done_clone = on_done.clone();
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        set_saving.set(true);
        set_error.set(None);

        let username = username_clone.clone();
        let admin_val = is_admin.get_untracked();
        let password_val = password.get_untracked();
        let caller_is_admin = auth_state.is_admin.get_untracked();
        let on_done = on_done_clone.clone();
        leptos::task::spawn_local(async move {
            let mut body = serde_json::Map::new();
            if caller_is_admin {
                body.insert("is_admin".into(), serde_json::json!(admin_val));
            }
            if !password_val.is_empty() {
                body.insert("password".into(), serde_json::json!(password_val));
            }

            if body.is_empty() {
                set_saving.set(false);
                set_error.set(Some("No changes to save.".into()));
                return;
            }

            let result = crate::api::put(&format!("/api/users/{username}"))
                .json(&serde_json::Value::Object(body))
                .unwrap()
                .send()
                .await;

            set_saving.set(false);

            match result {
                Ok(resp) if resp.ok() => {
                    on_done();
                }
                Ok(resp) if resp.status() == 403 => {
                    set_error.set(Some("You cannot change your own admin status.".into()));
                }
                _ => {
                    set_error.set(Some("Failed to update user.".into()));
                }
            }
        });
    };

    let display_username = username.clone();
    let caller_is_admin = auth_state.is_admin;
    view! {
        <h2>"Edit User: " {display_username}</h2>
        {move || loading.get().then(|| view! { <p>"Loading..."</p> })}
        {move || error.get().map(|e| view! { <p style="color:red">{e}</p> })}
        <form on:submit=on_submit autocomplete="off">
            {move || caller_is_admin.get().then(|| view! {
                <div>
                    <label>
                        <input
                            type="checkbox"
                            prop:checked=is_admin
                            on:change=move |ev| set_is_admin.set(event_target_checked(&ev))
                        />
                        " Admin"
                    </label>
                </div>
            })}
            <div style="display:flex;align-items:center;gap:0.25em">
                <label for="edit-password">"New Password: "</label>
                <input
                    id="edit-password"
                    type=move || if show_password.get() { "text" } else { "password" }
                    autocomplete="new-password"
                    placeholder="Leave blank to keep current"
                    on:input=move |ev| set_password.set(event_target_value(&ev))
                    prop:value=password
                />
                <button
                    type="button"
                    title="Toggle password visibility"
                    style="background:none;border:none;cursor:pointer;padding:0;font-size:1.1em"
                    on:click=move |_| set_show_password.update(|v| *v = !*v)
                >
                    {move || if show_password.get() { "\u{1F441}" } else { "\u{1F441}\u{200D}\u{1F5E8}" }}
                </button>
            </div>
            <button type="submit" disabled=move || loading.get() || saving.get()>"Save"</button>
            " "
            <button type="button" on:click=move |_| on_done()>"Cancel"</button>
        </form>
    }
}

#[component]
fn DeleteConfirmModal(
    username: String,
    on_confirm: impl Fn() + 'static,
    on_cancel: impl Fn() + 'static,
) -> impl IntoView {
    let display_username = username.clone();
    view! {
        <div style="position:fixed; top:0; left:0; right:0; bottom:0; background:rgba(0,0,0,0.5); display:flex; align-items:center; justify-content:center; z-index:1000;">
            <div style="background:white; padding:2em; border-radius:8px; max-width:400px;">
                <p>"Are you sure you want to delete user " <strong>{display_username}</strong> "?"</p>
                <button on:click=move |_| on_confirm()>"Confirm Delete"</button>
                " "
                <button on:click=move |_| on_cancel()>"Cancel"</button>
            </div>
        </div>
    }
}

fn event_target_checked(ev: &leptos::ev::Event) -> bool {
    ev.target()
        .unwrap()
        .unchecked_into::<web_sys::HtmlInputElement>()
        .checked()
}

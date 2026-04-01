use leptos::prelude::*;
use serde::Deserialize;
use wasm_bindgen::JsCast;

use crate::components::username_autocomplete::UsernameAutocomplete;

#[derive(Clone, Debug, Deserialize)]
struct GameSummary {
    id: String,
    variant: String,
    status: String,
    player_white_name: String,
    player_white_kind: String,
    player_black_name: String,
    player_black_kind: String,
    active_color: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ListGamesResponse {
    games: Vec<GameSummary>,
    next_page_token: Option<String>,
}

#[derive(Clone, PartialEq)]
enum ViewMode {
    List,
    Create,
}

#[component]
pub fn GamesPage() -> impl IntoView {
    let (view_mode, set_view_mode) = signal(ViewMode::List);
    let (refresh_counter, set_refresh_counter) = signal(0u32);

    let on_back_to_list = move || {
        set_refresh_counter.update(|c| *c += 1);
        set_view_mode.set(ViewMode::List);
    };

    move || match view_mode.get() {
        ViewMode::List => {
            let set_view_mode = set_view_mode.clone();
            view! {
                <GamesList
                    refresh_counter=refresh_counter
                    on_create=move || set_view_mode.set(ViewMode::Create)
                />
            }
            .into_any()
        }
        ViewMode::Create => {
            let on_back = on_back_to_list.clone();
            view! { <CreateGameForm on_done=move || on_back() /> }.into_any()
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

fn display_status(game: &GameSummary) -> String {
    match game.status.as_str() {
        "waiting" | "active" | "check" => "In Progress".to_string(),
        "checkmate" => {
            // The player whose turn it is has been checkmated, so the other player wins
            let winner = match game.active_color.as_deref() {
                Some("white") => &game.player_black_name,
                _ => &game.player_white_name,
            };
            format!("Victory for {winner}")
        }
        "resigned" => {
            let winner = match game.active_color.as_deref() {
                Some("white") => &game.player_black_name,
                _ => &game.player_white_name,
            };
            format!("Victory for {winner}")
        }
        "stalemate" | "draw" => "Draw".to_string(),
        "abandoned" => "Abandoned".to_string(),
        other => other.to_string(),
    }
}

fn display_variant(variant: &str) -> &str {
    match variant {
        "standard" => "Standard",
        "forced_capture_lose_all" => "Forced Capture (Lose All)",
        "forced_capture_checkmate" => "Forced Capture (Checkmate)",
        _ => variant,
    }
}

#[component]
fn GamesList(
    refresh_counter: ReadSignal<u32>,
    on_create: impl Fn() + Send + 'static + Clone,
) -> impl IntoView {
    let auth_state = expect_context::<crate::auth::AuthState>();
    let (games, set_games) = signal(Vec::<GameSummary>::new());
    let (next_token, set_next_token) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);
    let (deleting, set_deleting) = signal(Option::<String>::None);
    let sentinel_ref = NodeRef::<leptos::html::Div>::new();

    let fetch_page = move |token: Option<String>, append: bool| {
        set_loading.set(true);
        leptos::task::spawn_local(async move {
            let mut url = "/api/games?page_size=20".to_string();
            if let Some(ref t) = token {
                url.push_str(&format!("&page_token={t}"));
            }
            let Ok(resp) = crate::api::get(&url).build().unwrap().send().await else {
                set_loading.set(false);
                return;
            };
            set_loading.set(false);
            if resp.ok() {
                if let Ok(data) = resp.json::<ListGamesResponse>().await {
                    if append {
                        set_games.update(|g| g.extend(data.games));
                    } else {
                        set_games.set(data.games);
                    }
                    set_next_token.set(data.next_page_token);
                }
            }
        });
    };

    // Initial load and reload on refresh_counter change
    Effect::new(move |_| {
        let _ = refresh_counter.get();
        set_games.set(Vec::new());
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

    let do_delete = move |game_id: String| {
        set_deleting.set(None);
        leptos::task::spawn_local(async move {
            let Ok(resp) = crate::api::delete(&format!("/api/games/{game_id}"))
                .build()
                .unwrap()
                .send()
                .await
            else {
                return;
            };
            if resp.ok() {
                set_games.update(|g| g.retain(|game| game.id != game_id));
            }
        });
    };

    let on_create_clone = on_create.clone();
    let is_admin = auth_state.is_admin;

    view! {
        <h1>"Games"</h1>
        <button on:click=move |_| on_create_clone()>"New Game"</button>
        <table>
            <thead>
                <tr>
                    <th>"White"</th>
                    <th>"Black"</th>
                    <th>"Variant"</th>
                    <th>"Status"</th>
                    <th>"Created"</th>
                    <th>"Updated"</th>
                    <th>"Actions"</th>
                </tr>
            </thead>
            <tbody>
                <For
                    each=move || games.get()
                    key=|g| g.id.clone()
                    let:game
                >
                    {
                        let pw = format!("{} ({})", game.player_white_name, game.player_white_kind);
                        let pb = format!("{} ({})", game.player_black_name, game.player_black_kind);
                        let variant = display_variant(&game.variant).to_string();
                        let status = display_status(&game);
                        let created = game.created_at.clone();
                        let updated = game.updated_at.clone();
                        let game_id_for_delete = game.id.clone();
                        let game_id_for_link = game.id.clone();
                        view! {
                            <tr>
                                <td>{pw}</td>
                                <td>{pb}</td>
                                <td>{variant}</td>
                                <td>{status}</td>
                                <td>{created}</td>
                                <td>{updated}</td>
                                <td>
                                    <a href={format!("/game/{}", game_id_for_link)}>"View"</a>
                                    {move || is_admin.get().then(|| {
                                        let gid = game_id_for_delete.clone();
                                        view! {
                                            " "
                                            <button on:click=move |_| {
                                                set_deleting.set(Some(gid.clone()));
                                            }>"Delete"</button>
                                        }
                                    })}
                                </td>
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
        {move || deleting.get().map(|game_id| {
            let game_id_confirm = game_id.clone();
            let game_id_display = game_id.clone();
            view! {
                <DeleteConfirmModal
                    game_id=game_id_display
                    on_confirm=move || do_delete(game_id_confirm.clone())
                    on_cancel=move || set_deleting.set(None)
                />
            }
        })}
    }
}

#[component]
fn CreateGameForm(on_done: impl Fn() + 'static + Clone) -> impl IntoView {
    let auth_state = expect_context::<crate::auth::AuthState>();
    let current_username = get_current_username();
    let is_admin = auth_state.is_admin.get_untracked();

    let (variant, set_variant) = signal("standard".to_string());
    let (white_kind, set_white_kind) = signal("human".to_string());
    let white_username = RwSignal::new(if is_admin {
        String::new()
    } else {
        current_username.clone()
    });
    let (black_kind, set_black_kind) = signal("human".to_string());
    let black_username = RwSignal::new(String::new());
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    let current_username_for_validate = current_username.clone();
    let on_done_clone = on_done.clone();
    let navigate = leptos_router::hooks::use_navigate();
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let variant_val = variant.get_untracked();
        let wk = white_kind.get_untracked();
        let wu = white_username.get_untracked();
        let bk = black_kind.get_untracked();
        let bu = black_username.get_untracked();

        // Validate human participants have usernames
        if wk == "human" && wu.trim().is_empty() {
            set_error.set(Some("White player username is required.".into()));
            return;
        }
        if bk == "human" && bu.trim().is_empty() {
            set_error.set(Some("Black player username is required.".into()));
            return;
        }

        // Non-admin must be a participant
        if !is_admin {
            let is_white = wk == "human" && wu == current_username_for_validate;
            let is_black = bk == "human" && bu == current_username_for_validate;
            if !is_white && !is_black {
                set_error.set(Some("You must be one of the participants.".into()));
                return;
            }
        }

        set_loading.set(true);
        set_error.set(None);

        let on_done = on_done_clone.clone();
        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            let mut pw = serde_json::json!({ "kind": wk });
            if wk == "human" {
                pw.as_object_mut().unwrap().insert("username".into(), serde_json::json!(wu));
            }
            let mut pb = serde_json::json!({ "kind": bk });
            if bk == "human" {
                pb.as_object_mut().unwrap().insert("username".into(), serde_json::json!(bu));
            }

            let body = serde_json::json!({
                "variant": variant_val,
                "player_white": pw,
                "player_black": pb,
            });

            let result = crate::api::post("/api/games")
                .json(&body)
                .unwrap()
                .send()
                .await;

            set_loading.set(false);

            match result {
                Ok(resp) if resp.ok() => {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        if let Some(id) = body.get("id").and_then(|v| v.as_str()) {
                            navigate(&format!("/game/{id}"), Default::default());
                            return;
                        }
                    }
                    // Fallback if we can't parse the ID
                    on_done();
                }
                Ok(resp) => {
                    if let Ok(err_body) = resp.json::<serde_json::Value>().await {
                        let msg = err_body
                            .get("message")
                            .and_then(|m| m.as_str())
                            .or_else(|| err_body.get("error").and_then(|e| e.as_str()))
                            .unwrap_or("Failed to create game.");
                        set_error.set(Some(msg.to_string()));
                    } else {
                        set_error.set(Some("Failed to create game.".into()));
                    }
                }
                Err(_) => {
                    set_error.set(Some("Failed to create game.".into()));
                }
            }
        });
    };

    let do_swap = move |_: leptos::ev::MouseEvent| {
        let wk = white_kind.get_untracked();
        let wu = white_username.get_untracked();
        let bk = black_kind.get_untracked();
        let bu = black_username.get_untracked();
        set_white_kind.set(bk);
        white_username.set(bu);
        set_black_kind.set(wk);
        black_username.set(wu);
    };

    view! {
        <h2>"New Game"</h2>
        {move || error.get().map(|e| view! { <p style="color:red">{e}</p> })}
        <form on:submit=on_submit>
            <div>
                <label for="variant">"Variant: "</label>
                <select
                    id="variant"
                    on:change=move |ev| set_variant.set(event_target_value(&ev))
                    prop:value=variant
                >
                    <option value="standard">"Standard"</option>
                    <option value="forced_capture_lose_all">"Forced Capture (Lose All)"</option>
                    <option value="forced_capture_checkmate">"Forced Capture (Checkmate)"</option>
                </select>
            </div>

            <fieldset>
                <legend>"White"</legend>
                <div>
                    <label for="white-kind">"Type: "</label>
                    <select
                        id="white-kind"
                        on:change=move |ev| {
                            let val = event_target_value(&ev);
                            set_white_kind.set(val.clone());
                            if val == "ai" {
                                white_username.set(String::new());
                            }
                        }
                        prop:value=white_kind
                    >
                        <option value="human">"Human"</option>
                        <option value="ai">"AI"</option>
                    </select>
                </div>
                {move || (white_kind.get() == "human").then(|| view! {
                    <div>
                        <label for="white-username">"Username: "</label>
                        <UsernameAutocomplete value=white_username id="white-username" />
                    </div>
                })}
            </fieldset>

            <div style="text-align: center; margin: 0.5em 0;">
                <button type="button" on:click=do_swap title="Swap white and black">"Swap"</button>
            </div>

            <fieldset>
                <legend>"Black"</legend>
                <div>
                    <label for="black-kind">"Type: "</label>
                    <select
                        id="black-kind"
                        on:change=move |ev| {
                            let val = event_target_value(&ev);
                            set_black_kind.set(val.clone());
                            if val == "ai" {
                                black_username.set(String::new());
                            }
                        }
                        prop:value=black_kind
                    >
                        <option value="human">"Human"</option>
                        <option value="ai">"AI"</option>
                    </select>
                </div>
                {move || (black_kind.get() == "human").then(|| view! {
                    <div>
                        <label for="black-username">"Username: "</label>
                        <UsernameAutocomplete value=black_username id="black-username" />
                    </div>
                })}
            </fieldset>

            <div style="margin-top: 1em;">
                <button type="submit" disabled=loading>"Create Game"</button>
                " "
                <button type="button" on:click=move |_| on_done()>"Cancel"</button>
            </div>
        </form>
    }
}

#[component]
fn DeleteConfirmModal(
    game_id: String,
    on_confirm: impl Fn() + 'static,
    on_cancel: impl Fn() + 'static,
) -> impl IntoView {
    let display_id = game_id.clone();
    view! {
        <div style="position:fixed; top:0; left:0; right:0; bottom:0; background:rgba(0,0,0,0.5); display:flex; align-items:center; justify-content:center; z-index:1000;">
            <div style="background:white; padding:2em; border-radius:8px; max-width:400px;">
                <p>"Are you sure you want to delete game " <strong>{display_id}</strong> "?"</p>
                <button on:click=move |_| on_confirm()>"Confirm Delete"</button>
                " "
                <button on:click=move |_| on_cancel()>"Cancel"</button>
            </div>
        </div>
    }
}

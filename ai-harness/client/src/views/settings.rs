//! Settings: the theme picker, a read-only view of whichever theme is
//! currently selected, and — for a user-authored selection — Edit and
//! Delete; Clone is offered for any selection. Read-only by default; Edit
//! and Clone swap the body for a `ThemeForm`, the same inline-swap pattern
//! `views::Agents` uses for its `AgentForm`.

use leptos::prelude::*;
use shared::project::SandboxStatus;
use shared::theme::{BuiltIn, Theme, ThemeChoice, ThemeField, UserTheme};
use wasm_bindgen_futures::spawn_local;

use crate::commands;
use crate::theme::{use_theme, ThemeContext};
use crate::views::{ThemeForm, ThemeFormMode};

/// What the lower half of the page is showing.
#[derive(Debug, Clone)]
enum Body {
    /// The read-only palette for whatever is currently selected.
    View,
    /// Cloning `source` into a brand-new theme.
    Clone(Theme),
    /// Editing the currently-selected user theme in place.
    Edit(UserTheme),
}

/// The `UserTheme` row backing the current selection, if the selection is a
/// user theme at all — `None` for System or a built-in, and also for a
/// `User` choice whose row hasn't loaded yet or was deleted elsewhere.
fn selected_user_theme(ctx: ThemeContext) -> Option<UserTheme> {
    match ctx.choice.get() {
        ThemeChoice::User(id) => ctx.user_themes.get().into_iter().find(|t| t.id == id),
        _ => None,
    }
}

#[component]
pub fn Settings() -> impl IntoView {
    let theme = use_theme();
    let body = RwSignal::new(Body::View);
    let confirming_delete = RwSignal::new(false);
    let delete_error = RwSignal::new(None::<String>);
    let sandbox = RwSignal::new(None::<SandboxStatus>);

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(status) = commands::sandbox_status().await {
                sandbox.set(Some(status));
            }
        });
    });

    let on_change = move |ev: leptos::ev::Event| {
        theme.set_choice(ThemeChoice::from_stored(Some(&event_target_value(&ev))));
        body.set(Body::View);
        confirming_delete.set(false);
    };

    let on_saved = move |saved: UserTheme| {
        theme.reload_user_themes();
        theme.set_choice(ThemeChoice::User(saved.id));
        body.set(Body::View);
    };
    let on_cancel = move |_| body.set(Body::View);

    let delete = move || {
        let Some(existing) = selected_user_theme(theme) else { return };
        delete_error.set(None);
        spawn_local(async move {
            match commands::delete_theme(existing.id).await {
                Ok(()) => {
                    theme.reload_user_themes();
                    theme.set_choice(ThemeChoice::System);
                    confirming_delete.set(false);
                }
                Err(err) => delete_error.set(Some(err.message)),
            }
        });
    };

    view! {
        <div class="settings-view">
            <h1>"Settings"</h1>

            <label class="settings-field">
                <span>"Theme"</span>
                <select prop:value=move || theme.choice.get().to_stored() on:change=on_change>
                    <option value="system" selected=move || theme.choice.get() == ThemeChoice::System>
                        "System"
                    </option>
                    <optgroup label="Built-in">
                        <For each=|| BuiltIn::ALL key=|built_in| *built_in let:built_in>
                            <option
                                value=built_in.id()
                                selected=move || theme.choice.get() == ThemeChoice::BuiltIn(built_in)
                            >
                                {built_in.label()}
                            </option>
                        </For>
                    </optgroup>
                    {move || {
                        (!theme.user_themes.get().is_empty())
                            .then(|| {
                                view! {
                                    <optgroup label="My themes">
                                        <For each=move || theme.user_themes.get() key=|t| t.id let:user_theme>
                                            <option
                                                value=ThemeChoice::User(user_theme.id).to_stored()
                                                selected=move || {
                                                    theme.choice.get() == ThemeChoice::User(user_theme.id)
                                                }
                                            >
                                                {user_theme.name.clone()}
                                            </option>
                                        </For>
                                    </optgroup>
                                }
                            })
                    }}
                </select>
            </label>

            {move || {
                (theme.choice.get() == ThemeChoice::System)
                    .then(|| {
                        let following = if theme.system_dark.get() { "dark" } else { "light" };
                        view! {
                            <p class="muted">
                                {format!("Following the system appearance: {following}.")}
                            </p>
                        }
                    })
            }}

            {move || theme.error.get().map(|message| view! { <p class="error">{message}</p> })}

            {move || match body.get() {
                Body::View => {
                    view! {
                        <ThemePalette
                            theme=theme.theme.get()
                            editable=selected_user_theme(theme)
                            on_clone=move |()| body.set(Body::Clone(theme.theme.get_untracked()))
                            on_edit=move |existing| body.set(Body::Edit(existing))
                            confirming_delete=confirming_delete
                            on_delete=move |()| delete()
                            delete_error=delete_error
                        />
                    }
                        .into_any()
                }
                Body::Clone(source) => {
                    view! {
                        <ThemeForm
                            mode=ThemeFormMode::Create { source }
                            on_saved=on_saved
                            on_cancel=on_cancel
                        />
                    }
                        .into_any()
                }
                Body::Edit(existing) => {
                    view! {
                        <ThemeForm mode=ThemeFormMode::Edit { existing } on_saved=on_saved on_cancel=on_cancel />
                    }
                        .into_any()
                }
            }}

            <h2>"Sandbox"</h2>
            {move || match sandbox.get() {
                None => view! { <p class="muted">"Checking…"</p> }.into_any(),
                Some(status) if status.available => {
                    view! {
                        <p>
                            "Backend " <code>{status.backend}</code> " is available — "
                            "a project's directories are confined to sandboxed subprocesses through it."
                        </p>
                    }
                        .into_any()
                }
                Some(status) => {
                    view! {
                        <p class="error">
                            "Backend " <code>{status.backend}</code> " is unavailable"
                            {status.reason.map(|reason| format!(": {reason}"))}
                            ". Sandboxed commands will fail until this is fixed."
                        </p>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

/// The read-only palette for `theme`, plus the actions available on it:
/// Clone always, Edit and Delete only when `editable` is `Some` — i.e. only
/// when the current selection is a user theme, never a built-in.
#[component]
fn ThemePalette(
    theme: Theme,
    editable: Option<UserTheme>,
    #[prop(into)] on_clone: Callback<()>,
    #[prop(into)] on_edit: Callback<UserTheme>,
    confirming_delete: RwSignal<bool>,
    #[prop(into)] on_delete: Callback<()>,
    delete_error: RwSignal<Option<String>>,
) -> impl IntoView {
    let rows: Vec<(ThemeField, String)> =
        ThemeField::ALL.into_iter().map(|field| (field, field.get(&theme).to_string())).collect();

    view! {
        <table class="theme-palette">
            <tbody>
                <For each=move || rows.clone() key=|(field, _)| *field let:row>
                    <tr>
                        <td>{row.0.label()}</td>
                        <td class="theme-palette-value">
                            <span class="theme-swatch" style:background-color=row.1.clone()></span>
                            <code>{row.1.clone()}</code>
                        </td>
                    </tr>
                </For>
            </tbody>
        </table>

        <div class="theme-actions">
            <button type="button" on:click=move |_| on_clone.run(())>
                "Clone"
            </button>
            {editable
                .map(|existing| {
                    let for_edit = existing.clone();
                    view! {
                        <button type="button" on:click=move |_| on_edit.run(for_edit.clone())>
                            "Edit"
                        </button>
                        {move || {
                            if confirming_delete.get() {
                                view! {
                                    <span class="confirm-delete">
                                        "Delete this theme?"
                                        <button on:click=move |_| on_delete.run(())>"Yes"</button>
                                        <button on:click=move |_| confirming_delete.set(false)>"No"</button>
                                    </span>
                                }
                                    .into_any()
                            } else {
                                view! {
                                    <button type="button" on:click=move |_| confirming_delete.set(true)>
                                        "Delete"
                                    </button>
                                }
                                    .into_any()
                            }
                        }}
                    }
                })}
        </div>

        {move || delete_error.get().map(|message| view! { <p class="error">{message}</p> })}
    }
}

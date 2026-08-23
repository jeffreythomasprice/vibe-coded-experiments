//! The theme editor, used both for Clone (create a new theme, name blank)
//! and Edit (an existing user theme, every field prefilled) from the
//! settings page. Modeled directly on `AgentForm`.

use leptos::prelude::*;
use shared::theme::{Theme, ThemeField, UserTheme, UserThemeInput};
use wasm_bindgen_futures::spawn_local;

use crate::commands;

/// What this form is doing, and what it prefills from.
#[derive(Debug, Clone)]
pub enum ThemeFormMode {
    /// Clone: prefill every color from `source`; the name starts blank so
    /// saving is blocked until a unique one is typed. Saves via
    /// `create_theme`.
    Create { source: Theme },
    /// Edit: prefill every field, including the name, from `existing`.
    /// Saves via `update_theme(existing.id, ...)`.
    Edit { existing: UserTheme },
}

#[component]
pub fn ThemeForm(
    mode: ThemeFormMode,
    #[prop(into)] on_saved: Callback<UserTheme>,
    #[prop(into)] on_cancel: Callback<()>,
) -> impl IntoView {
    let (initial_name, initial_theme, edit_id) = match mode {
        ThemeFormMode::Create { source } => (String::new(), source, None),
        ThemeFormMode::Edit { existing } => (existing.name, existing.theme, Some(existing.id)),
    };

    let name = RwSignal::new(initial_name);
    let fields: Vec<(ThemeField, RwSignal<String>)> = ThemeField::ALL
        .into_iter()
        .map(|field| (field, RwSignal::new(field.get(&initial_theme).to_string())))
        .collect();
    let error = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);

    let submit_fields = fields.clone();
    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);

        let trimmed_name = name.get().trim().to_string();
        if trimmed_name.is_empty() {
            error.set(Some("Name is required".to_string()));
            return;
        }

        let mut theme = initial_theme.clone();
        for (field, value) in &submit_fields {
            let trimmed_value = value.get().trim().to_string();
            if trimmed_value.is_empty() {
                error.set(Some(format!("{} is required", field.label())));
                return;
            }
            field.set(&mut theme, trimmed_value);
        }

        let input = UserThemeInput { name: trimmed_name, theme };
        saving.set(true);
        spawn_local(async move {
            let result = match edit_id {
                Some(id) => commands::update_theme(id, input).await,
                None => commands::create_theme(input).await,
            };
            match result {
                Ok(saved) => {
                    saving.set(false);
                    on_saved.run(saved);
                }
                Err(err) => {
                    saving.set(false);
                    error.set(Some(err.message));
                }
            }
        });
    };

    view! {
        <form class="theme-form" on:submit=submit>
            <label class="theme-form-field">
                <span>"Name"</span>
                <input
                    type="text"
                    required=true
                    prop:value=move || name.get()
                    on:input=move |ev| name.set(event_target_value(&ev))
                />
            </label>

            <For each=move || fields.clone() key=|(field, _)| *field let:entry>
                <ThemeColorField field=entry.0 value=entry.1 />
            </For>

            {move || error.get().map(|message| view! { <p class="error">{message}</p> })}

            <div class="theme-form-actions">
                <button type="submit" disabled=move || saving.get()>
                    "Save"
                </button>
                <button type="button" on:click=move |_| on_cancel.run(())>
                    "Cancel"
                </button>
            </div>
        </form>
    }
}

/// One color field: a native color picker and its hex text, kept in sync
/// through the same `value` signal — editing either updates the other.
#[component]
fn ThemeColorField(field: ThemeField, value: RwSignal<String>) -> impl IntoView {
    view! {
        <label class="theme-form-field">
            <span>{field.label()}</span>
            <div class="theme-color-input">
                <input
                    type="color"
                    prop:value=move || value.get()
                    on:input=move |ev| value.set(event_target_value(&ev))
                />
                <input
                    type="text"
                    required=true
                    prop:value=move || value.get()
                    on:input=move |ev| value.set(event_target_value(&ev))
                />
            </div>
        </label>
    }
}

//! Settings: everything stored in the `preferences` table. Today that is the
//! theme and nothing else.

use leptos::prelude::*;
use shared::theme::ThemeChoice;

use crate::theme::use_theme;

#[component]
pub fn Settings() -> impl IntoView {
    let theme = use_theme();

    view! {
        <div class="settings-view">
            <h1>"Settings"</h1>

            <label class="settings-field">
                <span>"Theme"</span>
                <select
                    prop:value=move || theme.choice.get().as_str().to_string()
                    on:change=move |ev| {
                        // Unrecognized values can't occur from these three
                        // options; routing through `from_stored` anyway keeps
                        // one parse path, and it's the one with tests.
                        theme.set_choice(ThemeChoice::from_stored(Some(&event_target_value(&ev))));
                    }
                >
                    <option value="system" selected=move || theme.choice.get() == ThemeChoice::System>
                        "System"
                    </option>
                    <option value="light" selected=move || theme.choice.get() == ThemeChoice::Light>
                        "Light"
                    </option>
                    <option value="dark" selected=move || theme.choice.get() == ThemeChoice::Dark>
                        "Dark"
                    </option>
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
        </div>
    }
}

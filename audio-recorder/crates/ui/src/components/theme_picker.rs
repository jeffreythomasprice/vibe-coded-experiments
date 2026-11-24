use leptos::prelude::*;

use crate::{
    store::{Store, choose_theme},
    theme::SYSTEM_ID,
};

#[component]
pub fn ThemePicker(store: Store) -> impl IntoView {
    let options = store.themes.with_value(|themes| {
        themes
            .iter()
            .map(|t| {
                let id = t.id.clone();
                let selected_id = id.clone();
                view! {
                    <option value=id selected=move || store.theme.get() == selected_id>
                        {t.name.clone()}
                    </option>
                }
            })
            .collect_view()
    });
    view! {
        <label class="theme-picker">
            <span>"Theme"</span>
            <select on:change=move |ev| choose_theme(store, event_target_value(&ev))>
                <option value=SYSTEM_ID selected=move || store.theme.get() == SYSTEM_ID>
                    "System"
                </option>
                {options}
            </select>
        </label>
    }
}

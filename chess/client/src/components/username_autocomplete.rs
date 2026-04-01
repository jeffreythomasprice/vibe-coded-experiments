use std::cell::Cell;
use std::rc::Rc;

use gloo_timers::callback::Timeout;
use leptos::prelude::*;
use leptos::ev;

use chess_shared::ListUsersResponse;

#[component]
pub fn UsernameAutocomplete(
    value: RwSignal<String>,
    #[prop(into)] id: &'static str,
) -> impl IntoView {
    let (query, set_query) = signal(value.get_untracked());
    let (suggestions, set_suggestions) = signal(Vec::<String>::new());
    let (show, set_show) = signal(false);

    let debounce_handle: Rc<Cell<Option<Timeout>>> = Rc::new(Cell::new(None));

    // Sync external value changes (e.g. swap button) back to query
    Effect::new(move |_| {
        let v = value.get();
        if v != query.get_untracked() {
            set_query.set(v);
        }
    });

    let on_input = {
        let debounce_handle = debounce_handle.clone();
        move |ev: ev::Event| {
            let val = event_target_value(&ev);
            set_query.set(val.clone());
            value.set(val.clone());

            // Cancel previous debounce
            debounce_handle.set(None);

            if val.trim().is_empty() {
                set_suggestions.set(vec![]);
                set_show.set(false);
                return;
            }

            let timeout = Timeout::new(300, move || {
                let search = val.clone();
                leptos::task::spawn_local(async move {
                    let url = format!(
                        "/api/users?search={}&page_size=10",
                        js_sys::encode_uri_component(&search)
                    );
                    let Ok(resp) = crate::api::get(&url).send().await else {
                        return;
                    };
                    let Ok(data) = resp.json::<ListUsersResponse>().await else {
                        return;
                    };
                    let names: Vec<String> =
                        data.users.into_iter().map(|u| u.username).collect();
                    set_show.set(!names.is_empty());
                    set_suggestions.set(names);
                });
            });
            debounce_handle.set(Some(timeout));
        }
    };

    let on_focus = move |_: ev::FocusEvent| {
        if !suggestions.get_untracked().is_empty() {
            set_show.set(true);
        }
    };

    let on_blur = move |_: ev::FocusEvent| {
        // Delay hiding so mousedown on a suggestion fires first
        Timeout::new(150, move || {
            set_show.set(false);
        })
        .forget();
    };

    view! {
        <div style="position: relative; display: inline-block; width: 100%;">
            <input
                type="text"
                id=id
                autocomplete="off"
                prop:value=query
                on:input=on_input
                on:focus=on_focus
                on:blur=on_blur
            />
            {move || show.get().then(|| {
                let items = suggestions.get();
                view! {
                    <ul style="position: absolute; top: 100%; left: 0; right: 0; \
                               background: white; border: 1px solid #ccc; \
                               list-style: none; padding: 0; margin: 0; \
                               max-height: 200px; overflow-y: auto; z-index: 100;">
                        {items.into_iter().map(|name| {
                            let name_click = name.clone();
                            view! {
                                <li
                                    style="padding: 0.4em 0.6em; cursor: pointer;"
                                    on:mousedown=move |ev: ev::MouseEvent| {
                                        ev.prevent_default();
                                        value.set(name_click.clone());
                                        set_query.set(name_click.clone());
                                        set_show.set(false);
                                        set_suggestions.set(vec![]);
                                    }
                                >
                                    {name}
                                </li>
                            }
                        }).collect::<Vec<_>>()}
                    </ul>
                }
            })}
        </div>
    }
}

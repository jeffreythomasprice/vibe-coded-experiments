use audio_recorder_schema::ipc::{Empty, QuitArgs};
use leptos::prelude::*;

use crate::store::{Store, run};

fn quit(store: Store) {
    run(store, "quit_app", QuitArgs { force: false }, |_: Empty| {});
}

#[component]
pub fn MenuBar(store: Store) -> impl IntoView {
    let open = RwSignal::new(false);
    window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.ctrl_key() && !ev.shift_key() && !ev.alt_key() && ev.key().eq_ignore_ascii_case("q") {
            ev.prevent_default();
            quit(store);
        }
    });
    view! {
        <div class="menubar" role="menubar">
            <div class="menu">
                <button
                    class="menu-title"
                    role="menuitem"
                    aria-haspopup="true"
                    aria-expanded=move || open.get().to_string()
                    on:click=move |_| open.update(|o| *o = !*o)
                >
                    "File"
                </button>
                <Show when=move || open.get()>
                    <div class="menu-backdrop" on:click=move |_| open.set(false)></div>
                    <div class="menu-popup" role="menu" on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            open.set(false);
                        }
                    }>
                        <button
                            class="menu-item"
                            role="menuitem"
                            on:click=move |_| {
                                open.set(false);
                                quit(store);
                            }
                        >
                            <span>"Exit"</span>
                            <span class="shortcut">"Ctrl+Q"</span>
                        </button>
                    </div>
                </Show>
            </div>
        </div>
    }
}

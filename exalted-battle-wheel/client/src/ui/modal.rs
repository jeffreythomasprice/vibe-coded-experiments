use leptos::ev::MouseEvent;
use leptos::prelude::*;

#[component]
pub fn Modal(title: &'static str, on_close: impl Fn() + Copy + 'static, children: Children) -> impl IntoView {
    let handle = window_event_listener(leptos::ev::keydown, move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Escape" {
            on_close();
        }
    });
    on_cleanup(move || handle.remove());

    let stop_propagation = |ev: MouseEvent| ev.stop_propagation();

    view! {
        <div class="modal-backdrop" on:click=move |_| on_close()>
            <div class="modal-panel" on:click=stop_propagation>
                <button class="modal-dismiss" on:click=move |_| on_close()>
                    "\u{2715}"
                </button>
                <div class="modal-title">{title}</div>
                <div class="modal-body">{children()}</div>
            </div>
        </div>
    }
}

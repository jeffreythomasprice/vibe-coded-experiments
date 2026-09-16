use leptos::ev::MouseEvent;
use leptos::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// How many `Modal`s are currently mounted, so a nested one (the delete-confirmation dialog over
/// the settings dialog, say) can tell whether it's the topmost. Each installs its own window
/// `keydown` listener, so without this an Escape meant for the top one would close every modal
/// underneath it too.
static OPEN_MODALS: AtomicU32 = AtomicU32::new(0);

#[component]
pub fn Modal(
    title: &'static str,
    on_close: impl Fn() + Copy + 'static,
    #[prop(optional)] wide: bool,
    children: Children,
) -> impl IntoView {
    let depth = OPEN_MODALS.fetch_add(1, Ordering::Relaxed) + 1;

    let handle = window_event_listener(leptos::ev::keydown, move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Escape" && OPEN_MODALS.load(Ordering::Relaxed) == depth {
            on_close();
        }
    });
    on_cleanup(move || {
        handle.remove();
        OPEN_MODALS.fetch_sub(1, Ordering::Relaxed);
    });

    let stop_propagation = |ev: MouseEvent| ev.stop_propagation();

    view! {
        <div class="modal-backdrop" on:click=move |_| on_close()>
            <div class="modal-panel" class:modal-panel-wide=wide on:click=stop_propagation>
                <button class="modal-dismiss" on:click=move |_| on_close()>
                    "\u{2715}"
                </button>
                <div class="modal-title">{title}</div>
                <div class="modal-body">{children()}</div>
            </div>
        </div>
    }
}

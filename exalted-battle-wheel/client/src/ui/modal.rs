use leptos::ev::MouseEvent;
use leptos::portal::Portal;
use leptos::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

/// How many `Modal`s are currently mounted, so a nested one (the delete-confirmation dialog over
/// the settings dialog, say) can tell whether it's the topmost. Each installs its own window
/// `keydown` listener, so without this an Escape meant for the top one would close every modal
/// underneath it too.
static OPEN_MODALS: AtomicU32 = AtomicU32::new(0);

/// Portaled to `<body>` rather than rendered inline, so a modal always paints above every panel
/// regardless of where it's declared — `.side-column` and `.action-panel` are each their own
/// stacking context (`position: relative; z-index: 1`), so a `z-index: 900` backdrop rendered
/// inside one only wins locally and can still be covered by a sibling column. See the layering
/// note above `.toast-layer` in `styles.css`.
#[component]
pub fn Modal(
    title: &'static str,
    on_close: impl Fn() + Copy + Send + Sync + 'static,
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

    // `Portal`'s children run through an `Fn`, but `Modal` only receives a `FnOnce` — stash it and
    // take it on the first (only) run.
    let children = StoredValue::new_local(Some(children));

    view! {
        <Portal>
            <div class="modal-backdrop" on:click=move |_| on_close()>
                <div class="modal-panel" class:modal-panel-wide=wide on:click=stop_propagation>
                    <button class="modal-dismiss" on:click=move |_| on_close()>
                        "\u{2715}"
                    </button>
                    <div class="modal-title">{title}</div>
                    <div class="modal-body">{move || children.try_update_value(Option::take).flatten().map(|children| children())}</div>
                </div>
            </div>
        </Portal>
    }
}

//! Modelled on `tip.rs`'s `TipLayer`: one layer mounted once near the root, driven by a context
//! signal, with a free function any caller — including `persist.rs`, which has no view to thread
//! a prop through — can reach without knowing whether a layer is even mounted.

use leptos::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_ID: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub id: u32,
    pub message: String,
}

pub type Toasts = RwSignal<Vec<Toast>>;

/// Pushes an error message onto the toast layer, if one is mounted. Callers are expected to log
/// the underlying error themselves first — this only ever holds the user-facing text, not the
/// error's `Debug`/tracing fields.
pub fn error(message: impl Into<String>) {
    let Some(toasts) = use_context::<Toasts>() else { return };
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    toasts.update(|toasts| toasts.push(Toast { id, message: message.into() }));
}

fn dismiss(toasts: Toasts, id: u32) {
    toasts.update(|toasts| toasts.retain(|toast| toast.id != id));
}

/// Errors are sticky — dismissed by hand, not on a timer — because a lost battle or a save that
/// silently stopped working is worth reading, not glancing past.
#[component]
pub fn ToastLayer() -> impl IntoView {
    let toasts = expect_context::<Toasts>();

    view! {
        <div class="toast-layer" role="status" aria-live="polite">
            <For each=move || toasts.get() key=|toast| toast.id let:toast>
                <div class="toast">
                    <div class="toast-message">{toast.message.clone()}</div>
                    <button class="toast-dismiss" on:click=move |_| dismiss(toasts, toast.id)>
                        "\u{2715}"
                    </button>
                </div>
            </For>
        </div>
    }
}

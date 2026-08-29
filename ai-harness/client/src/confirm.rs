//! The one confirmation dialog every destructive action in this app uses —
//! deleting an agent, a project, a theme, or a conversation. Replaces what
//! used to be three hand-copied inline "Delete this X? [Yes] [No]" swaps
//! (agents, projects, themes each had their own), so every "are you sure"
//! in the app now looks and behaves the same way.
//!
//! Owns no open/closed state of its own — the caller's own
//! `Option<Id>`/`bool` decides whether to render it at all, the same split
//! `views::AgentForm`/`views::ProjectForm` already use for their own
//! create/edit toggle.

use leptos::html::Div;
use leptos::prelude::*;
use web_sys::{KeyboardEvent, MouseEvent};

/// A centered, modal confirmation for a destructive action.
#[component]
pub fn ConfirmDelete(
    /// The heading, e.g. "Delete this conversation?".
    #[prop(into)]
    title: String,
    /// The item's own name or a short description, shown under the
    /// heading. Omitted renders nothing there.
    #[prop(optional, into)]
    subject: Option<String>,
    #[prop(into)] on_confirm: Callback<()>,
    #[prop(into)] on_cancel: Callback<()>,
) -> impl IntoView {
    let card: NodeRef<Div> = NodeRef::new();

    // Focus the card as soon as it mounts, so Esc works immediately without
    // requiring a click first — there's nothing else on the page a modal
    // confirmation should be competing with for keyboard focus.
    Effect::new(move |_| {
        if let Some(el) = card.get() {
            let _ = el.focus();
        }
    });

    let keydown = move |ev: KeyboardEvent| {
        if ev.key() == "Escape" {
            on_cancel.run(());
        }
    };

    // The card re-dispatches its own clicks, so this only ever fires for a
    // click that actually lands on the backdrop itself.
    let cancel_on_backdrop = move |_: MouseEvent| on_cancel.run(());
    let stop_inside_click = move |ev: MouseEvent| ev.stop_propagation();

    view! {
        <div class="modal-backdrop" on:click=cancel_on_backdrop>
            <div
                class="modal"
                role="dialog"
                aria-modal="true"
                tabindex="-1"
                node_ref=card
                on:click=stop_inside_click
                on:keydown=keydown
            >
                <h2>{title}</h2>
                {subject.map(|text| view! { <p class="modal-subject">{text}</p> })}
                <p class="muted">"This can't be undone."</p>
                <div class="modal-actions">
                    <button type="button" on:click=move |_| on_cancel.run(())>
                        "Cancel"
                    </button>
                    <button type="button" class="danger" on:click=move |_| on_confirm.run(())>
                        "Delete"
                    </button>
                </div>
            </div>
        </div>
    }
}

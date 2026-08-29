//! "View all conversations": every conversation, most-recently-updated first.

use leptos::prelude::*;
use shared::conversation::{ConversationSummary, ListConversations};
use shared::ids::ConversationId;
use wasm_bindgen_futures::spawn_local;

use crate::app::Route;
use crate::commands;
use crate::confirm::ConfirmDelete;
use crate::spinner::LoadingPanel;

#[component]
pub fn AllConversations() -> impl IntoView {
    let route = use_context::<RwSignal<Route>>().expect("Route context is provided by App");
    let reload = use_context::<RwSignal<u32>>().expect("reload counter context is provided by App");

    let conversations = RwSignal::new(Vec::<ConversationSummary>::new());
    let error = RwSignal::new(None::<String>);
    let confirming_delete = RwSignal::new(None::<ConversationId>);
    // Only the very first load — an empty list after that is a real "no
    // conversations yet" state, not a wait.
    let loading = RwSignal::new(true);

    Effect::new(move |_| {
        reload.get();
        spawn_local(async move {
            match commands::list_conversations(ListConversations::default()).await {
                Ok(list) => {
                    conversations.set(list);
                    error.set(None);
                }
                Err(err) => error.set(Some(err.message)),
            }
            loading.set(false);
        });
    });

    let delete = move |id: ConversationId| {
        spawn_local(async move {
            match commands::delete_conversation(id).await {
                Ok(()) => {
                    reload.update(|n| *n += 1);
                    // Deleting the conversation currently open in the main
                    // pane leaves nothing there to show any more.
                    if route.get_untracked() == Route::Conversation(id) {
                        route.set(Route::Blank);
                    }
                }
                Err(err) => error.set(Some(err.message)),
            }
        });
    };

    view! {
        <div class="all-conversations">
            <h1>"All conversations"</h1>
            {move || {
                if loading.get() {
                    view! { <LoadingPanel label="Loading conversations…" /> }.into_any()
                } else {
                    view! {
                        {move || error.get().map(|message| view! { <p class="error">{message}</p> })}
                        <ul class="conversation-list">
                            <For each=move || conversations.get() key=|c| c.id.get() let:conv>
                                <ConversationRow conv=conv route=route confirming_delete=confirming_delete />
                            </For>
                        </ul>
                        {move || {
                            confirming_delete
                                .get()
                                .map(|id| {
                                    view! {
                                        <ConfirmDelete
                                            title="Delete this conversation?"
                                            on_confirm=move |()| {
                                                confirming_delete.set(None);
                                                delete(id);
                                            }
                                            on_cancel=move |()| confirming_delete.set(None)
                                        />
                                    }
                                })
                        }}
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

/// One row: a button that opens the conversation, plus a separate Delete
/// button beside it. Two siblings, not one button wrapping the other — an
/// `<li>` can't nest a `<button>` inside a `<button>`. `.conversation-open`
/// carries what `.conversation-row` used to style back when the whole row
/// was one button (sidebar's own, unrelated `ConversationRow` still is one,
/// so that class stays exactly as it was for it) — `.conversation-list-row`
/// is just this `<li>`'s flex wrapper around the two buttons.
#[component]
fn ConversationRow(
    conv: ConversationSummary,
    route: RwSignal<Route>,
    confirming_delete: RwSignal<Option<ConversationId>>,
) -> impl IntoView {
    let id = conv.id;
    view! {
        <li class="conversation-list-row">
            <button class="conversation-open" on:click=move |_| route.set(Route::Conversation(id))>
                <span class="conversation-title">
                    {conv.title.clone().unwrap_or_else(|| conv.agent_name.clone())}
                </span>
                <span class="conversation-meta">{conv.updated_at.clone()}</span>
                {conv.awaiting_approval.then(|| view! { <span class="badge">"awaiting approval"</span> })}
            </button>
            <button on:click=move |_| confirming_delete.set(Some(id))>"Delete"</button>
        </li>
    }
}

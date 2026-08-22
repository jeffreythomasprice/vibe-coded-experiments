//! "View all conversations": every conversation, most-recently-updated first.

use leptos::prelude::*;
use shared::conversation::{ConversationSummary, ListConversations};
use wasm_bindgen_futures::spawn_local;

use crate::app::Route;
use crate::commands;

#[component]
pub fn AllConversations() -> impl IntoView {
    let route = use_context::<RwSignal<Route>>().expect("Route context is provided by App");
    let reload = use_context::<RwSignal<u32>>().expect("reload counter context is provided by App");

    let conversations = RwSignal::new(Vec::<ConversationSummary>::new());
    let error = RwSignal::new(None::<String>);

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
        });
    });

    view! {
        <div class="all-conversations">
            <h1>"All conversations"</h1>
            {move || error.get().map(|message| view! { <p class="error">{message}</p> })}
            <ul class="conversation-list">
                <For each=move || conversations.get() key=|c| c.id.get() let:conv>
                    <li>
                        <button
                            class="conversation-row"
                            on:click=move |_| route.set(Route::Conversation(conv.id))
                        >
                            <span class="conversation-title">
                                {conv.title.clone().unwrap_or_else(|| conv.agent_name.clone())}
                            </span>
                            <span class="conversation-meta">{conv.updated_at.clone()}</span>
                            {conv
                                .awaiting_approval
                                .then(|| view! { <span class="badge">"awaiting approval"</span> })}
                        </button>
                    </li>
                </For>
            </ul>
        </div>
    }
}

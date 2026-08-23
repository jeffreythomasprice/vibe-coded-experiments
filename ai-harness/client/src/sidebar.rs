//! The left sidebar: New, Agents, the 20 most-recently-updated conversations,
//! then "View all conversations".

use leptos::prelude::*;
use shared::conversation::{ConversationSummary, ListConversations};
use wasm_bindgen_futures::spawn_local;

use crate::app::Route;
use crate::commands;

#[component]
pub fn Sidebar() -> impl IntoView {
    let route = use_context::<RwSignal<Route>>().expect("Route context is provided by App");
    let reload = use_context::<RwSignal<u32>>().expect("reload counter context is provided by App");

    let recent = RwSignal::new(Vec::<ConversationSummary>::new());
    let error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        // Track the reload counter so a create/send/delete anywhere in the
        // app refreshes this list.
        reload.get();
        spawn_local(async move {
            let query = ListConversations {
                limit: Some(20),
                ..Default::default()
            };
            match commands::list_conversations(query).await {
                Ok(list) => {
                    recent.set(list);
                    error.set(None);
                }
                Err(err) => error.set(Some(err.message)),
            }
        });
    });

    view! {
        <nav class="sidebar">
            <button class="sidebar-item" on:click=move |_| route.set(Route::New)>
                "New"
            </button>
            <button class="sidebar-item" on:click=move |_| route.set(Route::Agents)>
                "Agents"
            </button>
            <div class="sidebar-recent">
                <For each=move || recent.get() key=|c| c.id.get() let:conv>
                    <ConversationRow conv=conv route=route />
                </For>
                {move || {
                    error.get().map(|message| view! { <p class="error sidebar-error">{message}</p> })
                }}
            </div>
            <button class="sidebar-item" on:click=move |_| route.set(Route::AllConversations)>
                "View all conversations"
            </button>
            <button
                class="sidebar-item sidebar-settings"
                class:active=move || route.get() == Route::Settings
                on:click=move |_| route.set(Route::Settings)
            >
                <span class="icon" inner_html=crate::icons::GEAR></span>
                <span>"Settings"</span>
            </button>
        </nav>
    }
}

#[component]
fn ConversationRow(conv: ConversationSummary, route: RwSignal<Route>) -> impl IntoView {
    let id = conv.id;
    let label = conv.title.clone().unwrap_or_else(|| conv.agent_name.clone());
    let is_active = move || matches!(route.get(), Route::Conversation(current) if current == id);

    view! {
        <button
            class="sidebar-item conversation-row"
            class:active=is_active
            on:click=move |_| route.set(Route::Conversation(id))
        >
            <span class="conversation-title">{label}</span>
            {conv
                .awaiting_approval
                .then(|| view! { <span class="badge">"awaiting approval"</span> })}
        </button>
    }
}

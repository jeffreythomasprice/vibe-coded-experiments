//! The left sidebar: New, Agents, the 20 most-recently-updated conversations
//! (right-click one for a Delete menu), then "View all conversations".

use leptos::prelude::*;
use shared::conversation::{ConversationSummary, ListConversations};
use shared::ids::ConversationId;
use wasm_bindgen_futures::spawn_local;
use web_sys::MouseEvent;

use crate::app::Route;
use crate::commands;
use crate::confirm::ConfirmDelete;

/// Where the right-click menu is open, and for which conversation.
#[derive(Debug, Clone, Copy, PartialEq)]
struct MenuAt {
    id: ConversationId,
    x: i32,
    y: i32,
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let route = use_context::<RwSignal<Route>>().expect("Route context is provided by App");
    let reload = use_context::<RwSignal<u32>>().expect("reload counter context is provided by App");

    let recent = RwSignal::new(Vec::<ConversationSummary>::new());
    let error = RwSignal::new(None::<String>);
    let menu = RwSignal::new(None::<MenuAt>);
    let confirming_delete = RwSignal::new(None::<ConversationId>);

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

    let delete = move |id: ConversationId| {
        spawn_local(async move {
            match commands::delete_conversation(id).await {
                Ok(()) => {
                    reload.update(|n| *n += 1);
                    if route.get_untracked() == Route::Conversation(id) {
                        route.set(Route::Blank);
                    }
                }
                Err(err) => error.set(Some(err.message)),
            }
        });
    };

    view! {
        <nav class="sidebar">
            <button class="sidebar-item" on:click=move |_| route.set(Route::New)>
                "New"
            </button>
            <button class="sidebar-item" on:click=move |_| route.set(Route::Agents)>
                "Agents"
            </button>
            <button class="sidebar-item" on:click=move |_| route.set(Route::Projects)>
                "Projects"
            </button>
            <div class="sidebar-recent">
                <For each=move || recent.get() key=|c| c.id.get() let:conv>
                    <ConversationRow conv=conv route=route menu=menu />
                </For>
                {move || {
                    error.get().map(|message| view! { <p class="error sidebar-error">{message}</p> })
                }}
                <button class="sidebar-item" on:click=move |_| route.set(Route::AllConversations)>
                    "View all conversations"
                </button>
            </div>
            <button
                class="sidebar-item sidebar-settings"
                class:active=move || route.get() == Route::Settings
                on:click=move |_| route.set(Route::Settings)
            >
                <span class="icon" inner_html=crate::icons::GEAR></span>
                <span>"Settings"</span>
            </button>
        </nav>

        {move || {
            menu.get()
                .map(|at| {
                    let style = format!("left: {}px; top: {}px", at.x, at.y);
                    view! {
                        // A transparent, full-viewport click-catcher: any click
                        // outside the menu itself lands here and closes it —
                        // the same "dismiss on outside click" job
                        // `ConfirmDelete`'s backdrop does, without needing a
                        // document-level listener.
                        <div class="menu-backdrop" on:click=move |_| menu.set(None)></div>
                        <div class="context-menu" style=style>
                            <button on:click=move |_| {
                                menu.set(None);
                                confirming_delete.set(Some(at.id));
                            }>"Delete"</button>
                        </div>
                    }
                })
        }}

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
}

#[component]
fn ConversationRow(conv: ConversationSummary, route: RwSignal<Route>, menu: RwSignal<Option<MenuAt>>) -> impl IntoView {
    let id = conv.id;
    let label = conv.title.clone().unwrap_or_else(|| conv.agent_name.clone());
    let is_active = move || matches!(route.get(), Route::Conversation(current) if current == id);

    let open_menu = move |ev: MouseEvent| {
        ev.prevent_default();
        menu.set(Some(MenuAt { id, x: ev.client_x(), y: ev.client_y() }));
    };

    view! {
        <button
            class="sidebar-item conversation-row"
            class:active=is_active
            on:click=move |_| route.set(Route::Conversation(id))
            on:contextmenu=open_menu
        >
            <span class="conversation-title">{label}</span>
            {conv
                .awaiting_approval
                .then(|| view! { <span class="badge">"awaiting approval"</span> })}
        </button>
    }
}

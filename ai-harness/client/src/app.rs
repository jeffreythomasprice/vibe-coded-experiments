//! The application shell: a sidebar plus a main content area, and the
//! navigation/reload/model-catalog state every view shares through context.

use leptos::prelude::*;
use shared::ids::ConversationId;
use shared::llm::model::ModelCatalog;
use wasm_bindgen_futures::spawn_local;

use crate::sidebar::Sidebar;
use crate::{models, views};

/// Where the main content area is currently pointed. `Blank` is the startup
/// state — nothing is shown until the user picks something from the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Blank,
    New,
    Agents,
    AllConversations,
    Conversation(ConversationId),
}

#[component]
pub fn App() -> impl IntoView {
    let route = RwSignal::new(Route::Blank);
    // Bumped after any create/send/delete so the sidebar and list views know
    // to refetch. A plain counter, not a fine-grained event bus — this app
    // has a handful of views, not enough to need one.
    let reload = RwSignal::new(0u32);
    let catalog = RwSignal::new(None::<ModelCatalog>);

    provide_context(route);
    provide_context(reload);
    provide_context(catalog);

    // Fetched once, here, rather than per form open — `lib::catalog::load`
    // makes four sequential HTTP calls (disk-cached, but still not free).
    spawn_local(async move {
        match models::fetch().await {
            Ok(loaded) => catalog.set(Some(loaded)),
            Err(err) => tracing::error!(message = %err.message, "failed to load the model catalog"),
        }
    });

    view! {
        <div class="shell">
            <Sidebar />
            <main class="content">
                {move || match route.get() {
                    Route::Blank => view! { <div class="blank" /> }.into_any(),
                    Route::New => view! { <views::NewConversation /> }.into_any(),
                    Route::Agents => view! { <views::Agents /> }.into_any(),
                    Route::AllConversations => view! { <views::AllConversations /> }.into_any(),
                    Route::Conversation(id) => view! { <views::Conversation id=id /> }.into_any(),
                }}
            </main>
        </div>
    }
}

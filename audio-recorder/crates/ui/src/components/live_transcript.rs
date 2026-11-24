use leptos::{html, prelude::*};

use crate::{
    components::transcript::snippet_row,
    store::Store,
};
use audio_recorder_schema::ipc::SessionState;

#[component]
pub fn LiveTranscript(store: Store) -> impl IntoView {
    let list = NodeRef::<html::Div>::new();
    Effect::new(move |_| {
        store.live.track();
        if let Some(el) = list.get() {
            el.set_scroll_top(el.scroll_height());
        }
    });

    view! {
        <section
            class="live panel"
            aria-label="Live transcript"
            class:hidden=move || store.state.get() == SessionState::Idle || store.live.with(|l| l.name.is_empty())
        >
            <div class="panel-head">
                <h2>"Live transcript"</h2>
                {move || store.live.with(|l| l.status.clone()).map(|s| view! { <span class="badge auto">{s}</span> })}
            </div>
            <div class="live-body" node_ref=list aria-live="polite">
                {move || {
                    let l = store.live.get();
                    if l.snippets.is_empty() && l.partials.is_empty() {
                        return view! { <p class="empty">"Waiting for speech\u{2026}"</p> }.into_any();
                    }
                    let rows = l
                        .snippets
                        .into_iter()
                        .map(|(s, source)| snippet_row(s, source, None, store.tag_source, store.tag_speaker, store.tag_timestamps))
                        .collect_view();
                    let partials = l
                        .partials
                        .into_iter()
                        .map(|(_, source, text)| {
                            view! {
                                <li class="snippet partial">
                                    <span class="chip">{source}</span>
                                    <span class="snip-text">{text}</span>
                                </li>
                            }
                        })
                        .collect_view();
                    view! { <ol class="snippets">{rows}{partials}</ol> }.into_any()
                }}
            </div>
        </section>
    }
}

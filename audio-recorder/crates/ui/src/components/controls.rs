use audio_recorder_schema::ipc::SessionState;
use leptos::prelude::*;

use crate::{
    fmt,
    store::{Store, run_status},
};

#[component]
pub fn Controls(store: Store) -> impl IntoView {
    let state = store.state;
    view! {
        <div class="controls" role="group" aria-label="Recording controls">
            {move || match state.get() {
                SessionState::Idle => {
                    view! {
                        <button class="btn danger" on:click=move |_| run_status(store, "start_session")>"Record"</button>
                    }
                        .into_any()
                }
                SessionState::Recording => {
                    view! {
                        <button class="btn" on:click=move |_| run_status(store, "pause_session")>"Pause"</button>
                        <button class="btn danger" on:click=move |_| run_status(store, "stop_session")>"Stop"</button>
                    }
                        .into_any()
                }
                SessionState::Paused => {
                    view! {
                        <button class="btn" on:click=move |_| run_status(store, "resume_session")>"Resume"</button>
                        <button class="btn danger" on:click=move |_| run_status(store, "stop_session")>"Stop"</button>
                    }
                        .into_any()
                }
            }}
            <span class="elapsed" class:dim=move || state.get() != SessionState::Recording>
                {move || fmt::clock(store.elapsed_ms())}
            </span>
            <span class="session-size" class:hidden=move || state.get() == SessionState::Idle title="Size of this session on disk">
                {move || fmt::size(store.session_bytes.get())}
            </span>
            <span
                class="status-pill"
                class:rec=move || state.get() == SessionState::Recording
                class:paused=move || state.get() == SessionState::Paused
            >
                {move || match state.get() {
                    SessionState::Idle => "Idle",
                    SessionState::Recording => "Recording",
                    SessionState::Paused => "Paused",
                }}
            </span>
        </div>
    }
}

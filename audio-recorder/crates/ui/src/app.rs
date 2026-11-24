use audio_recorder_schema::ipc::{ConfirmQuitDto, JobList, LiveTranscriptEventDto, MixChangedDto, PlaybackStateDto, RecorderEventDto, SessionState};
use leptos::prelude::*;

use crate::{
    components::{
        confirm::ConfirmHost, controls::Controls, devices::Devices, live_transcript::LiveTranscript, menubar::MenuBar, player::PlayerBar, sessions::Sessions, tasks::Tasks,
        theme_picker::ThemePicker,
    },
    ipc,
    store::{
        Confirm, ConfirmAction, Store, Tab, apply_jobs, job_active, on_live_event, on_recorder_event, refresh_devices, refresh_jobs, refresh_sessions,
        refresh_settings, refresh_status,
    },
};

#[component]
pub fn App() -> impl IntoView {
    let store = Store::new();

    refresh_devices(store);
    refresh_status(store);
    refresh_sessions(store);
    refresh_settings(store);
    refresh_jobs(store);
    ipc::listen_event::<RecorderEventDto>("recorder://event", move |ev| on_recorder_event(store, ev));
    ipc::listen_event::<LiveTranscriptEventDto>("live://transcript", move |ev| on_live_event(store, ev));
    ipc::listen_event::<PlaybackStateDto>("player://state", move |state| store.playback.set(state));
    ipc::listen_event::<MixChangedDto>("mix://changed", move |ev| store.mix_event.set(Some((ev.name, ev.mix))));
    ipc::listen_event::<JobList>("jobs://changed", move |l| apply_jobs(store, l.jobs));
    ipc::listen_event::<ConfirmQuitDto>("app://confirm-quit", move |q| {
        store.confirm.set(Some(Confirm {
            title: "Quit while tasks are running?".into(),
            body: format!(
                "{} background task{} running or queued. Running work is stopped and resumes the next time the app starts.",
                q.active,
                if q.active == 1 { " is" } else { "s are" },
            ),
            confirm_label: "Quit".into(),
            action: ConfirmAction::QuitApp,
        }));
    });
    set_interval(move || store.now_ms.set(js_sys::Date::now()), std::time::Duration::from_millis(250));
    set_interval(
        move || {
            if store.state.get_untracked() != SessionState::Idle {
                refresh_status(store);
            }
        },
        std::time::Duration::from_secs(2),
    );

    view! {
        <MenuBar store=store />
        <header class="topbar">
            <h1>"Audio Recorder"</h1>
            <Controls store=store />
            <ThemePicker store=store />
        </header>
        {move || {
            store
                .error
                .get()
                .map(|msg| {
                    view! {
                        <div class="error" role="alert">
                            <span>{msg}</span>
                            <button class="btn small" on:click=move |_| store.error.set(None)>"Dismiss"</button>
                        </div>
                    }
                })
        }}
        <LiveTranscript store=store />
        <nav class="tabs" role="tablist">
            <button
                class="tab"
                role="tab"
                aria-selected=move || (store.tab.get() == Tab::Sessions).to_string()
                on:click=move |_| store.tab.set(Tab::Sessions)
            >
                "Sessions"
            </button>
            <button
                class="tab"
                role="tab"
                aria-selected=move || (store.tab.get() == Tab::Tasks).to_string()
                on:click=move |_| store.tab.set(Tab::Tasks)
            >
                "Tasks"
                {move || {
                    let n = store.jobs.get().iter().filter(|j| job_active(j)).count();
                    (n > 0).then(|| view! { <span class="badge auto">{n}</span> })
                }}
            </button>
        </nav>
        {move || match store.tab.get() {
            Tab::Sessions => {
                view! {
                    <main class="layout">
                        <Devices store=store />
                        <Sessions store=store />
                    </main>
                }
                    .into_any()
            }
            Tab::Tasks => view! { <main class="single"><Tasks store=store /></main> }.into_any(),
        }}
        <PlayerBar store=store />
        <ConfirmHost store=store />
    }
}

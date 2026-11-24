use audio_recorder_schema::ipc::{Empty, EnqueueSummaryArgs, EnqueueTranscriptionArgs, JobDto, QuitArgs, SessionNameArgs};
use leptos::{html, prelude::*};

use crate::store::{Confirm, ConfirmAction, Store, refresh_jobs, refresh_sessions, run};

fn execute(store: Store, action: ConfirmAction) {
    match action {
        ConfirmAction::DeleteSession(name) => {
            run(store, "delete_session", SessionNameArgs { name: name.clone() }, move |_: Empty| {
                if store.detail.get_untracked().is_some_and(|d| d.summary.name == name) {
                    store.detail.set(None);
                }
                refresh_sessions(store);
            });
        }
        ConfirmAction::Retranscribe(name) => {
            run(store, "enqueue_transcription", EnqueueTranscriptionArgs { name, replace: true }, move |_: JobDto| {
                refresh_jobs(store);
            });
        }
        ConfirmAction::Resummarize(name) => {
            run(store, "enqueue_summary", EnqueueSummaryArgs { name, replace: true }, move |_: JobDto| {
                refresh_jobs(store);
            });
        }
        ConfirmAction::QuitApp => run(store, "quit_app", QuitArgs { force: true }, |_: Empty| {}),
    }
}

#[component]
pub fn ConfirmHost(store: Store) -> impl IntoView {
    move || store.confirm.get().map(|c| view! { <ConfirmDialog store=store confirm=c /> })
}

#[component]
fn ConfirmDialog(store: Store, confirm: Confirm) -> impl IntoView {
    let cancel_ref = NodeRef::<html::Button>::new();
    Effect::new(move |_| {
        if let Some(button) = cancel_ref.get() {
            let _ = button.focus();
        }
    });
    let close = move || store.confirm.set(None);
    let action = confirm.action.clone();
    let accept = move |_| {
        execute(store, action.clone());
        close();
    };
    view! {
        <div class="overlay" on:click=move |_| close()>
            <div
                class="dialog"
                role="dialog"
                aria-modal="true"
                aria-labelledby="dialog-title"
                aria-describedby="dialog-body"
                on:click=|ev| ev.stop_propagation()
                on:keydown=move |ev| {
                    if ev.key() == "Escape" {
                        close();
                    }
                }
            >
                <h2 id="dialog-title">{confirm.title.clone()}</h2>
                <p id="dialog-body">{confirm.body.clone()}</p>
                <div class="dialog-actions">
                    <button class="btn" node_ref=cancel_ref on:click=move |_| close()>"Cancel"</button>
                    <button class="btn danger" on:click=accept>{confirm.confirm_label.clone()}</button>
                </div>
            </div>
        </div>
    }
}

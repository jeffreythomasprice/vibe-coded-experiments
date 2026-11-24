use audio_recorder_schema::ipc::{
    Empty, JobDto, JobDtoKind, JobDtoState, JobIdArgs, RenderTranscriptArgs, RenderedTranscript, SessionNameArgs, SnippetDto, TranscriptDto,
};
use leptos::{prelude::*, task::spawn_local};
use wasm_bindgen::{JsCast, JsValue};

use crate::{
    fmt, ipc,
    store::{Confirm, ConfirmAction, Store, job_active, refresh_jobs, run},
};

fn load(store: Store, name: String, into: RwSignal<Option<TranscriptDto>>) {
    spawn_local(async move {
        match ipc::call::<SessionNameArgs, TranscriptDto>("get_transcript", &SessionNameArgs { name }).await {
            Ok(t) => into.set(Some(t)),
            Err(e) if e.to_string().ends_with("has no transcript") => into.set(None),
            Err(e) => store.error.set(Some(format!("get_transcript: {e}"))),
        }
    });
}

pub(crate) fn copy_text(store: Store, text: String) {
    let result = (|| -> Option<js_sys::Promise> {
        let window = web_sys::window()?;
        let navigator = js_sys::Reflect::get(&window, &JsValue::from_str("navigator")).ok()?;
        let clipboard = js_sys::Reflect::get(&navigator, &JsValue::from_str("clipboard")).ok()?;
        let write: js_sys::Function = js_sys::Reflect::get(&clipboard, &JsValue::from_str("writeText")).ok()?.dyn_into().ok()?;
        write.call1(&clipboard, &JsValue::from_str(&text)).ok()?.dyn_into().ok()
    })();
    match result {
        Some(promise) => spawn_local(async move {
            if let Err(e) = wasm_bindgen_futures::JsFuture::from(promise).await {
                store.error.set(Some(format!("copy failed: {e:?}")));
            }
        }),
        None => store.error.set(Some("clipboard is not available".into())),
    }
}

fn toggle(label: &'static str, flag: RwSignal<bool>) -> impl IntoView {
    view! {
        <button class="btn small mute tag-toggle" aria-pressed=move || flag.get().to_string() on:click=move |_| flag.update(|v| *v = !*v)>
            {label}
        </button>
    }
}

#[component]
pub fn TranscriptSection(store: Store, name: String) -> impl IntoView {
    let transcript: RwSignal<Option<TranscriptDto>> = RwSignal::new(None);
    {
        let name = name.clone();
        Effect::new(move |_| {
            store.transcript_rev.track();
            load(store, name.clone(), transcript);
        });
    }

    let job = {
        let name = name.clone();
        Signal::derive(move || {
            store.jobs.get().into_iter().find(|j| j.kind == JobDtoKind::Transcribe && j.session == name && job_active(j))
        })
    };
    let ahead = {
        let name = name.clone();
        Signal::derive(move || {
            let jobs = store.jobs.get();
            let Some(mine) = jobs.iter().find(|j| j.kind == JobDtoKind::Transcribe && j.session == name && job_active(j)) else {
                return 0;
            };
            jobs.iter().filter(|j| job_active(j) && j.id < mine.id).count()
        })
    };

    let on_transcribe = {
        let name = name.clone();
        move |_| {
            run(
                store,
                "enqueue_transcription",
                audio_recorder_schema::ipc::EnqueueTranscriptionArgs { name: name.clone(), replace: false },
                move |_: JobDto| refresh_jobs(store),
            )
        }
    };
    let on_retranscribe = {
        let name = name.clone();
        move |_| {
            store.confirm.set(Some(Confirm {
                title: "Replace transcript?".into(),
                body: "The current transcript will be deleted and replaced by a new one. This cannot be undone.".into(),
                confirm_label: "Replace".into(),
                action: ConfirmAction::Retranscribe(name.clone()),
            }))
        }
    };
    let on_cancel = move |_| {
        if let Some(j) = job.get_untracked() {
            run(store, "cancel_job", JobIdArgs { id: j.id }, move |_: Empty| refresh_jobs(store));
        }
    };
    let on_copy = {
        let name = name.clone();
        move |_| {
            let args = RenderTranscriptArgs {
                name: name.clone(),
                source: store.tag_source.get_untracked(),
                speaker: store.tag_speaker.get_untracked(),
                timestamps: store.tag_timestamps.get_untracked(),
            };
            run(store, "render_transcript", args, move |r: RenderedTranscript| copy_text(store, r.text));
        }
    };

    let status = move || {
        job.get().map(|j| {
            let text = match j.state {
                JobDtoState::Running => {
                    let stage = j.stage.clone().unwrap_or_else(|| "Starting".into());
                    match j.fraction {
                        Some(f) => format!("{stage} · {:.0}%", f * 100.0),
                        None => stage,
                    }
                }
                _ => match ahead.get() {
                    0 => "Queued".to_string(),
                    n => format!("Queued behind {n} task{}", if n == 1 { "" } else { "s" }),
                },
            };
            view! { <span class="badge auto">{text}</span> }
        })
    };

    let body = move || match transcript.get() {
        None => view! { <p class="empty">"No transcript yet."</p> }.into_any(),
        Some(t) => {
            let show_source = store.tag_source;
            let show_speaker = store.tag_speaker;
            let show_time = store.tag_timestamps;
            let rows = t
                .snippets
                .iter()
                .map(|s| {
                    let source = t.sources.iter().find(|x| x.key == s.source).map_or(s.source.clone(), |x| x.description.clone());
                    let speaker = s
                        .speaker
                        .as_ref()
                        .map(|id| t.speakers.iter().find(|x| &x.id == id).map_or(id.clone(), |x| x.label.clone()));
                    snippet_row(s.clone(), source, speaker, show_source, show_speaker, show_time)
                })
                .collect_view();
            view! { <ol class="snippets">{rows}</ol> }.into_any()
        }
    };

    view! {
        <div class="transcript">
            <div class="transcript-head">
                <h3>"Transcript"</h3>
                {move || transcript.get().filter(|t| t.live).map(|_| view! { <span class="badge">"Live draft"</span> })}
                {status}
                {move || {
                    if job.get().is_some() {
                        view! { <button class="btn small danger-outline" on:click=on_cancel>"Cancel"</button> }.into_any()
                    } else if transcript.get().is_some_and(|t| !t.live) {
                        view! { <button class="btn small" on:click=on_retranscribe.clone()>"Retranscribe"</button> }.into_any()
                    } else {
                        view! { <button class="btn small" on:click=on_transcribe.clone()>"Transcribe"</button> }.into_any()
                    }
                }}
            </div>
            {move || {
                job.get()
                    .filter(|j| j.state == JobDtoState::Running)
                    .map(|j| {
                        let percent = j.fraction.map(|f| (f * 100.0).round() as u32).unwrap_or(0);
                        view! {
                            <div class="meter wide" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=percent>
                                <div class="meter-fill" style:width=format!("{percent}%")></div>
                            </div>
                        }
                    })
            }}
            <div class="transcript-tools" class:hidden=move || transcript.get().is_none()>
                {toggle("Time", store.tag_timestamps)}
                {toggle("Source", store.tag_source)}
                {toggle("Speaker", store.tag_speaker)}
                <button class="btn small" on:click=on_copy>"Copy"</button>
            </div>
            {body}
        </div>
    }
}

pub(crate) fn snippet_row(
    s: SnippetDto,
    source: String,
    speaker: Option<String>,
    show_source: RwSignal<bool>,
    show_speaker: RwSignal<bool>,
    show_time: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <li class="snippet">
            <span class="snip-time" class:hidden=move || !show_time.get()>{fmt::clock(s.start_ms)}</span>
            <span class="chip" class:hidden=move || !show_source.get()>{source}</span>
            {speaker.map(|label| view! { <span class="chip speaker" class:hidden=move || !show_speaker.get()>{label}</span> })}
            <span class="snip-text">{s.text}</span>
        </li>
    }
}

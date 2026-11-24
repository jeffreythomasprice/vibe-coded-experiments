use audio_recorder_schema::ipc::{Empty, EnqueueSummaryArgs, JobDto, JobDtoKind, JobDtoState, JobIdArgs, SessionNameArgs, SummaryDto};
use leptos::{prelude::*, task::spawn_local};

use crate::{
    fmt, ipc, markdown,
    components::transcript::copy_text,
    store::{Confirm, ConfirmAction, Store, job_active, refresh_jobs, run},
};

fn load(store: Store, name: String, into: RwSignal<Option<SummaryDto>>) {
    spawn_local(async move {
        match ipc::call::<SessionNameArgs, SummaryDto>("get_summary", &SessionNameArgs { name }).await {
            Ok(s) => into.set(Some(s)),
            Err(e) if e.to_string().ends_with("has no summary") => into.set(None),
            Err(e) => store.error.set(Some(format!("get_summary: {e}"))),
        }
    });
}

#[component]
pub fn SummarySection(store: Store, name: String) -> impl IntoView {
    let summary: RwSignal<Option<SummaryDto>> = RwSignal::new(None);
    {
        let name = name.clone();
        Effect::new(move |_| {
            store.transcript_rev.track();
            load(store, name.clone(), summary);
        });
    }

    let has_transcript = {
        let name = name.clone();
        Signal::derive(move || store.sessions.get().iter().any(|s| s.name == name && s.has_transcript))
    };
    let job = {
        let name = name.clone();
        Signal::derive(move || {
            store.jobs.get().into_iter().find(|j| j.kind == JobDtoKind::Summarize && j.session == name && job_active(j))
        })
    };
    let ahead = {
        let name = name.clone();
        Signal::derive(move || {
            let jobs = store.jobs.get();
            let Some(mine) = jobs.iter().find(|j| j.kind == JobDtoKind::Summarize && j.session == name && job_active(j)) else {
                return 0;
            };
            jobs.iter().filter(|j| job_active(j) && j.id < mine.id).count()
        })
    };
    let last_failure = {
        let name = name.clone();
        Signal::derive(move || {
            let jobs = store.jobs.get();
            let latest = jobs.iter().filter(|j| j.kind == JobDtoKind::Summarize && j.session == name).max_by(|a, b| a.id.cmp(&b.id))?;
            (latest.state == JobDtoState::Failed).then(|| latest.error.clone().unwrap_or_else(|| "failed".into()))
        })
    };

    let on_summarize = {
        let name = name.clone();
        move |_| {
            run(store, "enqueue_summary", EnqueueSummaryArgs { name: name.clone(), replace: false }, move |_: JobDto| refresh_jobs(store))
        }
    };
    let on_resummarize = {
        let name = name.clone();
        move |_| {
            store.confirm.set(Some(Confirm {
                title: "Replace summary?".into(),
                body: "The current summary will be deleted and replaced by a new one. This cannot be undone.".into(),
                confirm_label: "Replace".into(),
                action: ConfirmAction::Resummarize(name.clone()),
            }))
        }
    };
    let on_cancel = move |_| {
        if let Some(j) = job.get_untracked() {
            run(store, "cancel_job", JobIdArgs { id: j.id }, move |_: Empty| refresh_jobs(store));
        }
    };
    let on_copy = move |_| {
        if let Some(s) = summary.get_untracked() {
            copy_text(store, s.text);
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

    let body = move || match summary.get() {
        None => view! { <p class="empty">"No summary yet."</p> }.into_any(),
        Some(s) => {
            let html = markdown::to_html(&s.text);
            let meta = format!("Written {} by {} · {}", fmt::started(&s.created_at), s.model, s.provider);
            view! {
                <p class="summary-meta">
                    {meta} {s.stale.then(|| view! { <span class="badge" title="The transcript was replaced after this summary was written">"transcript changed"</span> })}
                </p>
                <div class="markdown" inner_html=html></div>
            }
            .into_any()
        }
    };

    view! {
        <div class="summary">
            <div class="transcript-head">
                <h3>"Summary"</h3>
                {status}
                {move || {
                    if job.get().is_some() {
                        view! { <button class="btn small danger-outline" on:click=on_cancel>"Cancel"</button> }.into_any()
                    } else if summary.get().is_some() {
                        view! {
                            <button class="btn small" on:click=on_copy>"Copy"</button>
                            <button class="btn small" disabled=move || !has_transcript.get() on:click=on_resummarize.clone()>
                                "Re-summarize"
                            </button>
                        }
                            .into_any()
                    } else {
                        view! {
                            <span class="tip" title=move || (!has_transcript.get()).then_some("A summary is made from the finished transcript")>
                                <button class="btn small" disabled=move || !has_transcript.get() on:click=on_summarize.clone()>
                                    "Summarize"
                                </button>
                            </span>
                        }
                            .into_any()
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
            {move || last_failure.get().filter(|_| job.get().is_none()).map(|e| view! { <p class="summary-error">{format!("Last attempt failed: {e}")}</p> })}
            {body}
        </div>
    }
}

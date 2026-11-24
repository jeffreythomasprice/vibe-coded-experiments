use audio_recorder_schema::ipc::{Empty, JobDto, JobDtoKind, JobDtoState, JobIdArgs, NoArgs};
use leptos::prelude::*;

use crate::store::{Store, job_active, refresh_jobs, run};

fn state_label(state: JobDtoState) -> &'static str {
    match state {
        JobDtoState::Queued => "queued",
        JobDtoState::Running => "running",
        JobDtoState::Done => "done",
        JobDtoState::Failed => "failed",
        JobDtoState::Cancelled => "cancelled",
    }
}

fn kind_label(job: &JobDto) -> String {
    match job.kind {
        JobDtoKind::Transcribe => format!("Transcription{}", if job.replace { " (replacing)" } else { "" }),
        JobDtoKind::Compress => "Track compression".to_string(),
        JobDtoKind::Summarize => format!("Summary{}", if job.replace { " (replacing)" } else { "" }),
    }
}

fn ordered(mut jobs: Vec<JobDto>) -> Vec<JobDto> {
    jobs.sort_by(|a, b| {
        let rank = |j: &JobDto| match j.state {
            JobDtoState::Running => 0,
            JobDtoState::Queued => 1,
            _ => 2,
        };
        rank(a).cmp(&rank(b)).then_with(|| {
            if job_active(a) { a.id.cmp(&b.id) } else { b.id.cmp(&a.id) }
        })
    });
    jobs
}

#[component]
pub fn Tasks(store: Store) -> impl IntoView {
    view! {
        <section class="panel" aria-labelledby="tasks-title">
            <div class="panel-head">
                <h2 id="tasks-title">"Background tasks"</h2>
                <button
                    class="btn small"
                    disabled=move || store.jobs.get().iter().all(job_active)
                    on:click=move |_| run(store, "clear_finished_jobs", NoArgs::default(), move |_: Empty| refresh_jobs(store))
                >
                    "Clear finished"
                </button>
            </div>
            <ul class="tasks">
                {move || {
                    let jobs = ordered(store.jobs.get());
                    if jobs.is_empty() {
                        view! { <li class="empty">"No background tasks. Transcription, summaries and track compression start automatically when a recording stops."</li> }
                            .into_any()
                    } else {
                        jobs.into_iter().map(|j| view! { <TaskRow store=store job=j /> }).collect_view().into_any()
                    }
                }}
            </ul>
        </section>
    }
}

#[component]
fn TaskRow(store: Store, job: JobDto) -> impl IntoView {
    let active = job_active(&job);
    let id = job.id.clone();
    let percent = job.fraction.map(|f| (f * 100.0).round() as u32);
    let detail = match (&job.stage, &job.error, job.state) {
        (_, Some(error), _) => error.clone(),
        (Some(stage), _, _) => stage.clone(),
        (None, None, JobDtoState::Queued) => "Waiting for earlier tasks".to_string(),
        _ => String::new(),
    };
    view! {
        <li class="task">
            <div class="task-row">
                <span class="task-name">
                    <span class="when">{job.session.clone()}</span>
                    <span class="meta">{kind_label(&job)}</span>
                </span>
                <span class="badge" class:rec=job.state == JobDtoState::Failed class:auto=job.state == JobDtoState::Running>
                    {state_label(job.state)}
                </span>
                {active
                    .then(|| {
                        view! {
                            <button
                                class="btn small danger-outline"
                                on:click=move |_| {
                                    run(
                                        store,
                                        "cancel_job",
                                        JobIdArgs { id: id.clone() },
                                        move |_: Empty| refresh_jobs(store),
                                    )
                                }
                            >
                                "Cancel"
                            </button>
                        }
                    })}
            </div>
            {(!detail.is_empty()).then(|| view! { <div class="meta task-detail">{detail}{percent.map(|p| format!(" · {p}%"))}</div> })}
            {(job.state == JobDtoState::Running)
                .then(|| {
                    view! {
                        <div class="meter wide" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow=percent.unwrap_or(0)>
                            <div class="meter-fill" style:width=format!("{}%", percent.unwrap_or(0))></div>
                        </div>
                    }
                })}
        </li>
    }
}

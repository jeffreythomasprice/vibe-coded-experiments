//! The Projects view: every saved project, each with Edit/Delete, plus a New
//! button — mirrors `views::Agents`. The default project never appears here
//! (it isn't a database row — see `shared::project`'s module doc); it's the
//! implicit "no project" a conversation gets when none is chosen.

use leptos::prelude::*;
use shared::project::Project;
use wasm_bindgen_futures::spawn_local;

use crate::commands;
use crate::spinner::LoadingPanel;
use crate::views::{ProjectForm, ProjectFormMode};

/// What the list row area is showing.
#[derive(Debug, Clone)]
enum Body {
    List,
    Create,
    Edit(Project),
}

#[component]
pub fn Projects() -> impl IntoView {
    let projects = RwSignal::new(Vec::<Project>::new());
    let error = RwSignal::new(None::<String>);
    let body = RwSignal::new(Body::List);
    let confirming_delete = RwSignal::new(None::<shared::ids::ProjectId>);
    let refresh = RwSignal::new(0u32);
    let loading = RwSignal::new(true);

    Effect::new(move |_| {
        refresh.get();
        spawn_local(async move {
            match commands::list_projects().await {
                Ok(list) => {
                    projects.set(list);
                    error.set(None);
                }
                Err(err) => error.set(Some(err.message)),
            }
            loading.set(false);
        });
    });

    let delete = move |id: shared::ids::ProjectId| {
        spawn_local(async move {
            match commands::delete_project(id).await {
                Ok(()) => refresh.update(|n| *n += 1),
                Err(err) => error.set(Some(err.message)),
            }
        });
    };

    let on_saved = move |_project: Project| {
        body.set(Body::List);
        refresh.update(|n| *n += 1);
    };
    let on_cancel = move |_| body.set(Body::List);

    view! {
        <div class="projects-view">
            <h1>"Projects"</h1>
            <p class="muted">
                "A project is a set of directories an agent's sandbox can reach. \
                Starting a conversation with no project chosen gives it no filesystem access at all."
            </p>
            {move || {
                if loading.get() {
                    view! { <LoadingPanel label="Loading projects…" /> }.into_any()
                } else {
                    view! {
                        {move || error.get().map(|message| view! { <p class="error">{message}</p> })}
                        {move || match body.get() {
                            Body::List => {
                                view! {
                                    <ul class="project-list">
                                        <For each=move || projects.get() key=|p| p.id.get() let:project>
                                            <ProjectRow
                                                project=project
                                                confirming_delete=confirming_delete
                                                on_delete=delete
                                                on_edit=move |p| body.set(Body::Edit(p))
                                            />
                                        </For>
                                    </ul>
                                    <button class="new-project-button" on:click=move |_| body.set(Body::Create)>
                                        "New"
                                    </button>
                                }
                                    .into_any()
                            }
                            Body::Create => {
                                view! {
                                    <ProjectForm mode=ProjectFormMode::Create on_saved=on_saved on_cancel=on_cancel />
                                }
                                    .into_any()
                            }
                            Body::Edit(existing) => {
                                view! {
                                    <ProjectForm
                                        mode=ProjectFormMode::Edit { existing }
                                        on_saved=on_saved
                                        on_cancel=on_cancel
                                    />
                                }
                                    .into_any()
                            }
                        }}
                    }
                        .into_any()
                }
            }}
        </div>
    }
}

#[component]
fn ProjectRow(
    project: Project,
    confirming_delete: RwSignal<Option<shared::ids::ProjectId>>,
    on_delete: impl Fn(shared::ids::ProjectId) + Copy + Send + Sync + 'static,
    on_edit: impl Fn(Project) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let id = project.id;
    let name = project.input.name.clone();
    let dir_count = project.input.dirs.len();
    let summary = if dir_count == 0 {
        "no directories".to_string()
    } else if dir_count == 1 {
        "1 directory".to_string()
    } else {
        format!("{dir_count} directories")
    };
    let for_edit = project.clone();

    view! {
        <li class="project-row">
            <div class="project-row-info">
                <strong>{name}</strong>
                <span class="muted">{summary}</span>
            </div>
            <button on:click=move |_| on_edit(for_edit.clone())>"Edit"</button>
            {move || {
                if confirming_delete.get() == Some(id) {
                    view! {
                        <span class="confirm-delete">
                            "Delete this project?"
                            <button on:click=move |_| {
                                confirming_delete.set(None);
                                on_delete(id);
                            }>"Yes"</button>
                            <button on:click=move |_| confirming_delete.set(None)>"No"</button>
                        </span>
                    }
                        .into_any()
                } else {
                    view! {
                        <button on:click=move |_| confirming_delete.set(Some(id))>"Delete"</button>
                    }
                        .into_any()
                }
            }}
        </li>
    }
}

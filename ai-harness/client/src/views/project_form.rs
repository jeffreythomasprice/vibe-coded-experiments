//! The project-creation/edit form, used both standalone (from the Projects
//! view) and inline (from New Conversation). Modeled on `AgentForm`/`ThemeForm`.

use std::path::PathBuf;

use leptos::prelude::*;
use shared::project::{AccessMode, Project, ProjectDir, ProjectInput};
use wasm_bindgen_futures::spawn_local;

use crate::commands;
use crate::spinner::BusyOverlay;

/// What this form is doing, and what it prefills from.
#[derive(Debug, Clone)]
pub enum ProjectFormMode {
    Create,
    /// Saves via `update_project(existing.id, ...)`.
    Edit { existing: Project },
}

/// One directory row being edited. A plain `id` (not the directory's index)
/// keys the `<For>` below, since rows are added/removed by the user and an
/// index would shift out from under an in-progress edit.
#[derive(Clone)]
struct DirRow {
    id: u32,
    path: RwSignal<String>,
    mode: RwSignal<AccessMode>,
}

#[component]
pub fn ProjectForm(
    mode: ProjectFormMode,
    #[prop(into)] on_saved: Callback<Project>,
    #[prop(into)] on_cancel: Callback<()>,
) -> impl IntoView {
    let (initial_name, initial_dirs, edit_id) = match mode {
        ProjectFormMode::Create => (String::new(), Vec::new(), None),
        ProjectFormMode::Edit { existing } => (existing.input.name, existing.input.dirs, Some(existing.id)),
    };

    let name = RwSignal::new(initial_name);
    let next_row_id = RwSignal::new(0u32);
    let new_row = |path: String, mode: AccessMode, next_row_id: RwSignal<u32>| -> DirRow {
        let id = next_row_id.get_untracked();
        next_row_id.set(id + 1);
        DirRow {
            id,
            path: RwSignal::new(path),
            mode: RwSignal::new(mode),
        }
    };
    let initial_rows: Vec<DirRow> = initial_dirs
        .into_iter()
        .map(|dir| new_row(dir.path.display().to_string(), dir.mode, next_row_id))
        .collect();
    let rows = RwSignal::new(initial_rows);
    let error = RwSignal::new(None::<String>);
    let saving = RwSignal::new(false);

    let add_row = move |_| {
        rows.update(|rows| rows.push(new_row(String::new(), AccessMode::ReadWrite, next_row_id)));
    };
    let remove_row = move |id: u32| {
        rows.update(|rows| rows.retain(|row| row.id != id));
    };

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);

        let trimmed_name = name.get().trim().to_string();
        if trimmed_name.is_empty() {
            error.set(Some("Name is required".to_string()));
            return;
        }

        let mut dirs = Vec::new();
        for row in rows.get() {
            let trimmed_path = row.path.get().trim().to_string();
            if trimmed_path.is_empty() {
                error.set(Some("every directory needs a path (or remove the row)".to_string()));
                return;
            }
            dirs.push(ProjectDir {
                path: PathBuf::from(trimmed_path),
                mode: row.mode.get(),
            });
        }

        let input = ProjectInput {
            name: trimmed_name,
            description: None,
            dirs,
        };
        saving.set(true);
        spawn_local(async move {
            let result = match edit_id {
                Some(id) => commands::update_project(id, input).await,
                None => commands::create_project(input).await,
            };
            match result {
                Ok(saved) => {
                    saving.set(false);
                    on_saved.run(saved);
                }
                Err(err) => {
                    saving.set(false);
                    error.set(Some(err.message));
                }
            }
        });
    };

    view! {
        <form class="project-form" on:submit=submit>
            {move || saving.get().then(|| view! { <BusyOverlay label="Saving project…" /> })}
            <fieldset disabled=saving>
                <label class="project-form-field">
                    <span>"Name"</span>
                    <input
                        type="text"
                        required=true
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                </label>

                <div class="project-form-dirs">
                    <span>"Directories"</span>
                    <p class="muted">
                        "Zero directories means no filesystem access at all — the default, safest project."
                    </p>
                    <For each=move || rows.get() key=|row| row.id let:row>
                        <ProjectDirRow row=row on_remove=remove_row />
                    </For>
                    <button type="button" class="new-project-dir-button" on:click=add_row>
                        "Add directory"
                    </button>
                </div>

                {move || error.get().map(|message| view! { <p class="error">{message}</p> })}
            </fieldset>

            <div class="project-form-actions">
                <button type="submit" disabled=move || saving.get()>
                    "Save"
                </button>
                <button type="button" on:click=move |_| on_cancel.run(())>
                    "Cancel"
                </button>
            </div>
        </form>
    }
}

#[component]
fn ProjectDirRow(row: DirRow, on_remove: impl Fn(u32) + Copy + Send + Sync + 'static) -> impl IntoView {
    let id = row.id;
    let path = row.path;
    let mode = row.mode;
    view! {
        <div class="project-dir-row">
            <input
                type="text"
                class="project-dir-path"
                placeholder="/absolute/path/to/directory"
                prop:value=move || path.get()
                on:input=move |ev| path.set(event_target_value(&ev))
            />
            <select
                prop:value=move || match mode.get() {
                    AccessMode::ReadWrite => "read_write",
                    AccessMode::ReadOnly => "read_only",
                }
                on:change=move |ev| {
                    mode.set(
                        if event_target_value(&ev) == "read_only" {
                            AccessMode::ReadOnly
                        } else {
                            AccessMode::ReadWrite
                        },
                    );
                }
            >
                <option value="read_write">"Read-write"</option>
                <option value="read_only">"Read-only"</option>
            </select>
            <button type="button" on:click=move |_| on_remove(id)>
                "Remove"
            </button>
        </div>
    }
}

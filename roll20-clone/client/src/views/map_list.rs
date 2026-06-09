use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use shared::MapSummary;
use wasm_bindgen_futures::spawn_local;

use crate::api;

/// Lists all maps and lets the user open or create one.
#[component]
pub fn MapListView() -> impl IntoView {
    let maps = RwSignal::new(Vec::<MapSummary>::new());
    let error = RwSignal::new(Option::<String>::None);
    let loading = RwSignal::new(true);

    // Load the list once on mount.
    spawn_local(async move {
        match api::list_maps().await {
            Ok(list) => maps.set(list),
            Err(e) => error.set(Some(e)),
        }
        loading.set(false);
    });

    let navigate = use_navigate();
    let on_new = {
        let navigate = navigate.clone();
        move |_| {
            let navigate = navigate.clone();
            spawn_local(async move {
                match api::create_map(&api::default_create_request("Untitled Map")).await {
                    Ok(map) => navigate(&format!("/maps/{}", map.id), Default::default()),
                    Err(e) => error.set(Some(e)),
                }
            });
        }
    };

    let rows = move || {
        let navigate = navigate.clone();
        maps.get()
            .into_iter()
            .map(|m| {
                let navigate = navigate.clone();
                let id = m.id.clone();
                let open = move |_| navigate(&format!("/maps/{id}"), Default::default());
                view! {
                    <tr class="map-row" on:click=open>
                        <td>{m.name}</td>
                        <td>{m.width}" x "{m.height}" sq"</td>
                        <td>{m.grid_size}" "{m.grid_unit}"/sq"</td>
                    </tr>
                }
            })
            .collect_view()
    };

    view! {
        <div class="pad">
            <div class="list-header">
                <h2>"Maps"</h2>
                <button on:click=on_new>"+ New map"</button>
            </div>
            {move || error.get().map(|e| view! { <p class="error">"Error: " {e}</p> })}
            {move || loading.get().then(|| view! { <p>"Loading…"</p> })}
            <table class="map-table">
                <thead>
                    <tr><th>"Name"</th><th>"Size"</th><th>"Grid"</th></tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
            {move || (!loading.get() && maps.get().is_empty())
                .then(|| view! { <p class="muted">"No maps yet. Create one to get started."</p> })}
        </div>
    }
}

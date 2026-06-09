//! Right-sidebar inspector: shows and edits the style of the current selection,
//! and renders the boolean-operator tree of any selected group.

use leptos::prelude::*;
use shared::{BoolOp, Geometry, GroupNode, Map, Style, UpdateGroupRequest, UpdateShapeRequest};
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::render::SelId;

/// Which style field an edit targets.
#[derive(Clone)]
enum Edit {
    LineColor(String),
    LineWidth(f64),
    Background(String),
}

fn apply_to_style(style: &mut Style, edit: &Edit) {
    match edit {
        Edit::LineColor(c) => style.line_color = c.clone(),
        Edit::LineWidth(w) => style.line_width = *w,
        Edit::Background(c) => style.background_color = c.clone(),
    }
}

/// Collapse a list of values to `Some(v)` if all equal, else `None` ("multiple").
fn merged<T: PartialEq + Clone>(vals: &[T]) -> Option<T> {
    match vals.split_first() {
        Some((head, tail)) if tail.iter().all(|v| v == head) => Some(head.clone()),
        _ => None,
    }
}

/// The styles of every currently-selected item.
fn selected_styles(map: &Map, selection: &[SelId]) -> Vec<Style> {
    selection
        .iter()
        .filter_map(|s| match s {
            SelId::Shape(id) => map.shapes.iter().find(|x| &x.id == id).map(|x| x.style.clone()),
            SelId::Group(id) => map.groups.iter().find(|x| &x.id == id).map(|x| x.style.clone()),
        })
        .collect()
}

#[component]
pub fn SelectionInspector(
    map: RwSignal<Option<Map>>,
    selection: RwSignal<Vec<SelId>>,
) -> impl IntoView {
    // Apply an edit to every selected item, persisting each via the API. The
    // requests run sequentially so the final response reflects all changes.
    let apply = move |edit: Edit| {
        let Some(m) = map.get_untracked() else { return };
        let sel = selection.get_untracked();
        let map_id = m.id.clone();
        spawn_local(async move {
            let mut last = None;
            for item in &sel {
                match item {
                    SelId::Shape(id) => {
                        if let Some(shape) = m.shapes.iter().find(|x| &x.id == id) {
                            let mut style = shape.style.clone();
                            apply_to_style(&mut style, &edit);
                            if let Ok(updated) = api::update_shape(
                                &map_id,
                                id,
                                &UpdateShapeRequest { geometry: None, style: Some(style) },
                            )
                            .await
                            {
                                last = Some(updated);
                            }
                        }
                    }
                    SelId::Group(id) => {
                        if let Some(group) = m.groups.iter().find(|x| &x.id == id) {
                            let mut style = group.style.clone();
                            apply_to_style(&mut style, &edit);
                            if let Ok(updated) = api::update_group(
                                &map_id,
                                id,
                                &UpdateGroupRequest { style: Some(style), root: None },
                            )
                            .await
                            {
                                last = Some(updated);
                            }
                        }
                    }
                }
            }
            if let Some(updated) = last {
                map.set(Some(updated));
            }
        });
    };

    let styles = move || match map.get() {
        Some(m) => selected_styles(&m, &selection.get()),
        None => Vec::new(),
    };

    let line_color = move || merged(&styles().iter().map(|s| s.line_color.clone()).collect::<Vec<_>>());
    let line_width = move || merged(&styles().iter().map(|s| s.line_width).collect::<Vec<_>>());
    let background = move || {
        merged(&styles().iter().map(|s| s.background_color.clone()).collect::<Vec<_>>())
    };

    // Group trees for any selected groups.
    let groups_view = move || {
        let m = map.get()?;
        let sel = selection.get();
        let items: Vec<_> = sel
            .iter()
            .filter_map(|s| match s {
                SelId::Group(id) => m.groups.iter().find(|g| &g.id == id).cloned(),
                _ => None,
            })
            .map(|g| {
                view! {
                    <div class="tree">
                        <div class="tree-title">"Group " <code>{short(&g.id)}</code></div>
                        <ul>{tree_node(&g.root)}</ul>
                    </div>
                }
            })
            .collect();
        (!items.is_empty()).then_some(items)
    };

    view! {
        <aside class="inspector">
            <h3>"Inspector"</h3>
            {move || {
                let count = selection.get().len();
                if count == 0 {
                    view! { <p class="muted">"Nothing selected."</p> }.into_any()
                } else {
                    let apply = apply.clone();
                    let apply2 = apply.clone();
                    let apply3 = apply.clone();
                    view! {
                        <p class="muted">{count}" selected"</p>
                        <div class="field">
                            <label>"Line color"</label>
                            <ColorField value=Signal::derive(line_color) on_set=move |c| apply(Edit::LineColor(c))/>
                        </div>
                        <div class="field">
                            <label>"Line width"</label>
                            <NumberField value=Signal::derive(line_width) on_set=move |w| apply2(Edit::LineWidth(w))/>
                        </div>
                        <div class="field">
                            <label>"Background"</label>
                            <ColorField value=Signal::derive(background) on_set=move |c| apply3(Edit::Background(c))/>
                        </div>
                    }.into_any()
                }
            }}
            {groups_view}
        </aside>
    }
}

/// A color input that shows "multiple" when the selection's values differ.
#[component]
fn ColorField(
    value: Signal<Option<String>>,
    on_set: impl Fn(String) + 'static,
) -> impl IntoView {
    let on_input = move |ev: leptos::ev::Event| on_set(event_target_value(&ev));
    view! {
        <span class="value-edit">
            <input
                type="color"
                prop:value=move || value.get().unwrap_or_else(|| "#000000".to_string())
                on:input=on_input
            />
            {move || value.get().is_none().then(|| view! { <em class="multi">"(multiple)"</em> })}
        </span>
    }
}

/// A number input that shows a "multiple" placeholder when values differ.
#[component]
fn NumberField(
    value: Signal<Option<f64>>,
    on_set: impl Fn(f64) + 'static,
) -> impl IntoView {
    let on_change = move |ev: leptos::ev::Event| {
        if let Ok(n) = event_target_value(&ev).parse::<f64>() {
            on_set(n);
        }
    };
    view! {
        <span class="value-edit">
            <input
                type="number"
                min="0"
                step="0.5"
                prop:value=move || value.get().map(|w| w.to_string()).unwrap_or_default()
                placeholder=move || if value.get().is_none() { "multiple" } else { "" }
                on:change=on_change
            />
        </span>
    }
}

/// Render one node of a group's boolean tree as nested list items.
fn tree_node(node: &GroupNode) -> AnyView {
    match node {
        GroupNode::Leaf { shape } => {
            let desc = match shape.geometry {
                Geometry::Rect { x, y, w, h } => {
                    format!("rect ({x}, {y}) {w}×{h}")
                }
            };
            view! { <li class="leaf"><code>{short(&shape.id)}</code>" "{desc}</li> }.into_any()
        }
        GroupNode::Op { op, left, right } => {
            let label = match op {
                BoolOp::Union => "UNION",
                BoolOp::Intersect => "INTERSECT",
                BoolOp::Subtract => "SUBTRACT",
            };
            view! {
                <li class="op">
                    <span class="op-label">{label}</span>
                    <ul>
                        {tree_node(left)}
                        {tree_node(right)}
                    </ul>
                </li>
            }
            .into_any()
        }
    }
}

/// Short form of a uuid/id for display.
fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

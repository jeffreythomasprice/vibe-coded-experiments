//! Generalizes the wheel's original `Hovered`/`HoverCard` pattern (`tooltip.rs`) into a tooltip
//! usable on any control: `Tip` wraps ordinary HTML content, and `on_pointer_*`/`on_focus_*`
//! attach the same behavior directly to SVG nodes, which cannot host a wrapping `<span>`.

use crate::prefs::Prefs;
use crate::ui::glossary::{Source, Topic};
use leptos::ev::{FocusEvent, PointerEvent};
use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys;

pub type ActiveTip = RwSignal<Option<TipAnchor>>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TipAnchor {
    pub topic: Topic,
    pub x: f64,
    pub y: f64,
}

fn show(topic: Topic, x: f64, y: f64) {
    if !expect_context::<Prefs>().teaching_mode.get_untracked() {
        return;
    }
    expect_context::<ActiveTip>().set(Some(TipAnchor { topic, x, y }));
}

/// Only clears the tip if it's still showing `topic` — otherwise a leave event fired after the
/// pointer has already entered a neighboring tip target would erase that one instead.
fn hide(topic: Topic) {
    expect_context::<ActiveTip>().update(|current| {
        if matches!(current, Some(anchor) if anchor.topic == topic) {
            *current = None;
        }
    });
}

fn element_anchor(target: Option<web_sys::EventTarget>) -> (f64, f64) {
    target
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        .map(|el| {
            let rect = el.get_bounding_client_rect();
            (rect.left() + rect.width() / 2.0, rect.bottom())
        })
        .unwrap_or((0.0, 0.0))
}

fn window_size() -> (f64, f64) {
    let Some(window) = web_sys::window() else { return (0.0, 0.0) };
    let width = window.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    let height = window.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    (width, height)
}

pub fn on_pointer_enter(topic: Topic) -> impl Fn(PointerEvent) + Clone {
    move |ev: PointerEvent| show(topic, ev.client_x() as f64, ev.client_y() as f64)
}

pub fn on_pointer_leave(topic: Topic) -> impl Fn(PointerEvent) + Clone {
    move |_: PointerEvent| hide(topic)
}

pub fn on_focus_in(topic: Topic) -> impl Fn(FocusEvent) + Clone {
    move |ev: FocusEvent| {
        let (x, y) = element_anchor(ev.target());
        show(topic, x, y);
    }
}

pub fn on_focus_out(topic: Topic) -> impl Fn(FocusEvent) + Clone {
    move |_: FocusEvent| hide(topic)
}

/// Wraps HTML content so hovering or focusing it shows `topic`'s glossary entry.
#[component]
pub fn Tip(topic: Topic, children: Children) -> impl IntoView {
    view! {
        <span
            class="tip-target"
            tabindex="0"
            on:pointerenter=on_pointer_enter(topic)
            on:pointerleave=on_pointer_leave(topic)
            on:focusin=on_focus_in(topic)
            on:focusout=on_focus_out(topic)
        >
            {children()}
        </span>
    }
}

/// Mounted once, high in the tree. Follows the most recently anchored tip and flips to stay on
/// screen near the window edges.
#[component]
pub fn TipLayer() -> impl IntoView {
    let active = expect_context::<ActiveTip>();

    let position_style = move || {
        let Some(anchor) = active.get() else { return String::new() };
        let (width, height) = window_size();
        let horizontal = if anchor.x > width / 2.0 {
            format!("right: {}px;", (width - anchor.x).max(8.0))
        } else {
            format!("left: {}px;", anchor.x.max(8.0))
        };
        let vertical = if anchor.y > height / 2.0 {
            format!("bottom: {}px;", (height - anchor.y).max(8.0))
        } else {
            format!("top: {}px;", anchor.y.max(8.0))
        };
        format!("{horizontal} {vertical}")
    };

    let entry = move || active.get().map(|anchor| anchor.topic.entry());

    view! {
        <div class="tip-layer" class:tip-layer-visible=move || entry().is_some() style=position_style>
            {move || {
                entry()
                    .map(|entry| {
                        let (quote, cite_label) = match entry.source {
                            Source::Book { quote, cite } => (quote, Some(cite.label())),
                            Source::AppConvention => (None, None),
                        };
                        view! {
                            <div class="tip-term">{entry.term}</div>
                            <div class="tip-what">{entry.what}</div>
                            <div class="tip-interacts">{entry.interacts}</div>
                            {quote.map(|q| view! { <div class="tip-quote">{format!("“{q}”")}</div> })}
                            {cite_label.map(|c| view! { <div class="tip-cite">{c}</div> })}
                        }
                    })
            }}
        </div>
    }
}

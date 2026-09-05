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

/// A glossary `Topic` is gated behind Teaching mode; free-form `Text` (e.g. event-log detail) is
/// not teaching content and always shows. `TopicWithDetail` is a `Topic` plus a computed line
/// (e.g. projected tick numbers) appended to the tooltip — still cited and Teaching-gated.
#[derive(Debug, Clone, PartialEq)]
pub enum TipContent {
    Topic(Topic),
    TopicWithDetail(Topic, String),
    Text(String),
}

impl TipContent {
    fn topic(&self) -> Option<Topic> {
        match self {
            TipContent::Topic(topic) | TipContent::TopicWithDetail(topic, _) => Some(*topic),
            TipContent::Text(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TipAnchor {
    pub content: TipContent,
    pub x: f64,
    pub y: f64,
}

fn show(content: TipContent, x: f64, y: f64) {
    if content.topic().is_some() && !expect_context::<Prefs>().teaching_mode.get_untracked() {
        return;
    }
    expect_context::<ActiveTip>().set(Some(TipAnchor { content, x, y }));
}

/// Only clears the tip if it's still showing `content` — otherwise a leave event fired after the
/// pointer has already entered a neighboring tip target would erase that one instead.
fn hide(content: TipContent) {
    expect_context::<ActiveTip>().update(|current| {
        if matches!(current, Some(anchor) if anchor.content == content) {
            *current = None;
        }
    });
}

/// Like `hide`, but matches on topic alone. Needed for a `DetailTip` whose detail text can change
/// out from under a still-hovering pointer (e.g. the selected sorcery Circle changes) — comparing
/// the whole `TipContent` would leave the stale tip stranded since the detail no longer matches.
fn hide_topic(topic: Topic) {
    expect_context::<ActiveTip>().update(|current| {
        if matches!(current, Some(anchor) if anchor.content.topic() == Some(topic)) {
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
    move |ev: PointerEvent| show(TipContent::Topic(topic), ev.client_x() as f64, ev.client_y() as f64)
}

pub fn on_pointer_leave(topic: Topic) -> impl Fn(PointerEvent) + Clone {
    move |_: PointerEvent| hide(TipContent::Topic(topic))
}

pub fn on_focus_in(topic: Topic) -> impl Fn(FocusEvent) + Clone {
    move |ev: FocusEvent| {
        let (x, y) = element_anchor(ev.target());
        show(TipContent::Topic(topic), x, y);
    }
}

pub fn on_focus_out(topic: Topic) -> impl Fn(FocusEvent) + Clone {
    move |_: FocusEvent| hide(TipContent::Topic(topic))
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

/// Like `Tip`, but `topic` and `detail` are reactive: used where the glossary entry to show
/// depends on live state (e.g. Declare's tooltip switches topic with the selected action, and
/// appends computed tick numbers for a sorcery sequence).
#[component]
pub fn DetailTip(
    #[prop(into)] topic: Signal<Topic>,
    #[prop(into)] detail: Signal<String>,
    children: Children,
) -> impl IntoView {
    let content = move || {
        let topic = topic.get_untracked();
        match detail.get_untracked() {
            detail if detail.is_empty() => TipContent::Topic(topic),
            detail => TipContent::TopicWithDetail(topic, detail),
        }
    };
    view! {
        <span
            class="tip-target"
            tabindex="0"
            on:pointerenter=move |ev: PointerEvent| show(content(), ev.client_x() as f64, ev.client_y() as f64)
            on:pointerleave=move |_: PointerEvent| hide_topic(topic.get_untracked())
            on:focusin=move |ev: FocusEvent| {
                let (x, y) = element_anchor(ev.target());
                show(content(), x, y);
            }
            on:focusout=move |_: FocusEvent| hide_topic(topic.get_untracked())
        >
            {children()}
        </span>
    }
}

/// Like `Tip`, but shows free-form `text` instead of a glossary entry, and ignores Teaching mode —
/// used for dynamic content (e.g. event-log detail) that isn't teaching material.
#[component]
pub fn TextTip(text: String, children: Children) -> impl IntoView {
    let enter_text = text.clone();
    let leave_text = text.clone();
    let focus_in_text = text.clone();
    let focus_out_text = text;
    view! {
        <span
            class="tip-target"
            tabindex="0"
            on:pointerenter=move |ev: PointerEvent| {
                show(TipContent::Text(enter_text.clone()), ev.client_x() as f64, ev.client_y() as f64)
            }
            on:pointerleave=move |_: PointerEvent| hide(TipContent::Text(leave_text.clone()))
            on:focusin=move |ev: FocusEvent| {
                let (x, y) = element_anchor(ev.target());
                show(TipContent::Text(focus_in_text.clone()), x, y);
            }
            on:focusout=move |_: FocusEvent| hide(TipContent::Text(focus_out_text.clone()))
        >
            {children()}
        </span>
    }
}

fn render_topic(topic: Topic, detail: Option<String>) -> AnyView {
    let entry = topic.entry();
    let (quote, cite_label) = match entry.source {
        Source::Book { quote, cite } => (quote, Some(cite.label())),
        Source::AppConvention => (None, None),
    };
    view! {
        <div class="tip-term">{entry.term}</div>
        <div class="tip-what">{entry.what}</div>
        <div class="tip-interacts">{entry.interacts}</div>
        {detail.map(|d| view! { <div class="tip-detail">{d}</div> })}
        {quote.map(|q| view! { <div class="tip-quote">{format!("“{q}”")}</div> })}
        {cite_label.map(|c| view! { <div class="tip-cite">{c}</div> })}
    }
        .into_any()
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

    let content = move || active.get().map(|anchor| anchor.content);

    view! {
        <div class="tip-layer" class:tip-layer-visible=move || content().is_some() style=position_style>
            {move || {
                content()
                    .map(|content| match content {
                        TipContent::Topic(topic) => render_topic(topic, None),
                        TipContent::TopicWithDetail(topic, detail) => render_topic(topic, Some(detail)),
                        TipContent::Text(text) => view! { <div class="tip-what">{text}</div> }.into_any(),
                    })
            }}
        </div>
    }
}

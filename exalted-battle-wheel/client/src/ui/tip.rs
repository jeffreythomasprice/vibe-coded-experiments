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
/// not teaching content and always shows.
#[derive(Debug, Clone, PartialEq)]
pub enum TipContent {
    Topic(TopicTip),
    Text(String),
}

/// A `Topic` plus two independent optional lines. `detail` is a computed line (e.g. projected tick
/// numbers) appended to the tooltip — still cited and Teaching-gated. `notice` is why the control
/// this tip is attached to is currently disabled: unlike the rest of a topic tip, it is never
/// gated behind Teaching mode, since a dead button has to explain itself either way.
#[derive(Debug, Clone, PartialEq)]
pub struct TopicTip {
    pub topic: Topic,
    pub notice: Option<String>,
    pub detail: Option<String>,
}

impl TipContent {
    fn topic(&self) -> Option<Topic> {
        match self {
            TipContent::Topic(tip) => Some(tip.topic),
            TipContent::Text(_) => None,
        }
    }

    fn notice(&self) -> Option<&str> {
        match self {
            TipContent::Topic(tip) => tip.notice.as_deref(),
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
    let gated_by_teaching = content.topic().is_some() && content.notice().is_none();
    if gated_by_teaching && !expect_context::<Prefs>().teaching_mode.get_untracked() {
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

/// Clears whatever tip is showing, unconditionally. For a widget that opens a popup of its own
/// (e.g. a combobox's suggestion list), which the tip layer would otherwise sit on top of.
pub fn dismiss() {
    expect_context::<ActiveTip>().set(None);
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

fn plain_topic(topic: Topic) -> TipContent {
    TipContent::Topic(TopicTip { topic, notice: None, detail: None })
}

pub fn on_pointer_enter(topic: Topic) -> impl Fn(PointerEvent) + Clone {
    move |ev: PointerEvent| show(plain_topic(topic), ev.client_x() as f64, ev.client_y() as f64)
}

pub fn on_pointer_leave(topic: Topic) -> impl Fn(PointerEvent) + Clone {
    move |_: PointerEvent| hide(plain_topic(topic))
}

pub fn on_focus_in(topic: Topic) -> impl Fn(FocusEvent) + Clone {
    move |ev: FocusEvent| {
        let (x, y) = element_anchor(ev.target());
        show(plain_topic(topic), x, y);
    }
}

pub fn on_focus_out(topic: Topic) -> impl Fn(FocusEvent) + Clone {
    move |_: FocusEvent| hide(plain_topic(topic))
}

/// Free-text counterparts of `on_pointer_*`/`on_focus_*`, for SVG nodes that need `TextTip`'s
/// free-form, non-teaching-gated content but — like the `Topic` variants above — cannot host a
/// wrapping `<span>`. Take a `Signal` rather than a plain `String` so a target whose tip text
/// depends on live state (e.g. a wheel token's occupant list) reads it fresh on every event
/// instead of baking in whatever it was when the handler was built.
///
/// `hide`'s content-equality check (above) means that if the text changes while the pointer is
/// still resting on the target, the eventual leave reads the new text and won't match the shown
/// anchor, leaving that tip stranded until the next `show`. Same tradeoff `hide_topic` accepts
/// below; needs an off-pointer state change to trigger and hasn't been worth more machinery for.
pub fn on_pointer_enter_text(text: impl Into<Signal<String>>) -> impl Fn(PointerEvent) + Clone {
    let text = text.into();
    move |ev: PointerEvent| show(TipContent::Text(text.get_untracked()), ev.client_x() as f64, ev.client_y() as f64)
}

pub fn on_pointer_leave_text(text: impl Into<Signal<String>>) -> impl Fn(PointerEvent) + Clone {
    let text = text.into();
    move |_: PointerEvent| hide(TipContent::Text(text.get_untracked()))
}

pub fn on_focus_in_text(text: impl Into<Signal<String>>) -> impl Fn(FocusEvent) + Clone {
    let text = text.into();
    move |ev: FocusEvent| {
        let (x, y) = element_anchor(ev.target());
        show(TipContent::Text(text.get_untracked()), x, y);
    }
}

pub fn on_focus_out_text(text: impl Into<Signal<String>>) -> impl Fn(FocusEvent) + Clone {
    let text = text.into();
    move |_: FocusEvent| hide(TipContent::Text(text.get_untracked()))
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
/// appends computed tick numbers for a sorcery sequence). `notice` is likewise reactive, for a
/// control whose disabled reason can change (e.g. Advance Tick before Start Battle).
#[component]
pub fn DetailTip(
    #[prop(into)] topic: Signal<Topic>,
    #[prop(into)] detail: Signal<String>,
    #[prop(into, optional)] notice: Signal<String>,
    children: Children,
) -> impl IntoView {
    let content = move || {
        TipContent::Topic(TopicTip {
            topic: topic.get_untracked(),
            notice: Some(notice.get_untracked()).filter(|n| !n.is_empty()),
            detail: Some(detail.get_untracked()).filter(|d| !d.is_empty()),
        })
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

/// `notice` renders unconditionally, at the top, since it's why the control is disabled rather
/// than teaching material. The rest of the entry is teaching material and only renders when
/// `teaching` is true — for a notice shown with Teaching mode off, that leaves the notice as the
/// tooltip's only content.
fn render_topic(tip: TopicTip, teaching: bool) -> AnyView {
    let notice = tip.notice.map(|n| view! { <div class="tip-notice">{n}</div> });
    if !teaching {
        return view! { {notice} }.into_any();
    }
    let entry = tip.topic.entry();
    let (quote, cite_label) = match entry.source {
        Source::Book { quote, cite } => (quote, Some(cite.label())),
        Source::AppConvention => (None, None),
    };
    view! {
        {notice}
        <div class="tip-term">{entry.term}</div>
        <div class="tip-what">{entry.what}</div>
        <div class="tip-interacts">{entry.interacts}</div>
        {tip.detail.map(|d| view! { <div class="tip-detail">{d}</div> })}
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
    let teaching = move || expect_context::<Prefs>().teaching_mode.get();

    view! {
        <div class="tip-layer" class:tip-layer-visible=move || content().is_some() style=position_style>
            {move || {
                content()
                    .map(|content| match content {
                        TipContent::Topic(tip) => render_topic(tip, teaching()),
                        TipContent::Text(text) => view! { <div class="tip-what">{text}</div> }.into_any(),
                    })
            }}
        </div>
    }
}

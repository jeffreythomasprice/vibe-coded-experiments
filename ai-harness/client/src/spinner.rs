//! The one loading spinner every view uses, plus two placement wrappers for
//! the two shapes a wait takes in this app: a view's very first load, with
//! nothing else on screen yet ([`LoadingPanel`]), and a form that already
//! has content on screen but is waiting on something — loading its data, or
//! saving ([`BusyOverlay`], meant to sit alongside a `disabled` `<fieldset>`
//! wrapping that content).
//!
//! Not built with `view!`'s inert-element optimization like `icons.rs` — the
//! dial's rotation is driven by a plain CSS animation on `.spinner-dial` in
//! `style.css`, so there's no per-icon SVG string to intern here; a `<span>`
//! is enough.

use leptos::prelude::*;

/// The spinner primitive: a rotating dial, plus an optional label alongside
/// it (e.g. "Responding…", "Sending…"). `role="status"` with an
/// `aria-label` — always present, falling back to "Loading" — so a screen
/// reader announces the wait even when no visible label is given.
#[component]
pub fn Spinner(#[prop(optional, into)] label: Option<String>) -> impl IntoView {
    let aria_label = label.clone().unwrap_or_else(|| "Loading".to_string());
    view! {
        <span class="spinner" role="status" aria-label=aria_label>
            <span class="spinner-dial" aria-hidden="true"></span>
            {label.map(|text| view! { <span class="spinner-label">{text}</span> })}
        </span>
    }
}

/// A view's very first load, before it has anything else to show — the
/// spinner centered on its own in the main content area. Replaces the ad
/// hoc `<p class="loading">"Loading…"</p>` every view used to write by hand.
#[component]
pub fn LoadingPanel(#[prop(into)] label: String) -> impl IntoView {
    view! {
        <div class="loading-panel">
            <Spinner label=label />
        </div>
    }
}

/// A scrim over already-rendered content, with the spinner centered on top
/// — for a form that's waiting on something (its data loading, or a save in
/// flight) but whose content is still worth showing underneath. Pair this
/// with a `disabled` `<fieldset>` around that content: the overlay is
/// visual only and blocks no input on its own.
#[component]
pub fn BusyOverlay(#[prop(into)] label: String) -> impl IntoView {
    view! {
        <div class="busy-overlay">
            <Spinner label=label />
        </div>
    }
}

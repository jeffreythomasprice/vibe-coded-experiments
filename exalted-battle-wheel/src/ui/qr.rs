//! Renders a `qrcodegen::QrCode` as one accumulated SVG `<path>` rather than one `<rect>` per
//! module — an invite QR easily has 1000+ modules, and a single path is far cheaper to build and
//! paint than that many DOM nodes.

use leptos::prelude::*;
use qrcodegen::{QrCode, QrCodeEcc};

/// Modules of white margin around the code on every side — the "quiet zone" scanners rely on to
/// find the code's edges at all.
const QUIET_ZONE: i32 = 4;

#[component]
pub fn Qr(text: Signal<String>) -> impl IntoView {
    move || {
        let text = text.get();
        // Only ever `Err` for input far past what an invite code reaches (a few thousand
        // characters even for the verbatim SDP fallback, well under `QrCodeEcc::Medium`'s
        // ~2300-byte ceiling) — nothing sensible to show instead, so the QR just quietly doesn't
        // render.
        let qr = QrCode::encode_text(&text, QrCodeEcc::Medium).ok()?;
        let size = qr.size();

        let mut path = String::new();
        for y in 0..size {
            for x in 0..size {
                if qr.get_module(x, y) {
                    path.push_str(&format!("M{x},{y}h1v1h-1z"));
                }
            }
        }

        let dimension = size + QUIET_ZONE * 2;
        let view_box = format!("-{QUIET_ZONE} -{QUIET_ZONE} {dimension} {dimension}");

        // Literal black-on-white, deliberately ignoring theme tokens: an inverted QR (light
        // modules on a dark ground) fails to scan on many phone cameras.
        Some(view! {
            <svg class="qr-code" viewBox=view_box xmlns="http://www.w3.org/2000/svg">
                <rect x=-QUIET_ZONE y=-QUIET_ZONE width=dimension height=dimension fill="#fff" />
                <path d=path fill="#000" />
            </svg>
        })
    }
}

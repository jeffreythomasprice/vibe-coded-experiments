//! Inline SVG source, one `&'static str` per icon, handed to `inner_html`.
//!
//! Not built with `view!`: a fully-static SVG subtree is eligible for the
//! macro's inert-element optimization, which compiles it to a string and sets
//! it via `innerHTML` anyway — and whether that path is taken flips the
//! moment one attribute on the element becomes dynamic (e.g. adding
//! `class:active` to the surrounding button). Writing the string here keeps
//! the construction fixed and puts every icon in one greppable place. Same
//! mechanism `views/conversation.rs` already uses for rendered markdown.
//!
//! These are compile-time constants, so `inner_html` carries no injection
//! risk here, unlike `crate::markdown`'s model-output input.
//!
//! House style: a 24x24 `viewBox`, `stroke="currentColor"`, no hardcoded
//! color — an icon inherits `--fg` (or `--accent-fg` inside an active
//! sidebar row) from whatever contains it, so it follows the theme without
//! `crate::theme` knowing icons exist. Size comes from CSS, not attributes,
//! so one constant serves every call site. `aria-hidden` because every icon
//! here sits next to a real text label.

/// Settings. Adapted from the Feather icon set (MIT licensed).
pub const GEAR: &str = concat!(
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" "#,
    r#"stroke="currentColor" stroke-width="2" stroke-linecap="round" "#,
    r#"stroke-linejoin="round" aria-hidden="true" focusable="false">"#,
    r#"<circle cx="12" cy="12" r="3"/>"#,
    r#"<path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06"#,
    r#"a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 "#,
    r#"1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06"#,
    r#"a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 "#,
    r#"1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06"#,
    r#"a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 "#,
    r#"1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06"#,
    r#"a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 "#,
    r#"1.65 0 0 0-1.51 1z"/>"#,
    r#"</svg>"#,
);

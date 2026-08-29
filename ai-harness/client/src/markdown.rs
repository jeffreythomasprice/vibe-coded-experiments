//! Markdown-to-HTML for assistant and user messages.
//!
//! Two entry points share one parser and one security posture:
//! [`render`] for assistant output (strict CommonMark soft breaks) and
//! [`render_with_breaks`] for user-composed text (soft breaks promoted to
//! `<br />`, since a composer newline is a deliberate line break, not
//! CommonMark's paragraph-reflow hint).
//!
//! Model *and user* output are both untrusted input here: `server/tauri.conf.json` sets
//! `csp: null` and `withGlobalTauri: true`, so any script that reached the
//! webview's DOM would have `window.__TAURI__.core.invoke` sitting right
//! there to call. Raw HTML in the source (a literal `<script>`, an `onerror`
//! attribute) is therefore rendered as inert text, never passed through —
//! there is no separate sanitization pass, because none is needed once the
//! parser itself never emits an `Html`/`InlineHtml` event as one.

use pulldown_cmark::{html, Event, Options, Parser};

/// Render `source` to an HTML string safe to hand to Leptos's `inner_html`.
pub fn render(source: &str) -> String {
    render_with(source, false)
}

/// Render `source` the way [`render`] does, but with every soft break (a
/// single newline) promoted to a `<br />`.
///
/// This is the *user* composer's rendering: someone who pressed Shift+Enter
/// meant a line break, not the paragraph-reflow a strict CommonMark reader
/// would give them. Assistant output deliberately does not go through here
/// — see [`render`].
pub fn render_with_breaks(source: &str) -> String {
    render_with(source, true)
}

fn render_with(source: &str, hard_breaks: bool) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options).map(move |event| match event {
        // Raw HTML (a fenced block of it, or an inline tag) is downgraded to
        // literal text rather than emitted — see this module's doc. This
        // applies on both rendering paths: it's the module's whole security
        // posture, not an assistant-only concern.
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
        Event::SoftBreak if hard_breaks => Event::HardBreak,
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, parser);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_and_paragraphs_render() {
        let html = render("# Title\n\nSome *text*.");
        assert!(html.contains("<h1>Title</h1>"), "got: {html}");
        assert!(html.contains("<em>text</em>"), "got: {html}");
    }

    #[test]
    fn lists_render() {
        let html = render("- one\n- two\n");
        assert!(html.contains("<ul>"), "got: {html}");
        assert!(html.contains("<li>one</li>"), "got: {html}");
    }

    #[test]
    fn fenced_code_blocks_render_as_pre_code() {
        let html = render("```rust\nfn main() {}\n```");
        assert!(html.contains("<pre><code"), "got: {html}");
        assert!(html.contains("fn main"), "got: {html}");
    }

    #[test]
    fn tables_render_when_the_extension_is_enabled() {
        let html = render("| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(html.contains("<table>"), "got: {html}");
    }

    #[test]
    fn a_literal_script_tag_is_escaped_rather_than_emitted_live() {
        let html = render("<script>alert(1)</script>");
        assert!(!html.contains("<script>"), "script tag leaked into output: {html}");
        assert!(html.contains("&lt;script&gt;"), "got: {html}");
    }

    #[test]
    fn an_inline_html_attribute_injection_is_escaped() {
        let html = render("before <img src=x onerror=\"alert(1)\"> after");
        assert!(!html.contains("<img"), "raw img tag leaked into output: {html}");
        assert!(html.contains("&lt;img"), "got: {html}");
    }

    #[test]
    fn render_with_breaks_promotes_a_single_newline_to_a_br() {
        let html = render_with_breaks("line one\nline two");
        assert!(html.contains("line one<br"), "got: {html}");
        assert!(html.contains("line two"), "got: {html}");
    }

    #[test]
    fn render_leaves_a_single_newline_as_a_soft_break() {
        // The assistant path must not gain hard breaks — a model wraps
        // prose with bare newlines it doesn't intend as line breaks.
        let html = render("line one\nline two");
        assert!(!html.contains("<br"), "got: {html}");
    }

    #[test]
    fn both_renderers_still_split_a_blank_line_into_two_paragraphs() {
        let with_breaks = render_with_breaks("para one\n\npara two");
        let strict = render("para one\n\npara two");
        for html in [&with_breaks, &strict] {
            assert_eq!(html.matches("<p>").count(), 2, "got: {html}");
        }
    }

    #[test]
    fn render_with_breaks_still_renders_ordinary_markdown() {
        let html = render_with_breaks("- one\n- two\n\n```rust\nfn main() {}\n```\n\nSome *text*.");
        assert!(html.contains("<li>one</li>"), "got: {html}");
        assert!(html.contains("<pre><code"), "got: {html}");
        assert!(html.contains("<em>text</em>"), "got: {html}");
    }

    #[test]
    fn render_with_breaks_still_escapes_a_literal_script_tag() {
        let html = render_with_breaks("<script>alert(1)</script>");
        assert!(!html.contains("<script>"), "script tag leaked into output: {html}");
        assert!(html.contains("&lt;script&gt;"), "got: {html}");
    }

    #[test]
    fn render_with_breaks_still_escapes_an_inline_html_attribute_injection() {
        let html = render_with_breaks("before <img src=x onerror=\"alert(1)\"> after");
        assert!(!html.contains("<img"), "raw img tag leaked into output: {html}");
        assert!(html.contains("&lt;img"), "got: {html}");
    }
}

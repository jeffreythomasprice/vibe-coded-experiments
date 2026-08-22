//! Markdown-to-HTML for assistant messages.
//!
//! Model output is untrusted input here: `server/tauri.conf.json` sets
//! `csp: null` and `withGlobalTauri: true`, so any script that reached the
//! webview's DOM would have `window.__TAURI__.core.invoke` sitting right
//! there to call. Raw HTML in the source (a literal `<script>`, an `onerror`
//! attribute) is therefore rendered as inert text, never passed through —
//! there is no separate sanitization pass, because none is needed once the
//! parser itself never emits an `Html`/`InlineHtml` event as one.

use pulldown_cmark::{html, Event, Options, Parser};

/// Render `source` to an HTML string safe to hand to Leptos's `inner_html`.
pub fn render(source: &str) -> String {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(source, options).map(|event| match event {
        // Raw HTML (a fenced block of it, or an inline tag) is downgraded to
        // literal text rather than emitted — see this module's doc.
        Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
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
}

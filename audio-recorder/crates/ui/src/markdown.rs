use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};

/// Renders model output to HTML without ever passing through raw HTML, links or images.
pub fn to_html(markdown: &str) -> String {
    let events = Parser::new_ext(markdown, Options::ENABLE_STRIKETHROUGH).filter_map(|event| match event {
        Event::Html(text) | Event::InlineHtml(text) => Some(Event::Text(text)),
        Event::Start(Tag::Link { .. } | Tag::Image { .. }) | Event::End(TagEnd::Link | TagEnd::Image) => None,
        other => Some(other),
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

#[cfg(test)]
mod tests {
    use super::to_html;

    #[test]
    fn renders_headings_and_lists() {
        let html = to_html("## Summary\n\nHello **world**\n\n- [00:01:02] one\n- two\n");
        assert!(html.contains("<h2>Summary</h2>"));
        assert!(html.contains("<strong>world</strong>"));
        assert!(html.contains("<li>[00:01:02] one</li>"));
    }

    #[test]
    fn raw_html_is_escaped() {
        let html = to_html("<script>alert(1)</script>\n\ntext <img src=x onerror=alert(1)> more");
        assert!(!html.contains("<script"));
        assert!(!html.contains("<img"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn links_and_images_keep_only_their_text() {
        let html = to_html("[click](javascript:alert(1)) and ![alt](http://x/y.png)");
        assert!(!html.contains("<a"));
        assert!(!html.contains("href"));
        assert!(!html.contains("<img"));
        assert!(html.contains("click"));
    }
}

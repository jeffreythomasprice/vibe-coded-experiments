//! Plain-text and markdown output.

use super::blocks::Block;

/// The canonical text with page-break form feeds turned into blank lines.
/// Nothing else is touched, so the output stays faithful to what was ingested.
pub fn to_txt(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 1);
    for c in text.replace("\r\n", "\n").chars() {
        if c == '\u{c}' {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    let trimmed = out.trim_end();
    format!("{trimmed}\n")
}

/// Markdown for `blocks`: `#` headings, `- ` list items, escaped paragraphs.
pub fn to_markdown(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut prev_list = false;
    for block in blocks {
        let is_list = matches!(block, Block::ListItem(_));
        if !out.is_empty() {
            // Consecutive list items stay tight; everything else is
            // separated by a blank line.
            out.push_str(if is_list && prev_list { "\n" } else { "\n\n" });
        }
        match block {
            Block::Heading { level, text } => {
                out.push_str(&"#".repeat(usize::from((*level).clamp(1, 6))));
                out.push(' ');
                out.push_str(&escape_inline(text));
            }
            Block::Paragraph(text) => out.push_str(&escape_line_start(&escape_inline(text))),
            Block::ListItem(text) => {
                out.push_str("- ");
                out.push_str(&escape_inline(text));
            }
        }
        prev_list = is_list;
    }
    out.push('\n');
    out
}

/// Characters that would start emphasis, code, links or raw HTML anywhere
/// in a line.
fn escape_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '[' | ']' | '<') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A paragraph that begins like a heading, quote, list or table must not
/// become one.
fn escape_line_start(s: &str) -> String {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    if matches!(first, '#' | '>' | '-' | '+' | '|' | '=' | '~') {
        return format!("\\{s}");
    }
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let rest = &s[digits..];
        if rest.starts_with('.') || rest.starts_with(')') {
            return format!("{}\\{rest}", &s[..digits]);
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_turns_form_feeds_into_one_blank_line() {
        assert_eq!(to_txt("a\n\u{c}b\n"), "a\n\nb\n");
        assert_eq!(to_txt("a\u{c}b"), "a\n\nb\n");
        assert_eq!(to_txt("a  \n\n\n"), "a\n");
    }

    #[test]
    fn markdown_has_headings_tight_lists_and_blank_line_paragraphs() {
        let blocks = vec![
            Block::Heading {
                level: 2,
                text: "Intro".into(),
            },
            Block::Paragraph("Body text.".into()),
            Block::ListItem("one".into()),
            Block::ListItem("two".into()),
            Block::Paragraph("After.".into()),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "## Intro\n\nBody text.\n\n- one\n- two\n\nAfter.\n"
        );
    }

    #[test]
    fn markdown_escapes_accidental_syntax() {
        let blocks = vec![
            Block::Paragraph("# not a heading".into()),
            Block::Paragraph("1. not a list".into()),
            Block::Paragraph("snake_case and *stars* and [x]".into()),
            Block::Paragraph("-5 degrees".into()),
        ];
        let md = to_markdown(&blocks);
        assert!(md.contains("\\# not a heading"), "{md}");
        assert!(md.contains("1\\. not a list"), "{md}");
        assert!(
            md.contains("snake\\_case and \\*stars\\* and \\[x\\]"),
            "{md}"
        );
        assert!(md.contains("\\-5 degrees"), "{md}");
    }
}

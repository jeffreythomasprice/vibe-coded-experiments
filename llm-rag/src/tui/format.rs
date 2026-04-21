//! Shared formatting helpers used by both the streaming re-render path and
//! the history replay path so the live and post-`ChatDone` views match
//! byte-for-byte.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Maximum characters to show for tool-call arguments or a tool result before
/// mid-ellipsis truncation kicks in. Kept tight so huge JSON payloads don't
/// blow out the transcript.
pub const TOOL_SEGMENT_MAX: usize = 80;

/// `▸ {name}({truncated_args})` — styled yellow + DIM.
pub fn tool_call_line(name: &str, args: &str) -> Line<'static> {
    let truncated = truncate_middle(args, TOOL_SEGMENT_MAX);
    Line::from(Span::styled(
        format!("▸ {name}({truncated})"),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::DIM),
    ))
}

/// `  ↳ {truncated_content}` — styled magenta + DIM.
pub fn tool_result_line(content: &str) -> Line<'static> {
    let truncated = truncate_middle(content, TOOL_SEGMENT_MAX);
    Line::from(Span::styled(
        format!("  ↳ {truncated}"),
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::DIM),
    ))
}

/// UTF-8 safe mid-ellipsis truncation measured in chars, not bytes.
///
/// If the input is already short enough it is returned as-is. Otherwise the
/// output is exactly `max` chars long including the `…` separator; the prefix
/// gets the larger half when the remaining budget is odd.
pub fn truncate_middle(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max {
        return s.to_string();
    }
    // Reserve one char for the ellipsis; weight the prefix a little heavier so
    // opening tokens (e.g., `{"a":`) stay visible.
    let budget = max.saturating_sub(1);
    let prefix_len = budget.div_ceil(2);
    let suffix_len = budget - prefix_len;
    let prefix: String = s.chars().take(prefix_len).collect();
    let suffix_rev: Vec<char> = s.chars().rev().take(suffix_len).collect();
    let suffix: String = suffix_rev.into_iter().rev().collect();
    format!("{prefix}…{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_middle_noop_when_short() {
        assert_eq!(truncate_middle("hello", 10), "hello");
        assert_eq!(truncate_middle("hello", 5), "hello");
    }

    #[test]
    fn truncate_middle_applies_ellipsis_and_exact_length() {
        let s = "abcdefghijklmnop";
        let t = truncate_middle(s, 7);
        assert_eq!(t.chars().count(), 7);
        assert_eq!(t, "abc…nop");
    }

    #[test]
    fn truncate_middle_is_utf8_safe_on_multibyte() {
        let s = "😀😁😂😃😄😅😆😇😈😉😊😋";
        let t = truncate_middle(s, 5);
        assert_eq!(t.chars().count(), 5);
        assert!(t.contains('…'));
    }

    #[test]
    fn tool_call_line_truncates_large_args() {
        let huge = "x".repeat(500);
        let line = tool_call_line("add", &huge);
        let rendered = line.to_string();
        assert!(rendered.contains('…'));
        // name + parens + truncated args + "▸ "
        assert!(rendered.chars().count() < 120);
    }

    #[test]
    fn tool_result_line_truncates_large_content() {
        let huge = "y".repeat(500);
        let line = tool_result_line(&huge);
        let rendered = line.to_string();
        assert!(rendered.contains('…'));
    }
}

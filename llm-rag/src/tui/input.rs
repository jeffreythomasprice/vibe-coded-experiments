//! Readline-style editing primitives shared by the Chat / Search / TagEditor
//! screens. Each of those screens holds a `String` buffer and a `usize` byte
//! cursor; keeping the logic here means the cursor-on-UTF-8-boundary
//! invariant and the "did the buffer change?" signal live in exactly one
//! place.
//!
//! Mutations return `bool = "buf changed"` so callers can conditionally run
//! side effects like `refresh_autocomplete` / `refresh_suggestions` without a
//! before/after compare. Cursor-only ops return `()`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn insert_char(buf: &mut String, cursor: &mut usize, ch: char) -> bool {
    buf.insert(*cursor, ch);
    *cursor += ch.len_utf8();
    true
}

pub fn delete_char_backward(buf: &mut String, cursor: &mut usize) -> bool {
    if *cursor == 0 {
        return false;
    }
    let prev = buf[..*cursor]
        .chars()
        .next_back()
        .map(char::len_utf8)
        .unwrap_or(0);
    if prev == 0 {
        return false;
    }
    let new_cursor = *cursor - prev;
    buf.replace_range(new_cursor..*cursor, "");
    *cursor = new_cursor;
    true
}

pub fn delete_char_forward(buf: &mut String, cursor: &mut usize) -> bool {
    let next = buf[*cursor..]
        .chars()
        .next()
        .map(char::len_utf8)
        .unwrap_or(0);
    if next == 0 {
        return false;
    }
    buf.replace_range(*cursor..*cursor + next, "");
    true
}

/// bash `unix-word-rubout`: from the cursor, skip any run of whitespace
/// going backward, then delete the contiguous run of non-whitespace.
pub fn delete_word_backward(buf: &mut String, cursor: &mut usize) -> bool {
    if *cursor == 0 {
        return false;
    }
    let mut i = *cursor;
    while let Some(ch) = buf[..i].chars().next_back() {
        if !ch.is_whitespace() {
            break;
        }
        i -= ch.len_utf8();
    }
    while let Some(ch) = buf[..i].chars().next_back() {
        if ch.is_whitespace() {
            break;
        }
        i -= ch.len_utf8();
    }
    if i == *cursor {
        return false;
    }
    buf.replace_range(i..*cursor, "");
    *cursor = i;
    true
}

pub fn delete_to_start(buf: &mut String, cursor: &mut usize) -> bool {
    if *cursor == 0 {
        return false;
    }
    buf.replace_range(0..*cursor, "");
    *cursor = 0;
    true
}

pub fn delete_to_end(buf: &mut String, cursor: &mut usize) -> bool {
    if *cursor >= buf.len() {
        return false;
    }
    buf.truncate(*cursor);
    true
}

pub fn cursor_left(buf: &str, cursor: &mut usize) {
    let prev = buf[..*cursor]
        .chars()
        .next_back()
        .map(char::len_utf8)
        .unwrap_or(0);
    *cursor -= prev;
}

pub fn cursor_right(buf: &str, cursor: &mut usize) {
    let next = buf[*cursor..]
        .chars()
        .next()
        .map(char::len_utf8)
        .unwrap_or(0);
    *cursor += next;
}

pub fn cursor_home(cursor: &mut usize) {
    *cursor = 0;
}

pub fn cursor_end(buf: &str, cursor: &mut usize) {
    *cursor = buf.len();
}

/// Predicate for waiting-mode swallow filters: `true` iff `key` would mutate
/// a text buffer under our editing rules — a plain printable Char, Backspace,
/// Delete, or one of Ctrl+{W, U, K}. Intentionally excludes Enter (screens
/// treat Enter as submit, not a buffer edit) and all cursor-only keys.
pub fn is_buffer_edit(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::Backspace | KeyCode::Delete => true,
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                matches!(c, 'w' | 'u' | 'k')
            } else {
                true
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str, c: usize) -> (String, usize) {
        (s.to_string(), c)
    }

    #[test]
    fn word_backward_at_column_zero_is_noop() {
        let (mut b, mut c) = at("hello", 0);
        assert!(!delete_word_backward(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("hello", 0));
    }

    #[test]
    fn word_backward_empty_is_noop() {
        let (mut b, mut c) = at("", 0);
        assert!(!delete_word_backward(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("", 0));
    }

    #[test]
    fn word_backward_mid_word_deletes_to_previous_space() {
        // "foo b|ar" → "foo |ar"
        let (mut b, mut c) = at("foo bar", 5);
        assert!(delete_word_backward(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("foo ar", 4));
    }

    #[test]
    fn word_backward_at_end_deletes_trailing_word() {
        let (mut b, mut c) = at("foo bar baz", 11);
        assert!(delete_word_backward(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("foo bar ", 8));
    }

    #[test]
    fn word_backward_skips_trailing_whitespace() {
        // "foo   |" → "|"
        let (mut b, mut c) = at("foo   ", 6);
        assert!(delete_word_backward(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("", 0));
    }

    #[test]
    fn word_backward_all_whitespace() {
        let (mut b, mut c) = at("   ", 3);
        assert!(delete_word_backward(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("", 0));
    }

    #[test]
    fn word_backward_handles_utf8() {
        // "héllo wörld" with cursor at end (byte len).
        let s = "héllo wörld";
        let (mut b, mut c) = at(s, s.len());
        assert!(delete_word_backward(&mut b, &mut c));
        assert_eq!(b.as_str(), "héllo ");
        assert_eq!(c, "héllo ".len());
    }

    #[test]
    fn word_backward_treats_punctuation_as_word_part() {
        // bash-style: `foo-bar.baz` is one word.
        let s = "hello foo-bar.baz";
        let (mut b, mut c) = at(s, s.len());
        assert!(delete_word_backward(&mut b, &mut c));
        assert_eq!(b.as_str(), "hello ");
    }

    #[test]
    fn delete_to_start_clears_prefix() {
        let (mut b, mut c) = at("abc def", 4);
        assert!(delete_to_start(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("def", 0));
    }

    #[test]
    fn delete_to_start_at_zero_noop() {
        let (mut b, mut c) = at("abc", 0);
        assert!(!delete_to_start(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("abc", 0));
    }

    #[test]
    fn delete_to_end_clears_suffix() {
        let (mut b, mut c) = at("abc def", 3);
        assert!(delete_to_end(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("abc", 3));
    }

    #[test]
    fn delete_to_end_at_end_noop() {
        let (mut b, mut c) = at("abc", 3);
        assert!(!delete_to_end(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("abc", 3));
    }

    #[test]
    fn delete_char_forward_at_end_noop() {
        let (mut b, mut c) = at("abc", 3);
        assert!(!delete_char_forward(&mut b, &mut c));
        assert_eq!((b.as_str(), c), ("abc", 3));
    }

    #[test]
    fn delete_char_forward_utf8() {
        let s = "héllo";
        let (mut b, mut c) = at(s, 0);
        assert!(delete_char_forward(&mut b, &mut c));
        assert_eq!(b.as_str(), "éllo");
        assert_eq!(c, 0);
    }

    #[test]
    fn cursor_left_utf8_boundary() {
        let s = "héllo";
        let mut c = "hé".len();
        cursor_left(s, &mut c);
        assert_eq!(c, "h".len());
    }

    #[test]
    fn cursor_right_utf8_boundary() {
        let s = "héllo";
        let mut c = "h".len();
        cursor_right(s, &mut c);
        assert_eq!(c, "hé".len());
    }

    #[test]
    fn cursor_home_end() {
        let s = "hello world";
        let mut c = 3;
        cursor_home(&mut c);
        assert_eq!(c, 0);
        cursor_end(s, &mut c);
        assert_eq!(c, s.len());
    }

    #[test]
    fn is_buffer_edit_cases() {
        let plain_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let ctrl_w = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);
        let ctrl_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let backspace = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        let delete = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let left = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);

        assert!(is_buffer_edit(&plain_a));
        assert!(is_buffer_edit(&ctrl_w));
        assert!(is_buffer_edit(&ctrl_u));
        assert!(is_buffer_edit(&ctrl_k));
        assert!(is_buffer_edit(&backspace));
        assert!(is_buffer_edit(&delete));

        assert!(!is_buffer_edit(&ctrl_c));
        assert!(!is_buffer_edit(&ctrl_a));
        assert!(!is_buffer_edit(&enter));
        assert!(!is_buffer_edit(&left));
    }
}

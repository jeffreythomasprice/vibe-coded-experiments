//! Shared `Enter`-to-send keyboard handling for the message composer
//! `<textarea>`s in [`crate::views::Conversation`] and
//! [`crate::views::NewConversation`].
//!
//! Plain `Enter` submits; `Shift`/`Ctrl`/`Alt`/`Meta`+`Enter` insert a
//! newline instead. Every other key (including whatever a paste generates —
//! pasting doesn't synthesize per-character `keydown` events at all) falls
//! through untouched, which is what keeps a multi-line paste from ever
//! risking a submit.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlTextAreaElement, KeyboardEvent};

/// What an `Enter` press (or any other key) in a message composer should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// Send the message.
    Submit,
    /// Insert a literal newline at the caret — the browser wouldn't do this
    /// on its own for `Ctrl`/`Alt`/`Meta`+`Enter`.
    InsertNewline,
    /// Not ours; leave the browser's default handling alone.
    Pass,
}

/// Pure decision: given `key` and the modifier/IME state of a `keydown`,
/// what should the composer do?
///
/// `key` is the DOM `KeyboardEvent.key` string (e.g. `"Enter"`, `"a"`,
/// `"Tab"`). `composing` is `KeyboardEvent.isComposing` — true while an IME
/// candidate is being edited, in which case `Enter` confirms the candidate
/// rather than submitting.
pub fn key_action(key: &str, shift: bool, ctrl: bool, alt: bool, meta: bool, composing: bool) -> KeyAction {
    if key != "Enter" {
        return KeyAction::Pass;
    }
    if composing {
        return KeyAction::Pass;
    }
    if shift {
        // Already the browser default in every engine — let it happen
        // natively rather than re-implementing it.
        return KeyAction::Pass;
    }
    if ctrl || alt || meta {
        return KeyAction::InsertNewline;
    }
    KeyAction::Submit
}

/// Wire this onto a composer `<textarea>`'s `on:keydown`.
///
/// `message` is the signal backing the textarea's `prop:value`, kept in
/// sync here the same way `on:input` does elsewhere. `submit` is called with
/// no arguments on `Enter`; callers pass their own submit closure, which
/// still runs its own busy/empty-text guards.
pub fn keydown(ev: KeyboardEvent, message: RwSignal<String>, submit: impl Fn()) {
    let action = key_action(
        &ev.key(),
        ev.shift_key(),
        ev.ctrl_key(),
        ev.alt_key(),
        ev.meta_key(),
        ev.is_composing(),
    );
    match action {
        KeyAction::Pass => {}
        KeyAction::Submit => {
            ev.prevent_default();
            submit();
        }
        KeyAction::InsertNewline => {
            ev.prevent_default();
            insert_newline_at_caret(&ev, message);
        }
    }
}

/// Splice a newline in at the caret via `setRangeText` rather than editing
/// the Rust `String` directly: `selectionStart`/`selectionEnd` are UTF-16
/// code-unit offsets, while a Rust `String` is byte-indexed — letting the
/// browser do the splice sidesteps that mismatch entirely (and keeps the
/// native undo stack intact).
fn insert_newline_at_caret(ev: &KeyboardEvent, message: RwSignal<String>) {
    let Some(el) = ev.target().and_then(|t| t.dyn_into::<HtmlTextAreaElement>().ok()) else {
        return;
    };
    let Some(start) = el.selection_start().ok().flatten() else {
        return;
    };
    let end = el.selection_end().ok().flatten().unwrap_or(start);
    if el.set_range_text_with_start_and_end("\n", start, end).is_err() {
        return;
    }
    let caret = start + 1;
    let _ = el.set_selection_range(caret, caret);
    message.set(el.value());
}

#[cfg(test)]
mod tests {
    use super::*;

    // Plain Enter submits.
    #[test]
    fn plain_enter_submits() {
        assert_eq!(key_action("Enter", false, false, false, false, false), KeyAction::Submit);
    }

    // Shift+Enter is left to the browser's native newline.
    #[test]
    fn shift_enter_passes_through() {
        assert_eq!(key_action("Enter", true, false, false, false, false), KeyAction::Pass);
    }

    // Ctrl/Alt/Meta+Enter each insert a newline explicitly.
    #[test]
    fn ctrl_enter_inserts_newline() {
        assert_eq!(key_action("Enter", false, true, false, false, false), KeyAction::InsertNewline);
    }

    #[test]
    fn alt_enter_inserts_newline() {
        assert_eq!(key_action("Enter", false, false, true, false, false), KeyAction::InsertNewline);
    }

    #[test]
    fn meta_enter_inserts_newline() {
        assert_eq!(key_action("Enter", false, false, false, true, false), KeyAction::InsertNewline);
    }

    // Shift wins over Ctrl: the browser already inserts a newline for
    // Shift+Enter, so there's no need to intercept the combination just
    // because Ctrl is also held.
    #[test]
    fn ctrl_shift_enter_passes_through() {
        assert_eq!(key_action("Enter", true, true, false, false, false), KeyAction::Pass);
    }

    // IME candidate confirmation must never submit.
    #[test]
    fn composing_enter_passes_through() {
        assert_eq!(key_action("Enter", false, false, false, false, true), KeyAction::Pass);
    }

    // Non-Enter keys are always left alone, modifiers or not — this is the
    // paste regression guard: a paste never synthesizes an Enter keydown,
    // but Ctrl+V does synthesize a "v" keydown, which must pass through.
    #[test]
    fn other_keys_always_pass_through() {
        assert_eq!(key_action("a", false, false, false, false, false), KeyAction::Pass);
        assert_eq!(key_action("v", false, true, false, false, false), KeyAction::Pass);
        assert_eq!(key_action("Tab", false, false, false, false, false), KeyAction::Pass);
        assert_eq!(key_action("ArrowUp", false, false, false, false, false), KeyAction::Pass);
    }
}

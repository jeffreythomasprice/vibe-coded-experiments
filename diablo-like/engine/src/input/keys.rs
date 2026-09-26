use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use serde::de::IntoDeserializer;
use winit::keyboard::{KeyCode, NativeKeyCode, PhysicalKey};

use super::source::InputParseError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyboardKey {
    Code(KeyCode),
    /// A key winit could not map to a `KeyCode` at all. The native code is
    /// still enough to bind a keypress to, per `PhysicalKey`'s own docs:
    /// "it is also possible to use this for keybinds for non-standard keys,
    /// but such keybinds are tied to a given platform."
    Native(NativeKeyCode),
}

impl From<PhysicalKey> for KeyboardKey {
    fn from(key: PhysicalKey) -> Self {
        match key {
            PhysicalKey::Code(code) => KeyboardKey::Code(code),
            PhysicalKey::Unidentified(native) => KeyboardKey::Native(native),
        }
    }
}

/// `KeyCode` is `#[non_exhaustive]` with 194 variants and its names are the
/// stable W3C UI Events `code` values, so rather than a 194-row table this
/// parses/formats through that name directly (falling back to a small table
/// of aliases only where the W3C name makes a poor config token or label).
/// This means a future winit release's new variants are bindable for free.
macro_rules! key_overrides {
    ($($variant:ident => $token:literal, $label:literal;)+) => {
        fn table_token(code: KeyCode) -> Option<&'static str> {
            match code {
                $(KeyCode::$variant => Some($token),)+
                _ => None,
            }
        }

        fn table_label(code: KeyCode) -> Option<&'static str> {
            match code {
                $(KeyCode::$variant => Some($label),)+
                _ => None,
            }
        }

        fn table_canonical_name(alias: &str) -> Option<&'static str> {
            match alias {
                $($token => Some(stringify!($variant)),)+
                _ => None,
            }
        }
    };
}

key_overrides! {
    Backquote => "`", "`";
    Backslash => "\\", "\\";
    BracketLeft => "[", "[";
    BracketRight => "]", "]";
    Comma => ",", ",";
    Equal => "=", "=";
    Minus => "-", "-";
    Period => ".", ".";
    Quote => "'", "'";
    Semicolon => ";", ";";
    Slash => "/", "/";
    AltLeft => "LeftAlt", "Left Alt";
    AltRight => "RightAlt", "Right Alt";
    ControlLeft => "LeftCtrl", "Left Ctrl";
    ControlRight => "RightCtrl", "Right Ctrl";
    ShiftLeft => "LeftShift", "Left Shift";
    ShiftRight => "RightShift", "Right Shift";
    SuperLeft => "LeftSuper", "Left Super";
    SuperRight => "RightSuper", "Right Super";
    ContextMenu => "Menu", "Menu";
    Escape => "Escape", "Esc";
    NumpadAdd => "Numpad+", "Numpad +";
    NumpadSubtract => "Numpad-", "Numpad -";
    NumpadMultiply => "Numpad*", "Numpad *";
    NumpadDivide => "Numpad/", "Numpad /";
    NumpadDecimal => "Numpad.", "Numpad .";
    NumpadEqual => "Numpad=", "Numpad =";
    Numpad0 => "Numpad0", "Numpad 0";
    Numpad1 => "Numpad1", "Numpad 1";
    Numpad2 => "Numpad2", "Numpad 2";
    Numpad3 => "Numpad3", "Numpad 3";
    Numpad4 => "Numpad4", "Numpad 4";
    Numpad5 => "Numpad5", "Numpad 5";
    Numpad6 => "Numpad6", "Numpad 6";
    Numpad7 => "Numpad7", "Numpad 7";
    Numpad8 => "Numpad8", "Numpad 8";
    Numpad9 => "Numpad9", "Numpad 9";
}

/// Handles the bulk of the keyboard (letters, digits, arrows) by stripping
/// the W3C name's prefix rather than listing all 40 of them in the table
/// above: `KeyW` -> `W`, `Digit1` -> `1`, `ArrowUp` -> `Up`. Token and label
/// are identical for these, unlike the table's punctuation/modifier rows.
fn procedural_alias(code: KeyCode) -> Option<String> {
    let name = format!("{code:?}");
    if let Some(letter) = name.strip_prefix("Key") {
        if letter.len() == 1 {
            return Some(letter.to_string());
        }
    }
    if let Some(digit) = name.strip_prefix("Digit") {
        if digit.len() == 1 && digit.chars().all(|c| c.is_ascii_digit()) {
            return Some(digit.to_string());
        }
    }
    if let Some(dir) = name.strip_prefix("Arrow") {
        return Some(dir.to_string());
    }
    None
}

fn procedural_canonical(alias: &str) -> Option<String> {
    let mut chars = alias.chars();
    let first = chars.next()?;
    if chars.as_str().is_empty() {
        if first.is_ascii_uppercase() {
            return Some(format!("Key{alias}"));
        }
        if first.is_ascii_digit() {
            return Some(format!("Digit{alias}"));
        }
    }
    match alias {
        "Up" | "Down" | "Left" | "Right" => Some(format!("Arrow{alias}")),
        _ => None,
    }
}

/// Splits a W3C `code` name at camelCase boundaries so every key has *some*
/// label even when it's neither in the override table nor a letter/digit/
/// arrow: `BrowserBack` -> `Browser Back`, `AudioVolumeUp` -> `Audio Volume
/// Up`. Ugly for obscure keys, but per the design decision here, that beats
/// being unable to label (and therefore bind) them at all.
fn camel_case_label(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    let mut prev_lower = false;
    for c in name.chars() {
        if c.is_uppercase() && prev_lower {
            out.push(' ');
        }
        out.push(c);
        prev_lower = c.is_lowercase();
    }
    out
}

fn parse_key_code(name: &str) -> Option<KeyCode> {
    let deserializer: serde::de::value::StrDeserializer<'_, serde::de::value::Error> =
        name.into_deserializer();
    KeyCode::deserialize(deserializer).ok()
}

fn native_token(native: NativeKeyCode) -> String {
    match native {
        NativeKeyCode::Unidentified => "native.unidentified".to_string(),
        NativeKeyCode::Android(n) => format!("native.android.{n}"),
        NativeKeyCode::MacOS(n) => format!("native.macos.{n}"),
        NativeKeyCode::Windows(n) => format!("native.windows.{n}"),
        NativeKeyCode::Xkb(n) => format!("native.xkb.{n}"),
    }
}

fn native_label(native: NativeKeyCode) -> String {
    match native {
        NativeKeyCode::Unidentified => "Unknown Key".to_string(),
        NativeKeyCode::Android(n) => format!("Native Key (android {n})"),
        NativeKeyCode::MacOS(n) => format!("Native Key (macos {n})"),
        NativeKeyCode::Windows(n) => format!("Native Key (windows {n})"),
        NativeKeyCode::Xkb(n) => format!("Native Key (xkb {n})"),
    }
}

fn parse_native(rest: &str, whole: &str) -> Result<NativeKeyCode, InputParseError> {
    if rest == "unidentified" {
        return Ok(NativeKeyCode::Unidentified);
    }
    let (platform, code) = rest
        .split_once('.')
        .ok_or_else(|| InputParseError::BadNativeKeyCode(whole.to_string()))?;
    let code: u32 = code
        .parse()
        .map_err(|_| InputParseError::BadNativeKeyCode(whole.to_string()))?;
    match platform {
        "android" => Ok(NativeKeyCode::Android(code)),
        "macos" => Ok(NativeKeyCode::MacOS(code as u16)),
        "windows" => Ok(NativeKeyCode::Windows(code as u16)),
        "xkb" => Ok(NativeKeyCode::Xkb(code)),
        _ => Err(InputParseError::BadNativeKeyCode(whole.to_string())),
    }
}

impl KeyboardKey {
    pub fn label(self) -> String {
        match self {
            KeyboardKey::Code(code) => table_label(code)
                .map(str::to_string)
                .or_else(|| procedural_alias(code))
                .unwrap_or_else(|| camel_case_label(&format!("{code:?}"))),
            KeyboardKey::Native(native) => native_label(native),
        }
    }
}

impl fmt::Display for KeyboardKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyboardKey::Code(code) => {
                let token = table_token(*code)
                    .map(str::to_string)
                    .or_else(|| procedural_alias(*code))
                    .unwrap_or_else(|| format!("{code:?}"));
                f.write_str(&token)
            }
            KeyboardKey::Native(native) => f.write_str(&native_token(*native)),
        }
    }
}

impl FromStr for KeyboardKey {
    type Err = InputParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Unlike gamepad/mouse tokens, keys never take an axis-direction
        // suffix, and some real key symbols legitimately end in `+`/`-`
        // (`Numpad+`, `Numpad-`), so there's nothing to reject here.
        if let Some(rest) = s.strip_prefix("native.") {
            return parse_native(rest, s).map(KeyboardKey::Native);
        }
        let canonical = table_canonical_name(s)
            .map(str::to_string)
            .or_else(|| procedural_canonical(s))
            .unwrap_or_else(|| s.to_string());
        parse_key_code(&canonical)
            .map(KeyboardKey::Code)
            .ok_or_else(|| InputParseError::UnknownKey(s.to_string()))
    }
}

/// A representative subset for a settings UI to list (e.g. as a fallback
/// when not capturing a live keypress). Not exhaustive: `KeyCode` is
/// `#[non_exhaustive]` and has no iterator, so unlike the gamepad enums
/// there is no `ALL` for the full keyboard.
pub const COMMON: &[KeyCode] = &[
    KeyCode::KeyA,
    KeyCode::KeyB,
    KeyCode::KeyC,
    KeyCode::KeyD,
    KeyCode::KeyE,
    KeyCode::KeyF,
    KeyCode::KeyG,
    KeyCode::KeyH,
    KeyCode::KeyI,
    KeyCode::KeyJ,
    KeyCode::KeyK,
    KeyCode::KeyL,
    KeyCode::KeyM,
    KeyCode::KeyN,
    KeyCode::KeyO,
    KeyCode::KeyP,
    KeyCode::KeyQ,
    KeyCode::KeyR,
    KeyCode::KeyS,
    KeyCode::KeyT,
    KeyCode::KeyU,
    KeyCode::KeyV,
    KeyCode::KeyW,
    KeyCode::KeyX,
    KeyCode::KeyY,
    KeyCode::KeyZ,
    KeyCode::Digit0,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::Escape,
    KeyCode::Enter,
    KeyCode::Space,
    KeyCode::Tab,
    KeyCode::Backspace,
    KeyCode::Delete,
    KeyCode::Insert,
    KeyCode::Home,
    KeyCode::End,
    KeyCode::PageUp,
    KeyCode::PageDown,
    KeyCode::ShiftLeft,
    KeyCode::ShiftRight,
    KeyCode::ControlLeft,
    KeyCode::ControlRight,
    KeyCode::AltLeft,
    KeyCode::AltRight,
    KeyCode::SuperLeft,
    KeyCode::SuperRight,
    KeyCode::F1,
    KeyCode::F2,
    KeyCode::F3,
    KeyCode::F4,
    KeyCode::F5,
    KeyCode::F6,
    KeyCode::F7,
    KeyCode::F8,
    KeyCode::F9,
    KeyCode::F10,
    KeyCode::F11,
    KeyCode::F12,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// This is the test that catches a future winit bump breaking the
    /// table-free format: `parse_key_code` relies on the `Debug` name and
    /// the serde (W3C) name coinciding for every variant.
    #[test]
    fn debug_name_and_serde_name_agree() {
        let sample = [
            KeyCode::KeyW,
            KeyCode::Digit1,
            KeyCode::ArrowUp,
            KeyCode::BracketLeft,
            KeyCode::BrowserBack,
            KeyCode::AudioVolumeUp,
            KeyCode::F30,
            KeyCode::IntlYen,
            KeyCode::KanaMode,
            KeyCode::NumpadAdd,
        ];
        for code in sample {
            let debug_name = format!("{code:?}");
            assert_eq!(parse_key_code(&debug_name), Some(code), "{debug_name}");
        }
    }

    #[test]
    fn table_rows_round_trip() {
        for &code in &[
            KeyCode::Backquote,
            KeyCode::BracketLeft,
            KeyCode::AltLeft,
            KeyCode::ControlRight,
            KeyCode::ShiftLeft,
            KeyCode::SuperRight,
            KeyCode::ContextMenu,
            KeyCode::Escape,
            KeyCode::NumpadAdd,
            KeyCode::Numpad3,
        ] {
            let key = KeyboardKey::Code(code);
            let token = key.to_string();
            assert_eq!(
                token.parse::<KeyboardKey>().unwrap(),
                key,
                "token {token:?}"
            );
        }
    }

    #[test]
    fn procedural_keys_round_trip() {
        for &code in &[
            KeyCode::KeyW,
            KeyCode::KeyZ,
            KeyCode::Digit1,
            KeyCode::Digit0,
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
        ] {
            let key = KeyboardKey::Code(code);
            let token = key.to_string();
            assert_eq!(
                token.parse::<KeyboardKey>().unwrap(),
                key,
                "token {token:?}"
            );
        }
        assert_eq!(KeyboardKey::Code(KeyCode::KeyW).to_string(), "W");
        assert_eq!(KeyboardKey::Code(KeyCode::Digit1).to_string(), "1");
        assert_eq!(KeyboardKey::Code(KeyCode::ArrowUp).to_string(), "Up");
    }

    #[test]
    fn canonical_names_still_parse() {
        assert_eq!(
            "AltLeft".parse::<KeyboardKey>().unwrap(),
            KeyboardKey::Code(KeyCode::AltLeft)
        );
        assert_eq!(
            "KeyW".parse::<KeyboardKey>().unwrap(),
            KeyboardKey::Code(KeyCode::KeyW)
        );
    }

    #[test]
    fn native_key_codes_round_trip() {
        for native in [
            NativeKeyCode::Unidentified,
            NativeKeyCode::Xkb(38),
            NativeKeyCode::MacOS(7),
            NativeKeyCode::Windows(43),
            NativeKeyCode::Android(12),
        ] {
            let key = KeyboardKey::Native(native);
            let token = key.to_string();
            assert_eq!(
                token.parse::<KeyboardKey>().unwrap(),
                key,
                "token {token:?}"
            );
        }
    }

    #[test]
    fn labels_are_non_empty_ascii() {
        for &code in COMMON {
            let label = KeyboardKey::Code(code).label();
            assert!(!label.is_empty());
            assert!(label.is_ascii(), "{label:?} for {code:?}");
        }
        for code in [KeyCode::BrowserBack, KeyCode::AudioVolumeUp, KeyCode::F30] {
            let label = KeyboardKey::Code(code).label();
            assert!(!label.is_empty());
            assert!(label.is_ascii(), "{label:?} for {code:?}");
        }
    }

    #[test]
    fn camel_case_label_splits_correctly() {
        assert_eq!(camel_case_label("BrowserBack"), "Browser Back");
        assert_eq!(camel_case_label("AudioVolumeUp"), "Audio Volume Up");
        assert_eq!(camel_case_label("F30"), "F30");
        assert_eq!(camel_case_label("IntlYen"), "Intl Yen");
    }

    #[test]
    fn unknown_key_errors() {
        assert!(matches!(
            "Notakey".parse::<KeyboardKey>(),
            Err(InputParseError::UnknownKey(_))
        ));
    }
}

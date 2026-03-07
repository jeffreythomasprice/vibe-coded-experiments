use bevy::prelude::*;

use robowar_shared::config::project::KeybindingsConfig;

pub fn parse_key(name: &str) -> Option<KeyCode> {
    match name.to_ascii_lowercase().as_str() {
        "space" => Some(KeyCode::Space),
        "escape" | "esc" => Some(KeyCode::Escape),
        "enter" | "return" => Some(KeyCode::Enter),
        "tab" => Some(KeyCode::Tab),
        "backspace" => Some(KeyCode::Backspace),
        "delete" => Some(KeyCode::Delete),
        "insert" => Some(KeyCode::Insert),
        "home" => Some(KeyCode::Home),
        "end" => Some(KeyCode::End),
        "pageup" => Some(KeyCode::PageUp),
        "pagedown" => Some(KeyCode::PageDown),
        "up" | "arrowup" => Some(KeyCode::ArrowUp),
        "down" | "arrowdown" => Some(KeyCode::ArrowDown),
        "left" | "arrowleft" => Some(KeyCode::ArrowLeft),
        "right" | "arrowright" => Some(KeyCode::ArrowRight),
        "equal" | "equals" | "+" => Some(KeyCode::Equal),
        "minus" | "-" => Some(KeyCode::Minus),
        "comma" | "," => Some(KeyCode::Comma),
        "period" | "." => Some(KeyCode::Period),
        "slash" | "/" => Some(KeyCode::Slash),
        "backslash" | "\\" => Some(KeyCode::Backslash),
        "semicolon" | ";" => Some(KeyCode::Semicolon),
        "bracketleft" | "[" => Some(KeyCode::BracketLeft),
        "bracketright" | "]" => Some(KeyCode::BracketRight),
        "backquote" | "`" => Some(KeyCode::Backquote),
        "quote" | "'" => Some(KeyCode::Quote),
        "a" => Some(KeyCode::KeyA),
        "b" => Some(KeyCode::KeyB),
        "c" => Some(KeyCode::KeyC),
        "d" => Some(KeyCode::KeyD),
        "e" => Some(KeyCode::KeyE),
        "f" => Some(KeyCode::KeyF),
        "g" => Some(KeyCode::KeyG),
        "h" => Some(KeyCode::KeyH),
        "i" => Some(KeyCode::KeyI),
        "j" => Some(KeyCode::KeyJ),
        "k" => Some(KeyCode::KeyK),
        "l" => Some(KeyCode::KeyL),
        "m" => Some(KeyCode::KeyM),
        "n" => Some(KeyCode::KeyN),
        "o" => Some(KeyCode::KeyO),
        "p" => Some(KeyCode::KeyP),
        "q" => Some(KeyCode::KeyQ),
        "r" => Some(KeyCode::KeyR),
        "s" => Some(KeyCode::KeyS),
        "t" => Some(KeyCode::KeyT),
        "u" => Some(KeyCode::KeyU),
        "v" => Some(KeyCode::KeyV),
        "w" => Some(KeyCode::KeyW),
        "x" => Some(KeyCode::KeyX),
        "y" => Some(KeyCode::KeyY),
        "z" => Some(KeyCode::KeyZ),
        "0" | "digit0" => Some(KeyCode::Digit0),
        "1" | "digit1" => Some(KeyCode::Digit1),
        "2" | "digit2" => Some(KeyCode::Digit2),
        "3" | "digit3" => Some(KeyCode::Digit3),
        "4" | "digit4" => Some(KeyCode::Digit4),
        "5" | "digit5" => Some(KeyCode::Digit5),
        "6" | "digit6" => Some(KeyCode::Digit6),
        "7" | "digit7" => Some(KeyCode::Digit7),
        "8" | "digit8" => Some(KeyCode::Digit8),
        "9" | "digit9" => Some(KeyCode::Digit9),
        "f1" => Some(KeyCode::F1),
        "f2" => Some(KeyCode::F2),
        "f3" => Some(KeyCode::F3),
        "f4" => Some(KeyCode::F4),
        "f5" => Some(KeyCode::F5),
        "f6" => Some(KeyCode::F6),
        "f7" => Some(KeyCode::F7),
        "f8" => Some(KeyCode::F8),
        "f9" => Some(KeyCode::F9),
        "f10" => Some(KeyCode::F10),
        "f11" => Some(KeyCode::F11),
        "f12" => Some(KeyCode::F12),
        _ => None,
    }
}

fn key_display_name(code: KeyCode) -> &'static str {
    match code {
        KeyCode::Space => "Space",
        KeyCode::Escape => "Esc",
        KeyCode::Enter => "Enter",
        KeyCode::Tab => "Tab",
        KeyCode::Backspace => "Backspace",
        KeyCode::Delete => "Delete",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        KeyCode::Equal => "=",
        KeyCode::Minus => "-",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::KeyA => "A",
        KeyCode::KeyB => "B",
        KeyCode::KeyC => "C",
        KeyCode::KeyD => "D",
        KeyCode::KeyE => "E",
        KeyCode::KeyF => "F",
        KeyCode::KeyG => "G",
        KeyCode::KeyH => "H",
        KeyCode::KeyI => "I",
        KeyCode::KeyJ => "J",
        KeyCode::KeyK => "K",
        KeyCode::KeyL => "L",
        KeyCode::KeyM => "M",
        KeyCode::KeyN => "N",
        KeyCode::KeyO => "O",
        KeyCode::KeyP => "P",
        KeyCode::KeyQ => "Q",
        KeyCode::KeyR => "R",
        KeyCode::KeyS => "S",
        KeyCode::KeyT => "T",
        KeyCode::KeyU => "U",
        KeyCode::KeyV => "V",
        KeyCode::KeyW => "W",
        KeyCode::KeyX => "X",
        KeyCode::KeyY => "Y",
        KeyCode::KeyZ => "Z",
        KeyCode::Digit0 => "0",
        KeyCode::Digit1 => "1",
        KeyCode::Digit2 => "2",
        KeyCode::Digit3 => "3",
        KeyCode::Digit4 => "4",
        KeyCode::Digit5 => "5",
        KeyCode::Digit6 => "6",
        KeyCode::Digit7 => "7",
        KeyCode::Digit8 => "8",
        KeyCode::Digit9 => "9",
        _ => "?",
    }
}

fn keys_display(keys: &[KeyCode]) -> String {
    keys.iter()
        .map(|k| key_display_name(*k))
        .collect::<Vec<_>>()
        .join(" / ")
}

fn parse_keys(names: &[String], action: &str, fallback: KeyCode) -> Vec<KeyCode> {
    let mut codes = Vec::new();
    for name in names {
        match parse_key(name) {
            Some(code) => codes.push(code),
            None => warn!("Unknown key '{}' for {}, skipping", name, action),
        }
    }
    if codes.is_empty() {
        codes.push(fallback);
    }
    codes
}

#[derive(Resource, Debug)]
pub struct Keybindings {
    pub pause: Vec<KeyCode>,
    pub speed_up: Vec<KeyCode>,
    pub speed_down: Vec<KeyCode>,
    pub restart: Vec<KeyCode>,
    pub menu: Vec<KeyCode>,
    pub zoom_in: Vec<KeyCode>,
    pub zoom_out: Vec<KeyCode>,
    pub pan_up: Vec<KeyCode>,
    pub pan_down: Vec<KeyCode>,
    pub pan_left: Vec<KeyCode>,
    pub pan_right: Vec<KeyCode>,
}

impl From<KeybindingsConfig> for Keybindings {
    fn from(config: KeybindingsConfig) -> Self {
        Self {
            pause: parse_keys(&config.pause, "pause", KeyCode::Space),
            speed_up: parse_keys(&config.speed_up, "speed_up", KeyCode::Equal),
            speed_down: parse_keys(&config.speed_down, "speed_down", KeyCode::Minus),
            restart: parse_keys(&config.restart, "restart", KeyCode::KeyR),
            menu: parse_keys(&config.menu, "menu", KeyCode::Escape),
            zoom_in: parse_keys(&config.zoom_in, "zoom_in", KeyCode::BracketRight),
            zoom_out: parse_keys(&config.zoom_out, "zoom_out", KeyCode::BracketLeft),
            pan_up: parse_keys(&config.pan_up, "pan_up", KeyCode::KeyW),
            pan_down: parse_keys(&config.pan_down, "pan_down", KeyCode::KeyS),
            pan_left: parse_keys(&config.pan_left, "pan_left", KeyCode::KeyA),
            pan_right: parse_keys(&config.pan_right, "pan_right", KeyCode::KeyD),
        }
    }
}

impl Keybindings {
    pub fn any_just_pressed(&self, keys: &ButtonInput<KeyCode>, bindings: &[KeyCode]) -> bool {
        bindings.iter().any(|k| keys.just_pressed(*k))
    }

    pub fn any_pressed(&self, keys: &ButtonInput<KeyCode>, bindings: &[KeyCode]) -> bool {
        bindings.iter().any(|k| keys.pressed(*k))
    }

    pub fn all_bindings(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Pause", keys_display(&self.pause)),
            ("Speed Up", keys_display(&self.speed_up)),
            ("Speed Down", keys_display(&self.speed_down)),
            ("Restart", keys_display(&self.restart)),
            ("Menu", keys_display(&self.menu)),
            ("Zoom In", keys_display(&self.zoom_in)),
            ("Zoom Out", keys_display(&self.zoom_out)),
            ("Pan Up", keys_display(&self.pan_up)),
            ("Pan Down", keys_display(&self.pan_down)),
            ("Pan Left", keys_display(&self.pan_left)),
            ("Pan Right", keys_display(&self.pan_right)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_known_keys() {
        assert_eq!(parse_key("Space"), Some(KeyCode::Space));
        assert_eq!(parse_key("space"), Some(KeyCode::Space));
        assert_eq!(parse_key("SPACE"), Some(KeyCode::Space));
        assert_eq!(parse_key("Escape"), Some(KeyCode::Escape));
        assert_eq!(parse_key("esc"), Some(KeyCode::Escape));
        assert_eq!(parse_key("Equal"), Some(KeyCode::Equal));
        assert_eq!(parse_key("Minus"), Some(KeyCode::Minus));
        assert_eq!(parse_key("R"), Some(KeyCode::KeyR));
        assert_eq!(parse_key("r"), Some(KeyCode::KeyR));
        assert_eq!(parse_key("F1"), Some(KeyCode::F1));
        assert_eq!(parse_key("Enter"), Some(KeyCode::Enter));
        assert_eq!(parse_key("Tab"), Some(KeyCode::Tab));
        assert_eq!(parse_key("BracketLeft"), Some(KeyCode::BracketLeft));
        assert_eq!(parse_key("["), Some(KeyCode::BracketLeft));
        assert_eq!(parse_key("BracketRight"), Some(KeyCode::BracketRight));
        assert_eq!(parse_key("]"), Some(KeyCode::BracketRight));
    }

    #[test]
    fn test_parse_key_unknown() {
        assert_eq!(parse_key("FooBar"), None);
        assert_eq!(parse_key(""), None);
        assert_eq!(parse_key("SuperKey"), None);
    }

    #[test]
    fn test_default_keybindings_config() {
        let config = KeybindingsConfig::default();
        assert_eq!(config.pause, vec!["Space"]);
        assert_eq!(config.speed_up, vec!["Equal"]);
        assert_eq!(config.speed_down, vec!["Minus"]);
        assert_eq!(config.restart, vec!["R"]);
        assert_eq!(config.menu, vec!["Escape"]);
        assert_eq!(config.pan_up, vec!["W", "ArrowUp"]);
    }

    #[test]
    fn test_all_bindings() {
        let kb = Keybindings {
            pause: vec![KeyCode::Space],
            speed_up: vec![KeyCode::Equal],
            speed_down: vec![KeyCode::Minus],
            restart: vec![KeyCode::KeyR],
            menu: vec![KeyCode::Escape],
            zoom_in: vec![KeyCode::BracketRight],
            zoom_out: vec![KeyCode::BracketLeft],
            pan_up: vec![KeyCode::KeyW, KeyCode::ArrowUp],
            pan_down: vec![KeyCode::KeyS, KeyCode::ArrowDown],
            pan_left: vec![KeyCode::KeyA, KeyCode::ArrowLeft],
            pan_right: vec![KeyCode::KeyD, KeyCode::ArrowRight],
        };
        let bindings = kb.all_bindings();
        assert_eq!(bindings[0], ("Pause", "Space".to_string()));
        assert_eq!(bindings[1], ("Speed Up", "=".to_string()));
        assert_eq!(bindings[2], ("Speed Down", "-".to_string()));
        assert_eq!(bindings[3], ("Restart", "R".to_string()));
        assert_eq!(bindings[4], ("Menu", "Esc".to_string()));
        assert_eq!(bindings[5], ("Zoom In", "]".to_string()));
        assert_eq!(bindings[6], ("Zoom Out", "[".to_string()));
        assert_eq!(bindings[7], ("Pan Up", "W / Up".to_string()));
    }

    #[test]
    fn test_keys_display_multiple() {
        assert_eq!(
            keys_display(&[KeyCode::KeyA, KeyCode::ArrowLeft]),
            "A / Left"
        );
        assert_eq!(keys_display(&[KeyCode::Space]), "Space");
    }
}

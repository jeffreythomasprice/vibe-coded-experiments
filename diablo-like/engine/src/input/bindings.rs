use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::action::Action;
use super::source::Input;

pub const DEFAULT_GAMEPAD_DEADZONE: f32 = 0.15;

const DEFAULT_BINDINGS: &[(Action, &[&str])] = &[
    (Action::MoveUp, &["key:W", "key:Up", "pad_axis:LeftStickY+"]),
    (
        Action::MoveDown,
        &["key:S", "key:Down", "pad_axis:LeftStickY-"],
    ),
    (
        Action::MoveLeft,
        &["key:A", "key:Left", "pad_axis:LeftStickX-"],
    ),
    (
        Action::MoveRight,
        &["key:D", "key:Right", "pad_axis:LeftStickX+"],
    ),
    (Action::CycleRenderScale, &["key:R", "pad_button:North"]),
    (Action::CycleRenderMode, &["key:M", "pad_button:West"]),
    (Action::Quit, &["key:Escape", "pad_button:Start"]),
    (Action::ZoomIn, &["mouse_axis:ScrollY+", "pad_button:RT"]),
    (Action::ZoomOut, &["mouse_axis:ScrollY-", "pad_button:LT"]),
];

/// Bindings for every action, backed by a `BTreeMap` (not a `HashMap`) so
/// serialized output is deterministic and ordered by declaration — needed
/// both for the write path and for the example-config round-trip test.
///
/// Deserializing only ever *overrides* entries on top of the full default
/// map (see `Deserialize` below): an action listed in `[input.bindings]`
/// replaces its defaults outright, an omitted action keeps its defaults,
/// and `[]` unbinds it. There is no additive "extend the defaults" mode.
#[derive(Debug, Clone, PartialEq)]
pub struct Bindings(BTreeMap<Action, Vec<Input>>);

impl Bindings {
    pub fn get(&self, action: Action) -> &[Input] {
        self.0.get(&action).map(Vec::as_slice).unwrap_or(&[])
    }
}

impl Default for Bindings {
    fn default() -> Self {
        let mut map = BTreeMap::new();
        for (action, tokens) in DEFAULT_BINDINGS {
            let inputs = tokens
                .iter()
                .map(|token| token.parse().expect("default binding token must parse"))
                .collect();
            map.insert(*action, inputs);
        }
        Bindings(map)
    }
}

impl Serialize for Bindings {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bindings {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let overrides = BTreeMap::<Action, Vec<Input>>::deserialize(deserializer)?;
        let mut merged = Bindings::default().0;
        for (action, inputs) in overrides {
            merged.insert(action, inputs);
        }
        Ok(Bindings(merged))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub gamepad_deadzone: f32,
    pub bindings: Bindings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            gamepad_deadzone: DEFAULT_GAMEPAD_DEADZONE,
            bindings: Bindings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_bind_every_action() {
        let bindings = Bindings::default();
        for &action in Action::ALL {
            assert!(
                !bindings.get(action).is_empty(),
                "{action:?} has no default binding"
            );
        }
    }

    #[test]
    fn partial_override_replaces_only_the_named_action() {
        let settings: Settings = toml::from_str(
            r#"
            [bindings]
            move_up = ["key:I"]
            "#,
        )
        .unwrap();
        assert_eq!(
            settings.bindings.get(Action::MoveUp),
            &[Input::KeyboardKey(crate::input::keys::KeyboardKey::Code(
                winit::keyboard::KeyCode::KeyI
            ))]
        );
        assert_eq!(
            settings.bindings.get(Action::MoveDown),
            Bindings::default().get(Action::MoveDown)
        );
    }

    #[test]
    fn empty_list_unbinds() {
        let settings: Settings = toml::from_str(
            r#"
            [bindings]
            quit = []
            "#,
        )
        .unwrap();
        assert!(settings.bindings.get(Action::Quit).is_empty());
    }

    #[test]
    fn unknown_action_name_is_rejected() {
        let result: Result<Settings, _> = toml::from_str(
            r#"
            [bindings]
            move_diagonally = ["key:W"]
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn unknown_settings_key_is_rejected() {
        let result: Result<Settings, _> = toml::from_str("gamepad_deadzon = 0.5");
        assert!(result.is_err());
    }

    #[test]
    fn missing_gamepad_deadzone_uses_default() {
        let settings: Settings = toml::from_str("").unwrap();
        assert_eq!(settings.gamepad_deadzone, DEFAULT_GAMEPAD_DEADZONE);
    }
}

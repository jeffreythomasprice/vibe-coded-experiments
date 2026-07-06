//! Backend-agnostic input bindings.
//!
//! This is the platform-agnostic half of the input system: the abstract set of
//! *triggers* (a keyboard key, a mouse button, a gamepad button), a
//! serializable container mapping named actions to arrays of triggers, and the
//! [`Action`] trait each emulator's action set implements to supply its stable
//! names and code defaults. It deliberately depends on nothing platform-specific
//! (no `winit`, `wgpu`, or `gilrs`) so a different frontend — say a web app
//! reading browser key events — can translate its own events into these same
//! triggers. The translation from a concrete windowing/gamepad library into
//! these types lives in the consuming crate (see the `emulator` crate).
//!
//! Binding resolution follows one rule, encoded in [`ActionBindings::triggers`]:
//! an action **absent** from the map uses its code default, while an action
//! **present** with an explicit (possibly empty) array uses exactly that array —
//! so an empty array is a deliberate "unbound".

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// A physical keyboard key. Variant names mirror `winit`'s `KeyCode` so the
/// backend translation is a 1:1 name match; `snake_case` serialization keeps the
/// TOML readable (`"arrow_up"`, `"key_x"`, `"shift_right"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[rustfmt::skip]
pub enum Key {
    // Letters.
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
    KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    // Top-row digits.
    Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
    // Function keys.
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    // Arrows.
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    // Editing / navigation.
    Escape, Enter, Space, Tab, Backspace, Delete, Insert,
    Home, End, PageUp, PageDown, CapsLock,
    // Modifiers.
    ShiftLeft, ShiftRight, ControlLeft, ControlRight,
    AltLeft, AltRight, SuperLeft, SuperRight,
    // Punctuation.
    Minus, Equal, BracketLeft, BracketRight, Backslash,
    Semicolon, Quote, Comma, Period, Slash, Backquote,
    // Numeric keypad.
    Numpad0, Numpad1, Numpad2, Numpad3, Numpad4,
    Numpad5, Numpad6, Numpad7, Numpad8, Numpad9,
    NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide,
    NumpadEnter, NumpadDecimal, NumLock,
}

/// A mouse button. Mirrors `winit`'s `MouseButton`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

/// A gamepad button. Variant names mirror `gilrs`'s `Button` (the standard
/// layout: `South`/`East`/`North`/`West` are the four face buttons).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GamepadButton {
    South,
    East,
    North,
    West,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    Select,
    Start,
    Mode,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

/// A single host input that can trigger an action. Serializes as an inline
/// table tagged by kind, e.g. `{ key = "escape" }` or `{ gamepad_button = "start" }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputTrigger {
    Key(Key),
    MouseButton(MouseButton),
    GamepadButton(GamepadButton),
}

/// An emulator's set of bindable actions. Implemented by a plain `enum` in each
/// system (plus the generic set here). It is intentionally *not* `Serialize` —
/// [`ActionBindings`] keys on the stable [`Action::name`], so the on-disk format
/// never depends on how the enum itself would serialize.
pub trait Action: Copy + 'static {
    /// Every action in this set, in a stable order.
    fn all() -> &'static [Self]
    where
        Self: Sized;
    /// The stable identifier used as this action's key in [`ActionBindings`].
    fn name(&self) -> &'static str;
    /// The triggers this action is bound to when the configuration says nothing.
    fn default_triggers(&self) -> Vec<InputTrigger>;
}

/// Persisted bindings for one action set: a map from [`Action::name`] to the
/// array of triggers that fire it. String-keyed so it round-trips cleanly
/// through any serde format (TOML table keys must be strings) and so `common`
/// need not know any concrete action enum.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionBindings(BTreeMap<String, Vec<InputTrigger>>);

impl ActionBindings {
    /// The triggers currently bound to `action`, applying the absent-vs-empty
    /// rule: an absent entry falls back to [`Action::default_triggers`]; a
    /// present entry (including an empty array, meaning "unbound") is used as-is.
    pub fn triggers<A: Action>(&self, action: &A) -> Vec<InputTrigger> {
        match self.0.get(action.name()) {
            Some(triggers) => triggers.clone(),
            None => action.default_triggers(),
        }
    }

    /// Overwrite the explicit triggers stored for `name` (an [`Action::name`]).
    /// The array is used verbatim by [`Self::triggers`] — including an empty one,
    /// which reads back as a deliberate "unbound" rather than the code default.
    /// This is the write path a rebinding UI uses.
    pub fn set(&mut self, name: &str, triggers: Vec<InputTrigger>) {
        self.0.insert(name.to_string(), triggers);
    }

    /// A binding set with every action written out explicitly at its default —
    /// the fully-populated form used by the config-dump subcommand.
    pub fn with_all_defaults<A: Action>() -> ActionBindings {
        ActionBindings(
            A::all()
                .iter()
                .map(|action| (action.name().to_string(), action.default_triggers()))
                .collect(),
        )
    }

    /// Invert the (resolved) bindings into a lookup from each trigger to the
    /// actions it fires, for dispatching host input events.
    pub fn reverse_index<A: Action>(&self) -> HashMap<InputTrigger, Vec<A>> {
        let mut index: HashMap<InputTrigger, Vec<A>> = HashMap::new();
        for action in A::all() {
            for trigger in self.triggers(action) {
                index.entry(trigger).or_default().push(*action);
            }
        }
        index
    }

    /// Keys present in the map that do not correspond to any action in `A` —
    /// i.e. typos or stale entries the backend can warn about.
    pub fn unknown_keys<A: Action>(&self) -> Vec<&str> {
        self.0
            .keys()
            .filter(|key| !A::all().iter().any(|action| action.name() == key.as_str()))
            .map(String::as_str)
            .collect()
    }
}

impl FromIterator<(String, Vec<InputTrigger>)> for ActionBindings {
    fn from_iter<I: IntoIterator<Item = (String, Vec<InputTrigger>)>>(iter: I) -> Self {
        ActionBindings(iter.into_iter().collect())
    }
}

/// Actions shared by every emulator core, independent of the system being run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GenericAction {
    /// Open the (not-yet-implemented) menu.
    Menu,
    /// Persist the loaded cartridge's battery-backed RAM (and RTC) to its save
    /// file immediately, rather than waiting for the on-exit flush.
    SaveBattery,
    /// Step the emulation speed up one ladder rung.
    IncreaseSpeed,
    /// Step the emulation speed down one ladder rung.
    DecreaseSpeed,
    /// Pause or resume the emulated machine.
    TogglePause,
}

impl Action for GenericAction {
    fn all() -> &'static [Self] {
        &[
            GenericAction::Menu,
            GenericAction::SaveBattery,
            GenericAction::IncreaseSpeed,
            GenericAction::DecreaseSpeed,
            GenericAction::TogglePause,
        ]
    }

    fn name(&self) -> &'static str {
        match self {
            GenericAction::Menu => "menu",
            GenericAction::SaveBattery => "save_battery",
            GenericAction::IncreaseSpeed => "increase_speed",
            GenericAction::DecreaseSpeed => "decrease_speed",
            GenericAction::TogglePause => "toggle_pause",
        }
    }

    fn default_triggers(&self) -> Vec<InputTrigger> {
        match self {
            GenericAction::Menu => vec![InputTrigger::Key(Key::Escape)],
            GenericAction::SaveBattery => vec![InputTrigger::Key(Key::F5)],
            GenericAction::IncreaseSpeed => vec![InputTrigger::Key(Key::Equal)],
            GenericAction::DecreaseSpeed => vec![InputTrigger::Key(Key::Minus)],
            GenericAction::TogglePause => vec![InputTrigger::Key(Key::Space)],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize)]
    struct Wrap {
        bindings: ActionBindings,
    }

    fn parse(toml: &str) -> ActionBindings {
        toml::from_str::<Wrap>(toml).unwrap().bindings
    }

    #[test]
    fn trigger_round_trips_through_toml() {
        for trigger in [
            InputTrigger::Key(Key::ArrowUp),
            InputTrigger::Key(Key::ShiftRight),
            InputTrigger::MouseButton(MouseButton::Left),
            InputTrigger::MouseButton(MouseButton::Other(7)),
            InputTrigger::GamepadButton(GamepadButton::DPadUp),
        ] {
            let bindings: ActionBindings =
                [("menu".to_string(), vec![trigger])].into_iter().collect();
            let text = toml::to_string(&Wrap { bindings }).unwrap();
            let back = parse(&text);
            assert_eq!(back.triggers(&GenericAction::Menu), vec![trigger]);
        }
    }

    #[test]
    fn absent_action_uses_default() {
        let bindings = ActionBindings::default();
        assert_eq!(
            bindings.triggers(&GenericAction::Menu),
            vec![InputTrigger::Key(Key::Escape)]
        );
    }

    #[test]
    fn empty_array_is_unbound() {
        let bindings = parse("bindings = { menu = [] }");
        assert!(bindings.triggers(&GenericAction::Menu).is_empty());
    }

    #[test]
    fn explicit_array_overrides_default() {
        let bindings = parse("bindings = { menu = [{ key = \"tab\" }] }");
        assert_eq!(
            bindings.triggers(&GenericAction::Menu),
            vec![InputTrigger::Key(Key::Tab)]
        );
    }

    #[test]
    fn reverse_index_maps_a_shared_trigger_to_multiple_actions() {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Pair {
            First,
            Second,
        }
        impl Action for Pair {
            fn all() -> &'static [Self] {
                &[Pair::First, Pair::Second]
            }
            fn name(&self) -> &'static str {
                match self {
                    Pair::First => "first",
                    Pair::Second => "second",
                }
            }
            fn default_triggers(&self) -> Vec<InputTrigger> {
                vec![InputTrigger::Key(Key::Space)]
            }
        }

        let index = ActionBindings::default().reverse_index::<Pair>();
        let mut actions = index.get(&InputTrigger::Key(Key::Space)).unwrap().clone();
        actions.sort_by_key(|a| a.name());
        assert_eq!(actions, vec![Pair::First, Pair::Second]);
    }

    #[test]
    fn with_all_defaults_covers_every_action() {
        let bindings = ActionBindings::with_all_defaults::<GenericAction>();
        assert!(bindings.unknown_keys::<GenericAction>().is_empty());
        assert_eq!(
            bindings.triggers(&GenericAction::Menu),
            vec![InputTrigger::Key(Key::Escape)]
        );
    }

    #[test]
    fn save_battery_action_has_stable_name_and_default() {
        assert_eq!(GenericAction::SaveBattery.name(), "save_battery");
        assert_eq!(
            ActionBindings::default().triggers(&GenericAction::SaveBattery),
            vec![InputTrigger::Key(Key::F5)]
        );
    }

    #[test]
    fn unknown_keys_flags_typos() {
        let bindings = parse("bindings = { mneu = [{ key = \"escape\" }] }");
        assert_eq!(bindings.unknown_keys::<GenericAction>(), vec!["mneu"]);
    }
}

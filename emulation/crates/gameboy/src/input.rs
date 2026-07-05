//! The Game Boy's bindable buttons, expressed as a [`common::input::Action`] set.
//!
//! This is the system-specific counterpart to the generic actions in `common`:
//! it names the eight DMG inputs and their default keyboard bindings, but knows
//! nothing about how host events are read (that translation lives in the
//! frontend). When the `FF00` joypad register is eventually implemented, it will
//! read the pressed state the frontend derives from these bindings.

use common::input::{Action, InputTrigger, Key};

/// The eight buttons of the original Game Boy: the four-way d-pad plus the
/// A/B/Start/Select action buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameboyButton {
    Right,
    Left,
    Up,
    Down,
    A,
    B,
    Select,
    Start,
}

impl Action for GameboyButton {
    fn all() -> &'static [Self] {
        use GameboyButton::*;
        &[Right, Left, Up, Down, A, B, Select, Start]
    }

    fn name(&self) -> &'static str {
        match self {
            GameboyButton::Right => "right",
            GameboyButton::Left => "left",
            GameboyButton::Up => "up",
            GameboyButton::Down => "down",
            GameboyButton::A => "a",
            GameboyButton::B => "b",
            GameboyButton::Select => "select",
            GameboyButton::Start => "start",
        }
    }

    fn default_triggers(&self) -> Vec<InputTrigger> {
        let key = |k| vec![InputTrigger::Key(k)];
        match self {
            GameboyButton::Right => key(Key::ArrowRight),
            GameboyButton::Left => key(Key::ArrowLeft),
            GameboyButton::Up => key(Key::ArrowUp),
            GameboyButton::Down => key(Key::ArrowDown),
            GameboyButton::A => key(Key::KeyX),
            GameboyButton::B => key(Key::KeyZ),
            GameboyButton::Start => key(Key::Enter),
            GameboyButton::Select => key(Key::ShiftRight),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_lists_the_eight_buttons() {
        assert_eq!(GameboyButton::all().len(), 8);
    }

    #[test]
    fn every_default_binding_is_non_empty() {
        for button in GameboyButton::all() {
            assert!(
                !button.default_triggers().is_empty(),
                "{button:?} should have a default binding",
            );
        }
    }

    #[test]
    fn names_are_unique() {
        let names: HashSet<_> = GameboyButton::all().iter().map(|b| b.name()).collect();
        assert_eq!(names.len(), GameboyButton::all().len());
    }

    #[test]
    fn agreed_default_keys() {
        assert_eq!(
            GameboyButton::A.default_triggers(),
            vec![InputTrigger::Key(Key::KeyX)]
        );
        assert_eq!(
            GameboyButton::B.default_triggers(),
            vec![InputTrigger::Key(Key::KeyZ)]
        );
        assert_eq!(
            GameboyButton::Start.default_triggers(),
            vec![InputTrigger::Key(Key::Enter)]
        );
        assert_eq!(
            GameboyButton::Select.default_triggers(),
            vec![InputTrigger::Key(Key::ShiftRight)]
        );
    }
}

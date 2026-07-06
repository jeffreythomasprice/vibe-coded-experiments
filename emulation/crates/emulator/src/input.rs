//! The desktop backend of the input system: translating concrete `winit` and
//! `gilrs` events into the platform-agnostic [`InputTrigger`]s defined in
//! `common`, and routing those triggers to the actions they are bound to.
//!
//! This is the counterpart the `common` module's doc comment refers to — the
//! platform-agnostic framework lives in `common`, and each frontend supplies its
//! own event translation here.

use std::collections::{HashMap, HashSet};

use common::input::{ActionBindings, GamepadButton, GenericAction, InputTrigger, Key};
use common::{InputBindings, MouseButton};
use gameboy::GameboyButton;

/// Translate a `winit` physical key code into our abstract [`Key`]. Variant names
/// match `winit`'s `KeyCode` one-to-one, so this is a pure name mapping; keys we
/// do not model return `None` and are ignored.
#[rustfmt::skip]
pub fn key_from_winit(code: winit::keyboard::KeyCode) -> Option<Key> {
    use winit::keyboard::KeyCode as W;
    macro_rules! map {
        ($($variant:ident),+ $(,)?) => {
            match code {
                $(W::$variant => Some(Key::$variant),)+
                _ => None,
            }
        };
    }
    map!(
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
        KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
        ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
        Escape, Enter, Space, Tab, Backspace, Delete, Insert,
        Home, End, PageUp, PageDown, CapsLock,
        ShiftLeft, ShiftRight, ControlLeft, ControlRight,
        AltLeft, AltRight, SuperLeft, SuperRight,
        Minus, Equal, BracketLeft, BracketRight, Backslash,
        Semicolon, Quote, Comma, Period, Slash, Backquote,
        Numpad0, Numpad1, Numpad2, Numpad3, Numpad4,
        Numpad5, Numpad6, Numpad7, Numpad8, Numpad9,
        NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide,
        NumpadEnter, NumpadDecimal, NumLock,
    )
}

/// Translate a `winit` mouse button into our abstract [`MouseButton`].
pub fn mouse_from_winit(button: winit::event::MouseButton) -> MouseButton {
    use winit::event::MouseButton as W;
    match button {
        W::Left => MouseButton::Left,
        W::Right => MouseButton::Right,
        W::Middle => MouseButton::Middle,
        W::Back => MouseButton::Back,
        W::Forward => MouseButton::Forward,
        W::Other(code) => MouseButton::Other(code),
    }
}

/// Translate a `gilrs` button into our abstract [`GamepadButton`]. `gilrs` names
/// the shoulder buttons `*Trigger` and the analog triggers `*Trigger2`; we map
/// those onto the more conventional bumper/trigger split. Buttons we do not model
/// (`C`, `Z`, `Unknown`) return `None`.
pub fn gamepad_from_gilrs(button: gilrs::Button) -> Option<GamepadButton> {
    use gilrs::Button as G;
    let mapped = match button {
        G::South => GamepadButton::South,
        G::East => GamepadButton::East,
        G::North => GamepadButton::North,
        G::West => GamepadButton::West,
        G::LeftTrigger => GamepadButton::LeftBumper,
        G::LeftTrigger2 => GamepadButton::LeftTrigger,
        G::RightTrigger => GamepadButton::RightBumper,
        G::RightTrigger2 => GamepadButton::RightTrigger,
        G::Select => GamepadButton::Select,
        G::Start => GamepadButton::Start,
        G::Mode => GamepadButton::Mode,
        G::LeftThumb => GamepadButton::LeftThumb,
        G::RightThumb => GamepadButton::RightThumb,
        G::DPadUp => GamepadButton::DPadUp,
        G::DPadDown => GamepadButton::DPadDown,
        G::DPadLeft => GamepadButton::DPadLeft,
        G::DPadRight => GamepadButton::DPadRight,
        G::C | G::Z | G::Unknown => return None,
    };
    Some(mapped)
}

/// Routes host input triggers to the actions bound to them.
///
/// It precomputes the reverse index (trigger → actions) for both the generic and
/// Game Boy action sets, then on each host event fires the affected actions.
/// Generic actions are edge-triggered on press and pushed onto a pending queue
/// that the owner drains with [`InputRouter::take_generic_actions`] and acts on
/// (the router itself has no access to the store or cartridge). Game Boy buttons
/// maintain a level-triggered pressed set — the seam the future `FF00` joypad
/// register will read.
pub struct InputRouter {
    generic: HashMap<InputTrigger, Vec<GenericAction>>,
    gameboy: HashMap<InputTrigger, Vec<GameboyButton>>,
    gb_pressed: HashSet<GameboyButton>,
    pending_generic: Vec<GenericAction>,
}

impl InputRouter {
    /// Build a router from the loaded generic and Game Boy bindings, warning
    /// about any configured action names that do not exist.
    pub fn new(generic: &InputBindings, gameboy: &ActionBindings) -> InputRouter {
        for key in generic.0.unknown_keys::<GenericAction>() {
            tracing::warn!(action = key, "unknown generic input binding; ignoring");
        }
        for key in gameboy.unknown_keys::<GameboyButton>() {
            tracing::warn!(action = key, "unknown gameboy input binding; ignoring");
        }
        InputRouter {
            generic: generic.0.reverse_index::<GenericAction>(),
            gameboy: gameboy.reverse_index::<GameboyButton>(),
            gb_pressed: HashSet::new(),
            pending_generic: Vec::new(),
        }
    }

    /// Dispatch a host input transition. `pressed` is `true` on press, `false` on
    /// release. Fired generic actions are queued for [`Self::take_generic_actions`]
    /// rather than handled here, since acting on them (e.g. writing a save) needs
    /// state the router does not own.
    pub fn handle(&mut self, trigger: InputTrigger, pressed: bool) {
        if pressed {
            self.handle_generic(trigger);
        }

        if let Some(buttons) = self.gameboy.get(&trigger) {
            for &button in buttons {
                let changed = if pressed {
                    self.gb_pressed.insert(button)
                } else {
                    self.gb_pressed.remove(&button)
                };
                if changed {
                    tracing::debug!(?button, pressed, "gameboy button");
                }
            }
        }
    }

    /// Queue only the *generic* actions bound to `trigger` (a press), ignoring the
    /// Game Boy button side. The menu overlay uses this so a global hotkey — most
    /// importantly the menu key itself — still fires while the menu is open (and
    /// the machine is paused), without a menu click leaking into the game as a
    /// held button.
    pub fn handle_generic(&mut self, trigger: InputTrigger) {
        if let Some(actions) = self.generic.get(&trigger) {
            self.pending_generic.extend_from_slice(actions);
        }
    }

    /// The Game Boy buttons currently held, fed to the joypad
    /// (`SystemBus::set_buttons`) by the run loop each frame.
    pub fn gameboy_pressed(&self) -> &HashSet<GameboyButton> {
        &self.gb_pressed
    }

    /// Drain the generic actions fired since the last call, for the owner to act
    /// on. Returns an empty vec when nothing fired.
    pub fn take_generic_actions(&mut self) -> Vec<GenericAction> {
        std::mem::take(&mut self.pending_generic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winit_arrow_maps_to_our_key() {
        assert_eq!(
            key_from_winit(winit::keyboard::KeyCode::ArrowUp),
            Some(Key::ArrowUp)
        );
        assert_eq!(
            key_from_winit(winit::keyboard::KeyCode::ShiftRight),
            Some(Key::ShiftRight)
        );
    }

    #[test]
    fn gilrs_shoulder_and_trigger_split() {
        assert_eq!(
            gamepad_from_gilrs(gilrs::Button::LeftTrigger),
            Some(GamepadButton::LeftBumper)
        );
        assert_eq!(
            gamepad_from_gilrs(gilrs::Button::LeftTrigger2),
            Some(GamepadButton::LeftTrigger)
        );
        assert_eq!(gamepad_from_gilrs(gilrs::Button::Unknown), None);
    }

    #[test]
    fn press_and_release_track_gameboy_state() {
        let generic = InputBindings::default();
        let gameboy = ActionBindings::with_all_defaults::<GameboyButton>();
        let mut router = InputRouter::new(&generic, &gameboy);

        // Default: ArrowRight -> Right.
        let right = InputTrigger::Key(Key::ArrowRight);
        router.handle(right, true);
        assert!(router.gameboy_pressed().contains(&GameboyButton::Right));
        router.handle(right, false);
        assert!(!router.gameboy_pressed().contains(&GameboyButton::Right));
    }

    #[test]
    fn save_battery_key_queues_a_generic_action() {
        let generic = InputBindings::default();
        let gameboy = ActionBindings::with_all_defaults::<GameboyButton>();
        let mut router = InputRouter::new(&generic, &gameboy);

        // Default: F5 -> SaveBattery, fired on press only.
        router.handle(InputTrigger::Key(Key::F5), true);
        assert_eq!(
            router.take_generic_actions(),
            vec![GenericAction::SaveBattery]
        );
        // Draining clears the queue; a release fires nothing.
        assert!(router.take_generic_actions().is_empty());
        router.handle(InputTrigger::Key(Key::F5), false);
        assert!(router.take_generic_actions().is_empty());
    }
}

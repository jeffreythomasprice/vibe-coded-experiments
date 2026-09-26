use std::collections::HashMap;

use glam::Vec2;

use super::action::Action;
use super::bindings::Settings;
use super::keys::KeyboardKey;
use super::pad::{GamepadAxis, GamepadButton};
use super::source::{AxisDir, Input, MouseAxis, MouseButton};

const PRESS_THRESHOLD: f32 = 0.5;

/// Every bound source resolves to `f32` in `0.0..=1.0`: digital sources are
/// 0 or 1, an axis half is its clamped magnitude on that side (so a
/// half-pushed stick reads `0.5`). This is what lets `movement()` read the
/// same way whether it's driven by keys or a stick, and lets an analog
/// gamepad trigger drive a nominally-digital action.
#[derive(Default)]
pub struct InputState {
    settings: Settings,
    keys: HashMap<KeyboardKey, bool>,
    mouse_buttons: HashMap<MouseButton, bool>,
    gamepad_buttons: HashMap<GamepadButton, f32>,
    gamepad_axes: HashMap<GamepadAxis, f32>,
    scroll_delta: Vec2,
    cursor_delta: Vec2,
    cursor_pos: Vec2,
    /// Sticky edges: set by `on_*` as soon as an event arrives, read by
    /// `just_pressed`/`just_released`, cleared by `end_frame`. Plain level
    /// diffing would miss a press-and-release that both land inside the
    /// same frame — see the fixed-timestep note in the design plan.
    just_pressed: HashMap<Input, bool>,
    just_released: HashMap<Input, bool>,
}

impl InputState {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            ..Default::default()
        }
    }

    pub fn on_key(&mut self, key: KeyboardKey, pressed: bool) {
        let was_down = self.keys.insert(key, pressed).unwrap_or(false);
        self.note_edge(Input::KeyboardKey(key), was_down, pressed);
    }

    pub fn on_mouse_button(&mut self, button: MouseButton, pressed: bool) {
        let was_down = self.mouse_buttons.insert(button, pressed).unwrap_or(false);
        self.note_edge(Input::MouseButton(button), was_down, pressed);
    }

    pub fn on_cursor_moved(&mut self, pos: Vec2) {
        self.cursor_delta += pos - self.cursor_pos;
        self.cursor_pos = pos;
    }

    /// `delta` is in "lines" (one mouse-wheel notch = 1.0); the caller
    /// normalizes a `MouseScrollDelta::PixelDelta` to that unit before
    /// calling this, so `InputState` doesn't need to know about winit's
    /// delta variants. Sign follows winit's own convention: positive is
    /// content moving right/down, i.e. scrolling the wheel *up* is a
    /// positive `y` — so `ZoomIn`'s default `mouse_axis:ScrollY+` fires on
    /// scroll-up.
    pub fn on_scroll(&mut self, delta: Vec2) {
        self.scroll_delta += delta;
    }

    pub fn on_focus_lost(&mut self) {
        for pressed in self.keys.values_mut() {
            *pressed = false;
        }
        for pressed in self.mouse_buttons.values_mut() {
            *pressed = false;
        }
    }

    pub fn set_gamepad_button_value(&mut self, button: GamepadButton, value: f32) {
        let was_pressed =
            self.gamepad_buttons.insert(button, value).unwrap_or(0.0) > PRESS_THRESHOLD;
        self.note_edge(
            Input::GamepadButton(button),
            was_pressed,
            value > PRESS_THRESHOLD,
        );
    }

    pub fn set_gamepad_axis_value(&mut self, axis: GamepadAxis, value: f32) {
        self.gamepad_axes.insert(axis, value);
    }

    fn note_edge(&mut self, input: Input, was_down: bool, is_down: bool) {
        if is_down && !was_down {
            self.just_pressed.insert(input, true);
        } else if was_down && !is_down {
            self.just_released.insert(input, true);
        }
    }

    /// Called once per frame after actions are read, per the fixed-timestep
    /// note: edges are consumed exactly once, deltas cover exactly the
    /// frame they arrived in, regardless of how many (if any) `step()`
    /// calls the accumulator runs.
    pub fn end_frame(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
        self.scroll_delta = Vec2::ZERO;
        self.cursor_delta = Vec2::ZERO;
    }

    fn source_value(&self, input: Input) -> f32 {
        match input {
            Input::KeyboardKey(key) => bool_value(self.keys.get(&key).copied().unwrap_or(false)),
            Input::MouseButton(button) => {
                bool_value(self.mouse_buttons.get(&button).copied().unwrap_or(false))
            }
            Input::MouseAxis(axis, dir) => axis_half(self.mouse_axis_delta(axis), dir),
            Input::GamepadButton(button) => self
                .gamepad_buttons
                .get(&button)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 1.0),
            Input::GamepadAxis(axis, dir) => {
                let raw = self.gamepad_axes.get(&axis).copied().unwrap_or(0.0);
                axis_half(apply_deadzone(raw, self.settings.gamepad_deadzone), dir)
            }
        }
    }

    fn mouse_axis_delta(&self, axis: MouseAxis) -> f32 {
        match axis {
            MouseAxis::X => self.cursor_delta.x,
            MouseAxis::Y => self.cursor_delta.y,
            MouseAxis::ScrollX => self.scroll_delta.x,
            MouseAxis::ScrollY => self.scroll_delta.y,
        }
    }

    pub fn value(&self, action: Action) -> f32 {
        self.settings
            .bindings
            .get(action)
            .iter()
            .map(|&input| self.source_value(input))
            .fold(0.0, f32::max)
    }

    pub fn pressed(&self, action: Action) -> bool {
        self.value(action) > PRESS_THRESHOLD
    }

    pub fn just_pressed(&self, action: Action) -> bool {
        self.settings
            .bindings
            .get(action)
            .iter()
            .any(|input| self.just_pressed.get(input).copied().unwrap_or(false))
    }

    pub fn just_released(&self, action: Action) -> bool {
        self.settings
            .bindings
            .get(action)
            .iter()
            .any(|input| self.just_released.get(input).copied().unwrap_or(false))
    }

    /// Normalized movement vector in world space (+x right, +y up). Uses
    /// `clamp_length_max` rather than the old `normalize_or_zero`: for
    /// digital input the two are identical (W+D is length sqrt(2), clamped
    /// to 1; W alone is already 1), but `clamp_length_max` preserves a
    /// half-pushed stick's magnitude instead of snapping it to 1.
    pub fn movement(&self) -> Vec2 {
        let x = self.value(Action::MoveRight) - self.value(Action::MoveLeft);
        let y = self.value(Action::MoveUp) - self.value(Action::MoveDown);
        Vec2::new(x, y).clamp_length_max(1.0)
    }

    pub fn cursor(&self) -> Vec2 {
        self.cursor_pos
    }

    /// Every control currently reading nonzero, as `(input, value)` pairs in
    /// `Input::display_order`. Must be read before `end_frame`, since mouse
    /// axes resolve against per-frame deltas that it zeroes.
    ///
    /// `Gamepad::pump` writes each gamepad event twice, once under its named
    /// `Known` control and once under its raw `code.<n>` twin, so that a
    /// raw-code binding has something to match. Reporting both would list one
    /// physical press twice, so an unbound raw code is dropped here: it says
    /// nothing its named twin doesn't.
    pub fn active(&self) -> Vec<(Input, f32)> {
        let mut active: Vec<(Input, f32)> = self
            .keys
            .keys()
            .map(|&key| Input::KeyboardKey(key))
            .chain(
                self.mouse_buttons
                    .keys()
                    .map(|&button| Input::MouseButton(button)),
            )
            .chain(
                self.gamepad_buttons
                    .keys()
                    .map(|&button| Input::GamepadButton(button)),
            )
            .chain(self.gamepad_axes.keys().flat_map(|&axis| {
                AxisDir::ALL
                    .iter()
                    .map(move |&dir| Input::GamepadAxis(axis, dir))
            }))
            .chain(MouseAxis::ALL.iter().flat_map(|&axis| {
                AxisDir::ALL
                    .iter()
                    .map(move |&dir| Input::MouseAxis(axis, dir))
            }))
            .filter(|&input| self.is_listable(input))
            .map(|input| (input, self.source_value(input)))
            .filter(|&(_, value)| value > 0.0)
            .collect();
        active.sort_by_key(|&(input, _)| input.display_order());
        active
    }

    /// A raw `code.<n>` gamepad source is only worth listing when something
    /// is actually bound to it — see `active`.
    fn is_listable(&self, input: Input) -> bool {
        match input {
            Input::GamepadButton(GamepadButton::Code(_))
            | Input::GamepadAxis(GamepadAxis::Code(_), _) => self.is_bound(input),
            _ => true,
        }
    }

    fn is_bound(&self, input: Input) -> bool {
        Action::ALL
            .iter()
            .any(|&action| self.settings.bindings.get(action).contains(&input))
    }
}

fn bool_value(pressed: bool) -> f32 {
    if pressed { 1.0 } else { 0.0 }
}

fn axis_half(value: f32, dir: AxisDir) -> f32 {
    match dir {
        AxisDir::Positive => value.max(0.0).min(1.0),
        AxisDir::Negative => (-value).max(0.0).min(1.0),
    }
}

/// Applies the deadzone ourselves (see the design plan) rather than opting
/// into gilrs's own `ev::filter::deadzone`, so the two never compound.
fn apply_deadzone(value: f32, deadzone: f32) -> f32 {
    let magnitude = value.abs();
    if magnitude <= deadzone {
        return 0.0;
    }
    let scaled = (magnitude - deadzone) / (1.0 - deadzone).max(f32::EPSILON);
    scaled.min(1.0) * value.signum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::keyboard::KeyCode;

    fn key(code: KeyCode) -> KeyboardKey {
        KeyboardKey::Code(code)
    }

    #[test]
    fn digital_movement_matches_old_normalize_or_zero() {
        let mut input = InputState::new(Settings::default());
        for (w, s, a, d) in [
            (false, false, false, false),
            (true, false, false, false),
            (false, true, false, false),
            (false, false, true, false),
            (false, false, false, true),
            (true, false, true, false),
            (true, false, false, true),
            (false, true, true, false),
        ] {
            input.on_key(key(KeyCode::KeyW), w);
            input.on_key(key(KeyCode::KeyS), s);
            input.on_key(key(KeyCode::KeyA), a);
            input.on_key(key(KeyCode::KeyD), d);
            let expected = {
                let mut v = Vec2::ZERO;
                if w {
                    v.y += 1.0;
                }
                if s {
                    v.y -= 1.0;
                }
                if d {
                    v.x += 1.0;
                }
                if a {
                    v.x -= 1.0;
                }
                v.normalize_or_zero()
            };
            let got = input.movement();
            assert!(
                (got - expected).length() < 1e-6,
                "{w} {s} {a} {d}: got {got:?} expected {expected:?}"
            );
        }
    }

    #[test]
    fn half_pushed_stick_preserves_magnitude() {
        // Deadzone is exercised separately below; zero it here so this test
        // isolates the magnitude-preservation behavior itself.
        let settings = Settings {
            gamepad_deadzone: 0.0,
            ..Settings::default()
        };
        let mut input = InputState::new(settings);
        input.set_gamepad_axis_value(GamepadAxis::Known(gilrs::Axis::LeftStickY), 0.5);
        assert!((input.movement().y - 0.5).abs() < 1e-6);
    }

    #[test]
    fn deadzone_zeroes_small_axis_values() {
        let mut settings = Settings::default();
        settings.gamepad_deadzone = 0.2;
        let mut input = InputState::new(settings);
        input.set_gamepad_axis_value(GamepadAxis::Known(gilrs::Axis::LeftStickY), 0.1);
        assert_eq!(input.movement().y, 0.0);
        input.set_gamepad_axis_value(GamepadAxis::Known(gilrs::Axis::LeftStickY), 0.6);
        assert!(input.movement().y > 0.0 && input.movement().y < 0.6);
    }

    #[test]
    fn max_over_sources() {
        let mut input = InputState::new(Settings::default());
        input.on_key(key(KeyCode::KeyW), true);
        input.set_gamepad_axis_value(GamepadAxis::Known(gilrs::Axis::LeftStickY), 0.3);
        assert_eq!(input.value(Action::MoveUp), 1.0);
    }

    #[test]
    fn sticky_edge_survives_press_and_release_within_one_frame() {
        let mut input = InputState::new(Settings::default());
        input.on_key(key(KeyCode::Escape), true);
        input.on_key(key(KeyCode::Escape), false);
        assert!(input.just_pressed(Action::Quit));
        assert!(input.just_released(Action::Quit));
        input.end_frame();
        assert!(!input.just_pressed(Action::Quit));
        assert!(!input.just_released(Action::Quit));
    }

    #[test]
    fn end_frame_zeroes_scroll_delta() {
        let mut input = InputState::new(Settings::default());
        input.on_scroll(Vec2::new(0.0, 3.0));
        assert!(input.value(Action::ZoomIn) > 0.0);
        input.end_frame();
        assert_eq!(input.value(Action::ZoomIn), 0.0);
    }

    #[test]
    fn scroll_up_drives_zoom_in_not_zoom_out() {
        let mut input = InputState::new(Settings::default());
        input.on_scroll(Vec2::new(0.0, 1.0));
        assert!(input.value(Action::ZoomIn) > 0.0);
        assert_eq!(input.value(Action::ZoomOut), 0.0);
    }

    #[test]
    fn focus_lost_clears_held_keys() {
        let mut input = InputState::new(Settings::default());
        input.on_key(key(KeyCode::KeyW), true);
        assert!(input.pressed(Action::MoveUp));
        input.on_focus_lost();
        assert!(!input.pressed(Action::MoveUp));
    }

    #[test]
    fn analog_trigger_drives_zoom_proportionally() {
        let mut input = InputState::new(Settings::default());
        input.set_gamepad_button_value(GamepadButton::Known(gilrs::Button::RightTrigger2), 0.7);
        assert!((input.value(Action::ZoomIn) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn active_lists_held_key() {
        let mut input = InputState::new(Settings::default());
        input.on_key(key(KeyCode::KeyW), true);
        assert_eq!(
            input.active(),
            vec![(Input::KeyboardKey(key(KeyCode::KeyW)), 1.0)]
        );
        input.on_key(key(KeyCode::KeyW), false);
        assert!(input.active().is_empty());
    }

    #[test]
    fn active_reports_analog_value() {
        let mut input = InputState::new(Settings::default());
        input.set_gamepad_button_value(GamepadButton::Known(gilrs::Button::RightTrigger2), 0.7);
        let active = input.active();
        assert_eq!(active.len(), 1);
        assert_eq!(
            active[0].0,
            Input::GamepadButton(GamepadButton::Known(gilrs::Button::RightTrigger2))
        );
        assert!((active[0].1 - 0.7).abs() < 1e-6);
    }

    #[test]
    fn active_applies_deadzone() {
        let settings = Settings {
            gamepad_deadzone: 0.2,
            ..Settings::default()
        };
        let mut input = InputState::new(settings);
        input.set_gamepad_axis_value(GamepadAxis::Known(gilrs::Axis::LeftStickY), 0.1);
        assert!(input.active().is_empty());

        input.set_gamepad_axis_value(GamepadAxis::Known(gilrs::Axis::LeftStickY), 0.6);
        let active = input.active();
        assert_eq!(active.len(), 1);
        assert_eq!(
            active[0].0,
            Input::GamepadAxis(
                GamepadAxis::Known(gilrs::Axis::LeftStickY),
                AxisDir::Positive
            )
        );
    }

    #[test]
    fn active_lists_scroll_until_end_frame() {
        let mut input = InputState::new(Settings::default());
        input.on_scroll(Vec2::new(0.0, 1.0));
        assert_eq!(
            input.active(),
            vec![(Input::MouseAxis(MouseAxis::ScrollY, AxisDir::Positive), 1.0)]
        );
        input.end_frame();
        assert!(input.active().is_empty());
    }

    #[test]
    fn active_drops_unbound_raw_gamepad_code() {
        let mut input = InputState::new(Settings::default());
        input.set_gamepad_button_value(GamepadButton::Known(gilrs::Button::North), 1.0);
        input.set_gamepad_button_value(GamepadButton::Code(304), 1.0);
        assert_eq!(
            input.active(),
            vec![(
                Input::GamepadButton(GamepadButton::Known(gilrs::Button::North)),
                1.0
            )]
        );

        let settings: Settings = toml::from_str(
            r#"
            [bindings]
            quit = ["pad_button:code.304"]
            "#,
        )
        .unwrap();
        let mut input = InputState::new(settings);
        input.set_gamepad_button_value(GamepadButton::Known(gilrs::Button::North), 1.0);
        input.set_gamepad_button_value(GamepadButton::Code(304), 1.0);
        assert_eq!(input.active().len(), 2);
    }

    #[test]
    fn active_is_sorted_by_device_then_label() {
        let mut input = InputState::new(Settings::default());
        input.on_key(key(KeyCode::KeyW), true);
        input.on_mouse_button(MouseButton::Left, true);
        input.set_gamepad_button_value(GamepadButton::Known(gilrs::Button::North), 1.0);

        let active = input.active();
        assert_eq!(active.len(), 3);
        assert!(matches!(active[0].0, Input::KeyboardKey(_)));
        assert!(matches!(active[1].0, Input::MouseButton(_)));
        assert!(matches!(active[2].0, Input::GamepadButton(_)));
    }
}

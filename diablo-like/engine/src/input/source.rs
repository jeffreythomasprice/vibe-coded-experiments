use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use super::keys::KeyboardKey;
use super::pad::{GamepadAxis, GamepadButton};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InputParseError {
    #[error(
        "input token {0:?} has no device prefix (expected key:, mouse_button:, mouse_axis:, pad_button:, or pad_axis:)"
    )]
    MissingDevice(String),
    #[error(
        "unknown input device {0:?} (expected key, mouse_button, mouse_axis, pad_button, or pad_axis)"
    )]
    UnknownDevice(String),
    #[error("unknown key {0:?}")]
    UnknownKey(String),
    #[error("unknown mouse button {0:?}")]
    UnknownMouseButton(String),
    #[error("unknown mouse axis {0:?}")]
    UnknownMouseAxis(String),
    #[error("unknown gamepad button {0:?}")]
    UnknownGamepadButton(String),
    #[error("unknown gamepad axis {0:?}")]
    UnknownGamepadAxis(String),
    #[error("axis binding {0:?} is missing a trailing + or - direction")]
    MissingAxisDirection(String),
    #[error("button binding {0:?} must not have a trailing + or - direction")]
    UnexpectedAxisDirection(String),
    #[error("invalid raw code {0:?}: expected code.<number>")]
    BadRawCode(String),
    #[error(
        "invalid native key code {0:?}: expected native.<platform>.<number> or native.unidentified"
    )]
    BadNativeKeyCode(String),
    #[error(
        "{0:?} is not directly bindable because it covers every unmapped control; \
         bind the specific control by its raw code instead, e.g. pad_button:code.<n> \
         (press it with debug logging on to find the code)"
    )]
    NotBindable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AxisDir {
    Positive,
    Negative,
}

impl AxisDir {
    pub const ALL: &'static [AxisDir] = &[AxisDir::Positive, AxisDir::Negative];
}

impl fmt::Display for AxisDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AxisDir::Positive => "+",
            AxisDir::Negative => "-",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Device {
    Keyboard,
    Mouse,
    Gamepad,
}

/// Splits a device-prefixed token on its first `:`, so the remainder (a key
/// name, a native code, or a raw gamepad code) is free to contain `.` itself.
fn split_device(s: &str) -> Result<(&str, &str), InputParseError> {
    s.split_once(':')
        .ok_or_else(|| InputParseError::MissingDevice(s.to_string()))
}

/// Strips a required trailing `+`/`-` axis direction, shared by mouse and
/// gamepad axis tokens.
fn split_axis_dir(s: &str) -> Result<(&str, AxisDir), InputParseError> {
    match s.as_bytes().last() {
        Some(b'+') => Ok((&s[..s.len() - 1], AxisDir::Positive)),
        Some(b'-') => Ok((&s[..s.len() - 1], AxisDir::Negative)),
        _ => Err(InputParseError::MissingAxisDirection(s.to_string())),
    }
}

/// Rejects a trailing `+`/`-` on a token that names a digital control, not
/// an axis half. Shared by every button-like `FromStr` impl.
fn reject_axis_dir(s: &str) -> Result<(), InputParseError> {
    match s.as_bytes().last() {
        Some(b'+') | Some(b'-') => Err(InputParseError::UnexpectedAxisDirection(s.to_string())),
        _ => Ok(()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u16),
}

impl MouseButton {
    pub const ALL: &'static [MouseButton] = &[
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
    ];

    pub fn label(self) -> String {
        match self {
            MouseButton::Left => "Mouse Left".to_string(),
            MouseButton::Right => "Mouse Right".to_string(),
            MouseButton::Middle => "Mouse Middle".to_string(),
            MouseButton::Back => "Mouse Back".to_string(),
            MouseButton::Forward => "Mouse Forward".to_string(),
            MouseButton::Other(n) => format!("Mouse Button {n}"),
        }
    }
}

impl fmt::Display for MouseButton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MouseButton::Left => f.write_str("Left"),
            MouseButton::Right => f.write_str("Right"),
            MouseButton::Middle => f.write_str("Middle"),
            MouseButton::Back => f.write_str("Back"),
            MouseButton::Forward => f.write_str("Forward"),
            MouseButton::Other(n) => write!(f, "other.{n}"),
        }
    }
}

impl FromStr for MouseButton {
    type Err = InputParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        reject_axis_dir(s)?;
        match s {
            "Left" => Ok(MouseButton::Left),
            "Right" => Ok(MouseButton::Right),
            "Middle" => Ok(MouseButton::Middle),
            "Back" => Ok(MouseButton::Back),
            "Forward" => Ok(MouseButton::Forward),
            other => other
                .strip_prefix("other.")
                .and_then(|n| n.parse().ok())
                .map(MouseButton::Other)
                .ok_or_else(|| InputParseError::UnknownMouseButton(s.to_string())),
        }
    }
}

impl From<winit::event::MouseButton> for MouseButton {
    fn from(button: winit::event::MouseButton) -> Self {
        match button {
            winit::event::MouseButton::Left => MouseButton::Left,
            winit::event::MouseButton::Right => MouseButton::Right,
            winit::event::MouseButton::Middle => MouseButton::Middle,
            winit::event::MouseButton::Back => MouseButton::Back,
            winit::event::MouseButton::Forward => MouseButton::Forward,
            winit::event::MouseButton::Other(n) => MouseButton::Other(n),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseAxis {
    X,
    Y,
    ScrollX,
    ScrollY,
}

impl MouseAxis {
    pub const ALL: &'static [MouseAxis] = &[
        MouseAxis::X,
        MouseAxis::Y,
        MouseAxis::ScrollX,
        MouseAxis::ScrollY,
    ];

    /// Direction-aware label: `X`/`Y` read as mouse motion, `ScrollX`/`ScrollY`
    /// as wheel notches. Vertical scroll is the case that matters by default
    /// (`ZoomIn`/`ZoomOut`), so `ScrollY` gets the human "Up"/"Down" framing
    /// rather than a bare `+`/`-`.
    pub fn direction_label(self, dir: AxisDir) -> String {
        let suffix = match (self, dir) {
            (MouseAxis::ScrollY, AxisDir::Positive) => "Up",
            (MouseAxis::ScrollY, AxisDir::Negative) => "Down",
            (MouseAxis::ScrollX, AxisDir::Positive) => "Right",
            (MouseAxis::ScrollX, AxisDir::Negative) => "Left",
            (_, AxisDir::Positive) => "+",
            (_, AxisDir::Negative) => "-",
        };
        let base = match self {
            MouseAxis::X => "Mouse X",
            MouseAxis::Y => "Mouse Y",
            MouseAxis::ScrollX => "Scroll",
            MouseAxis::ScrollY => "Scroll",
        };
        format!("{base} {suffix}")
    }
}

impl fmt::Display for MouseAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MouseAxis::X => "X",
            MouseAxis::Y => "Y",
            MouseAxis::ScrollX => "ScrollX",
            MouseAxis::ScrollY => "ScrollY",
        })
    }
}

impl FromStr for MouseAxis {
    type Err = InputParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "X" => Ok(MouseAxis::X),
            "Y" => Ok(MouseAxis::Y),
            "ScrollX" => Ok(MouseAxis::ScrollX),
            "ScrollY" => Ok(MouseAxis::ScrollY),
            other => Err(InputParseError::UnknownMouseAxis(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Input {
    KeyboardKey(KeyboardKey),
    MouseButton(MouseButton),
    MouseAxis(MouseAxis, AxisDir),
    GamepadButton(GamepadButton),
    GamepadAxis(GamepadAxis, AxisDir),
}

impl Input {
    pub fn device(self) -> Device {
        match self {
            Input::KeyboardKey(_) => Device::Keyboard,
            Input::MouseButton(_) | Input::MouseAxis(..) => Device::Mouse,
            Input::GamepadButton(_) | Input::GamepadAxis(..) => Device::Gamepad,
        }
    }

    pub fn label(self) -> String {
        match self {
            Input::KeyboardKey(key) => key.label(),
            Input::MouseButton(button) => button.label(),
            Input::MouseAxis(axis, dir) => axis.direction_label(dir),
            Input::GamepadButton(button) => button.label(),
            Input::GamepadAxis(axis, dir) => axis.direction_label(dir),
        }
    }

    /// Stable ordering for display lists: grouped by device, then
    /// alphabetical by label. `InputState`'s backing maps are `HashMap`s, so
    /// without this the HUD's active-input list would reshuffle every frame.
    pub fn display_order(self) -> (Device, String) {
        (self.device(), self.label())
    }
}

impl fmt::Display for Input {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Input::KeyboardKey(key) => write!(f, "key:{key}"),
            Input::MouseButton(button) => write!(f, "mouse_button:{button}"),
            Input::MouseAxis(axis, dir) => write!(f, "mouse_axis:{axis}{dir}"),
            Input::GamepadButton(button) => write!(f, "pad_button:{button}"),
            Input::GamepadAxis(axis, dir) => write!(f, "pad_axis:{axis}{dir}"),
        }
    }
}

impl FromStr for Input {
    type Err = InputParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (device, rest) = split_device(s)?;
        match device {
            "key" => Ok(Input::KeyboardKey(rest.parse()?)),
            "mouse_button" => Ok(Input::MouseButton(rest.parse()?)),
            "mouse_axis" => {
                let (name, dir) = split_axis_dir(rest)?;
                Ok(Input::MouseAxis(name.parse()?, dir))
            }
            "pad_button" => Ok(Input::GamepadButton(rest.parse()?)),
            "pad_axis" => {
                let (name, dir) = split_axis_dir(rest)?;
                Ok(Input::GamepadAxis(name.parse()?, dir))
            }
            other => Err(InputParseError::UnknownDevice(other.to_string())),
        }
    }
}

impl Serialize for Input {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Input {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_without_device_prefix_errors() {
        assert_eq!(
            "W".parse::<Input>(),
            Err(InputParseError::MissingDevice("W".to_string()))
        );
    }

    #[test]
    fn unknown_device_errors() {
        assert!(matches!(
            "keyboard:W".parse::<Input>(),
            Err(InputParseError::UnknownDevice(_))
        ));
    }

    #[test]
    fn axis_token_without_direction_errors() {
        assert!(matches!(
            "mouse_axis:ScrollY".parse::<Input>(),
            Err(InputParseError::MissingAxisDirection(_))
        ));
    }

    #[test]
    fn button_token_with_direction_errors() {
        assert!(matches!(
            "mouse_button:Left+".parse::<Input>(),
            Err(InputParseError::UnexpectedAxisDirection(_))
        ));
    }

    #[test]
    fn mouse_button_other_round_trips() {
        let input: Input = "mouse_button:other.5".parse().unwrap();
        assert_eq!(input.to_string(), "mouse_button:other.5");
    }

    #[test]
    fn mouse_axis_round_trips() {
        for &axis in MouseAxis::ALL {
            for &dir in AxisDir::ALL {
                let input = Input::MouseAxis(axis, dir);
                let token = input.to_string();
                assert_eq!(token.parse::<Input>().unwrap(), input, "token {token:?}");
            }
        }
    }

    #[test]
    fn axis_dir_all_covers_both_directions() {
        assert_eq!(AxisDir::ALL, &[AxisDir::Positive, AxisDir::Negative]);
    }

    #[test]
    fn every_device_prefix_is_distinct() {
        let tokens = [
            Input::KeyboardKey(KeyboardKey::Code(winit::keyboard::KeyCode::KeyW)).to_string(),
            Input::MouseButton(MouseButton::Left).to_string(),
            Input::MouseAxis(MouseAxis::X, AxisDir::Positive).to_string(),
        ];
        let prefixes: Vec<&str> = tokens
            .iter()
            .map(|t| t.split(':').next().unwrap())
            .collect();
        assert_eq!(prefixes, ["key", "mouse_button", "mouse_axis"]);
    }
}

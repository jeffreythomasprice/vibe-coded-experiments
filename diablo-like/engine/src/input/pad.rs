use std::fmt;
use std::str::FromStr;

use super::source::{AxisDir, InputParseError};

/// Unlike `KeyCode`, `gilrs::Button`/`Axis` are not `#[non_exhaustive]` (20
/// and 9 variants respectively), so an explicit table is cheap here and
/// buys friendly aliases plus an `ALL` for UI enumeration — the opposite
/// trade made in `keys.rs`, worth calling out since the two files otherwise
/// look alike.
macro_rules! button_table {
    ($($variant:ident => $token:literal $(| $alias:literal)*, $label:literal;)+) => {
        pub const ALL_BUTTONS: &[gilrs::Button] = &[$(gilrs::Button::$variant,)+];

        fn button_token(btn: gilrs::Button) -> Option<&'static str> {
            match btn {
                $(gilrs::Button::$variant => Some($token),)+
                gilrs::Button::Unknown => None,
            }
        }

        fn button_label(btn: gilrs::Button) -> Option<&'static str> {
            match btn {
                $(gilrs::Button::$variant => Some($label),)+
                gilrs::Button::Unknown => None,
            }
        }

        fn button_from_token(s: &str) -> Option<gilrs::Button> {
            match s {
                $($token => Some(gilrs::Button::$variant),)+
                $($($alias => Some(gilrs::Button::$variant),)*)+
                _ => None,
            }
        }
    };
}

// gilrs names the shoulder bumpers `LeftTrigger`/`RightTrigger` and the
// analog triggers `LeftTrigger2`/`RightTrigger2` (confirmed against its SDL2
// mapping table: "leftshoulder" -> LeftTrigger, "lefttrigger" ->
// LeftTrigger2) — the reverse of what the names suggest. The LB/RB/LT/RT
// aliases exist so config files don't have to know that.
button_table! {
    South => "South" | "A", "Gamepad A";
    East => "East" | "B", "Gamepad B";
    North => "North" | "Y", "Gamepad Y";
    West => "West" | "X", "Gamepad X";
    C => "C", "Gamepad C";
    Z => "Z", "Gamepad Z";
    LeftTrigger => "LeftTrigger" | "LB", "Left Bumper";
    LeftTrigger2 => "LeftTrigger2" | "LT", "Left Trigger";
    RightTrigger => "RightTrigger" | "RB", "Right Bumper";
    RightTrigger2 => "RightTrigger2" | "RT", "Right Trigger";
    Select => "Select", "Select";
    Start => "Start", "Start";
    Mode => "Mode", "Mode";
    LeftThumb => "LeftThumb", "Left Stick Click";
    RightThumb => "RightThumb", "Right Stick Click";
    DPadUp => "DPadUp", "D-Pad Up";
    DPadDown => "DPadDown", "D-Pad Down";
    DPadLeft => "DPadLeft", "D-Pad Left";
    DPadRight => "DPadRight", "D-Pad Right";
}

macro_rules! axis_table {
    ($($variant:ident => $token:literal, $label:literal;)+) => {
        pub const ALL_AXES: &[gilrs::Axis] = &[$(gilrs::Axis::$variant,)+];

        fn axis_token(axis: gilrs::Axis) -> Option<&'static str> {
            match axis {
                $(gilrs::Axis::$variant => Some($token),)+
                gilrs::Axis::Unknown => None,
            }
        }

        fn axis_label(axis: gilrs::Axis) -> Option<&'static str> {
            match axis {
                $(gilrs::Axis::$variant => Some($label),)+
                gilrs::Axis::Unknown => None,
            }
        }

        fn axis_from_token(s: &str) -> Option<gilrs::Axis> {
            match s {
                $($token => Some(gilrs::Axis::$variant),)+
                _ => None,
            }
        }
    };
}

// Labels intentionally omit the X/Y letter (`"Left Stick"`, not `"Left
// Stick X"`) so `direction_label` below can append `Up`/`Down`/`Left`/
// `Right` and read naturally, matching how a settings UI would show it.
axis_table! {
    LeftStickX => "LeftStickX", "Left Stick";
    LeftStickY => "LeftStickY", "Left Stick";
    LeftZ => "LeftZ", "Left Z";
    RightStickX => "RightStickX", "Right Stick";
    RightStickY => "RightStickY", "Right Stick";
    RightZ => "RightZ", "Right Z";
    DPadX => "DPadX", "D-Pad";
    DPadY => "DPadY", "D-Pad";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    Known(gilrs::Button),
    /// A control gilrs could not map to a named `Button`, addressed by its
    /// raw platform code (`gilrs::ev::Code::into_u32`). This is the escape
    /// hatch for pads with extra buttons gilrs doesn't recognize.
    Code(u32),
}

impl GamepadButton {
    pub fn all() -> impl Iterator<Item = GamepadButton> + Clone {
        ALL_BUTTONS.iter().copied().map(GamepadButton::Known)
    }

    pub fn label(self) -> String {
        match self {
            GamepadButton::Known(btn) => button_label(btn)
                .map(str::to_string)
                .unwrap_or_else(|| format!("Gamepad Button (code {btn:?})")),
            GamepadButton::Code(code) => format!("Gamepad Button {code}"),
        }
    }
}

impl fmt::Display for GamepadButton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GamepadButton::Known(btn) => match button_token(*btn) {
                Some(token) => f.write_str(token),
                None => write!(f, "{btn:?}"),
            },
            GamepadButton::Code(code) => write!(f, "code.{code}"),
        }
    }
}

impl FromStr for GamepadButton {
    type Err = InputParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if matches!(s.as_bytes().last(), Some(b'+') | Some(b'-')) {
            return Err(InputParseError::UnexpectedAxisDirection(s.to_string()));
        }
        if let Some(rest) = s.strip_prefix("code.") {
            return rest
                .parse()
                .map(GamepadButton::Code)
                .map_err(|_| InputParseError::BadRawCode(s.to_string()));
        }
        if s == "Unknown" {
            return Err(InputParseError::NotBindable(s.to_string()));
        }
        button_from_token(s)
            .map(GamepadButton::Known)
            .ok_or_else(|| InputParseError::UnknownGamepadButton(s.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadAxis {
    Known(gilrs::Axis),
    Code(u32),
}

impl GamepadAxis {
    pub fn all() -> impl Iterator<Item = GamepadAxis> + Clone {
        ALL_AXES.iter().copied().map(GamepadAxis::Known)
    }

    pub fn label(self) -> String {
        match self {
            GamepadAxis::Known(axis) => axis_label(axis)
                .map(str::to_string)
                .unwrap_or_else(|| format!("Gamepad Axis (code {axis:?})")),
            GamepadAxis::Code(code) => format!("Gamepad Axis {code}"),
        }
    }

    pub fn direction_label(self, dir: AxisDir) -> String {
        use gilrs::Axis;
        let suffix = match (self, dir) {
            (
                GamepadAxis::Known(Axis::LeftStickY | Axis::RightStickY | Axis::DPadY),
                AxisDir::Positive,
            ) => "Up",
            (
                GamepadAxis::Known(Axis::LeftStickY | Axis::RightStickY | Axis::DPadY),
                AxisDir::Negative,
            ) => "Down",
            (
                GamepadAxis::Known(Axis::LeftStickX | Axis::RightStickX | Axis::DPadX),
                AxisDir::Positive,
            ) => "Right",
            (
                GamepadAxis::Known(Axis::LeftStickX | Axis::RightStickX | Axis::DPadX),
                AxisDir::Negative,
            ) => "Left",
            (_, AxisDir::Positive) => "+",
            (_, AxisDir::Negative) => "-",
        };
        format!("{} {suffix}", self.label())
    }
}

impl fmt::Display for GamepadAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GamepadAxis::Known(axis) => match axis_token(*axis) {
                Some(token) => f.write_str(token),
                None => write!(f, "{axis:?}"),
            },
            GamepadAxis::Code(code) => write!(f, "code.{code}"),
        }
    }
}

impl FromStr for GamepadAxis {
    type Err = InputParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(rest) = s.strip_prefix("code.") {
            return rest
                .parse()
                .map(GamepadAxis::Code)
                .map_err(|_| InputParseError::BadRawCode(s.to_string()));
        }
        if s == "Unknown" {
            return Err(InputParseError::NotBindable(s.to_string()));
        }
        axis_from_token(s)
            .map(GamepadAxis::Known)
            .ok_or_else(|| InputParseError::UnknownGamepadAxis(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_buttons_excludes_unknown() {
        assert_eq!(ALL_BUTTONS.len(), 19);
        assert!(!ALL_BUTTONS.contains(&gilrs::Button::Unknown));
    }

    #[test]
    fn all_axes_excludes_unknown() {
        assert_eq!(ALL_AXES.len(), 8);
        assert!(!ALL_AXES.contains(&gilrs::Axis::Unknown));
    }

    #[test]
    fn buttons_round_trip() {
        for button in GamepadButton::all() {
            let token = button.to_string();
            assert_eq!(
                token.parse::<GamepadButton>().unwrap(),
                button,
                "token {token:?}"
            );
        }
    }

    #[test]
    fn axes_round_trip() {
        for axis in GamepadAxis::all() {
            let token = axis.to_string();
            assert_eq!(
                token.parse::<GamepadAxis>().unwrap(),
                axis,
                "token {token:?}"
            );
        }
    }

    #[test]
    fn raw_code_round_trips() {
        let button: GamepadButton = "code.704".parse().unwrap();
        assert_eq!(button.to_string(), "code.704");
        let axis: GamepadAxis = "code.17".parse().unwrap();
        assert_eq!(axis.to_string(), "code.17");
    }

    #[test]
    fn trigger_aliases_resolve() {
        assert_eq!(
            "RT".parse::<GamepadButton>().unwrap(),
            GamepadButton::Known(gilrs::Button::RightTrigger2)
        );
        assert_eq!(
            "LB".parse::<GamepadButton>().unwrap(),
            GamepadButton::Known(gilrs::Button::LeftTrigger)
        );
    }

    #[test]
    fn unknown_is_not_directly_bindable() {
        assert!(matches!(
            "Unknown".parse::<GamepadButton>(),
            Err(InputParseError::NotBindable(_))
        ));
        assert!(matches!(
            "Unknown".parse::<GamepadAxis>(),
            Err(InputParseError::NotBindable(_))
        ));
    }

    #[test]
    fn direction_labels_read_naturally() {
        let up = GamepadAxis::Known(gilrs::Axis::LeftStickY).direction_label(AxisDir::Positive);
        assert_eq!(up, "Left Stick Up");
        let right = GamepadAxis::Known(gilrs::Axis::RightStickX).direction_label(AxisDir::Positive);
        assert_eq!(right, "Right Stick Right");
    }
}

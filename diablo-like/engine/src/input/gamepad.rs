use super::pad::{GamepadAxis, GamepadButton};
use super::state::InputState;

/// Wraps `gilrs`, converting its events into `InputState` values. Polling
/// gilrs's own `Gamepad::is_pressed`/`value` isn't an option for the raw
/// `code.<n>` escape hatch: `gilrs::ev::Code` can't be constructed by hand
/// (`Code(pub(crate) EvCode)`), so a raw-code binding can only ever be
/// matched against the `Code` carried on an event — hence draining
/// `next_event` every frame rather than polling gamepad state directly.
pub struct Gamepad {
    gilrs: Option<gilrs::Gilrs>,
}

impl Gamepad {
    pub fn new() -> Self {
        match gilrs::Gilrs::new() {
            Ok(gilrs) => Self { gilrs: Some(gilrs) },
            Err(gilrs::Error::NotImplemented(dummy)) => {
                tracing::warn!("gamepad support is not implemented on this platform");
                Self { gilrs: Some(dummy) }
            }
            Err(err) => {
                tracing::warn!("failed to initialize gamepad support: {err}");
                Self { gilrs: None }
            }
        }
    }

    pub fn pump(&mut self, input: &mut InputState) {
        let Some(gilrs) = &mut self.gilrs else { return };
        while let Some(event) = gilrs.next_event() {
            let gilrs::Event { id, event, .. } = event;
            match event {
                gilrs::EventType::ButtonPressed(button, code) => {
                    tracing::debug!(
                        ?id,
                        ?button,
                        code = code.into_u32(),
                        "gamepad button pressed"
                    );
                    input.set_gamepad_button_value(GamepadButton::Known(button), 1.0);
                    input.set_gamepad_button_value(GamepadButton::Code(code.into_u32()), 1.0);
                }
                gilrs::EventType::ButtonReleased(button, code) => {
                    input.set_gamepad_button_value(GamepadButton::Known(button), 0.0);
                    input.set_gamepad_button_value(GamepadButton::Code(code.into_u32()), 0.0);
                }
                gilrs::EventType::ButtonChanged(button, value, code) => {
                    input.set_gamepad_button_value(GamepadButton::Known(button), value);
                    input.set_gamepad_button_value(GamepadButton::Code(code.into_u32()), value);
                }
                gilrs::EventType::AxisChanged(axis, value, code) => {
                    input.set_gamepad_axis_value(GamepadAxis::Known(axis), value);
                    input.set_gamepad_axis_value(GamepadAxis::Code(code.into_u32()), value);
                }
                gilrs::EventType::Connected => tracing::info!(?id, "gamepad connected"),
                gilrs::EventType::Disconnected => tracing::info!(?id, "gamepad disconnected"),
                gilrs::EventType::ButtonRepeated(..)
                | gilrs::EventType::Dropped
                | gilrs::EventType::ForceFeedbackEffectCompleted => {}
                _ => {}
            }
        }
    }
}

impl Default for Gamepad {
    fn default() -> Self {
        Self::new()
    }
}

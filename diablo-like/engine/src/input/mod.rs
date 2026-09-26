mod action;
mod bindings;
mod gamepad;
mod keys;
mod pad;
mod source;
mod state;

pub use action::Action;
pub use bindings::Settings;
pub use gamepad::Gamepad;
pub use keys::{COMMON as COMMON_KEYS, KeyboardKey};
pub use pad::{GamepadAxis, GamepadButton};
pub use source::{AxisDir, Device, Input, InputParseError, MouseAxis, MouseButton};
pub use state::InputState;

#[cfg(test)]
mod tests {
    use super::*;

    /// The example `config.toml`'s `[input]` section is the entire reason
    /// this file has an `[input]` section at all: it has to actually match
    /// `Settings::default()`, or the "commented-out defaults" convention
    /// (`config.rs`'s existing `# file = "..."` pattern) silently lies.
    /// Uncomments every `# key = value` line from `[input]` onward and
    /// parses it, the same idiom `text/mod.rs` uses to embed the font.
    #[test]
    fn example_config_input_section_matches_defaults() {
        // The sliced text still starts with the literal `[input]` header,
        // so it parses as `{ input: Settings }`, not `Settings` itself —
        // this wrapper mirrors that rather than re-nesting incorrectly.
        #[derive(serde::Deserialize)]
        struct Wrapper {
            input: Settings,
        }

        let raw = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../config.toml"));
        let start = raw
            .find("[input]")
            .expect("config.toml must have an [input] section");
        let uncommented: String = raw[start..]
            .lines()
            .map(|line| {
                line.trim_start()
                    .strip_prefix("# ")
                    .unwrap_or(line.trim_start())
            })
            .collect::<Vec<_>>()
            .join("\n");

        let wrapper: Wrapper = toml::from_str(&uncommented)
            .unwrap_or_else(|err| panic!("example [input] section failed to parse: {err}"));
        assert_eq!(wrapper.input, Settings::default());
    }
}

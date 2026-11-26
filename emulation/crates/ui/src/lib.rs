//! Backend-agnostic emulator UI, built on `egui` core.
//!
//! This crate owns the *actual* menu — the screens, the cursor navigation, the
//! input-bindings table, the rebind-capture logic, the old-school theme, and the
//! embedded pixel font — expressed purely as `egui` draw calls against a caller-supplied
//! [`egui::Context`]. It deliberately depends on nothing windowing- or GPU-
//! specific (no `winit`, `wgpu`) and on no particular emulator core (no
//! `gameboy`): every system-specific value crosses the boundary as a plain
//! `common`/`String` type in [`MenuData`], and every effect the UI cannot perform
//! itself leaves as a [`MenuCommand`]. A second frontend (e.g. a web app) reuses
//! this crate verbatim, supplying its own input and render backends.

mod menu;
mod nav;
mod screens;
mod theme;
mod widgets;

use common::{InputTrigger, Key, MouseButton};

pub use menu::MenuState;
pub use nav::MenuInput;
pub use theme::{Palette, Theme, DMG_GREEN};

/// The pixel font embedded and installed so all egui text renders in the Game
/// Boy aesthetic. Backend-agnostic — it is just glyph data handed to egui core.
const FONT: &[u8] = include_bytes!("../../../assets/Early GameBoy.ttf");

/// Install the embedded font as the top-priority proportional and monospace
/// family on `ctx`. Call once, after creating the context.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "early_gameboy".to_owned(),
        egui::FontData::from_static(FONT),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "early_gameboy".to_owned());
    }
    ctx.set_fonts(fonts);
}

/// Which binding set an [`BindingRow`]/[`MenuCommand::SetBinding`] refers to. The
/// UI is agnostic to what the sets *are*; the host maps these back to its
/// concrete action tables (generic actions vs. the system's buttons).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingSet {
    /// The cross-emulator generic actions (menu, save, ...).
    Generic,
    /// The current system's buttons (e.g. the Game Boy's d-pad/A/B/...).
    System,
}

/// One selectable ROM, as the host wants it shown. `id` is an opaque token the
/// host uses to load the ROM ([`MenuCommand::LoadRom`] hands it straight back).
#[derive(Clone, Debug)]
pub struct RomChoice {
    pub id: String,
    pub label: String,
}

/// One row of the input-bindings table: a display name plus the triggers
/// currently bound to it (only the first two are shown/editable).
#[derive(Clone, Debug)]
pub struct BindingRow {
    pub set: BindingSet,
    /// Stable action key, echoed back in [`MenuCommand::SetBinding`].
    pub action: String,
    /// Human-facing label for the left column.
    pub display: String,
    pub triggers: Vec<InputTrigger>,
}

/// Everything the host feeds the UI each frame. All borrowed, so building it is
/// cheap and the host keeps ownership of its catalogs and bindings.
pub struct MenuData<'a> {
    pub rom_loaded: bool,
    pub roms: &'a [RomChoice],
    pub bindings: &'a [BindingRow],
    /// Current emulation speed, as a short label for the HUD (e.g. `"1x"`,
    /// `"As Fast As Possible"`). Always shown, top-left, regardless of menu state.
    pub speed_label: &'a str,
    /// The measured actual emulation speed (× real time), shown under the target
    /// speed so the user can see when the host can't keep up. `None` hides the
    /// line (e.g. while paused or before the first measurement).
    pub actual_speed: Option<f32>,
    /// Whether the machine is paused (shows a `Paused` indicator, top-center).
    pub paused: bool,
}

/// An effect the UI produced this frame that only the host can carry out.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuCommand {
    /// Load (hot-swap) the ROM with this opaque id.
    LoadRom(String),
    /// Quit the application.
    Exit,
    /// Close the menu (resume the machine).
    CloseMenu,
    /// Assign `trigger` to `action`'s slot `slot` (`None` clears/unbinds it).
    SetBinding {
        set: BindingSet,
        action: String,
        slot: usize,
        trigger: Option<InputTrigger>,
    },
    /// Reset every binding (both sets) to its code default.
    ResetBindings,
}

/// Render the always-on HUD plus the current menu screen (if open), apply any
/// queued [`MenuInput`]s, and return the commands the menu produced.
pub fn draw(ctx: &egui::Context, state: &mut MenuState, data: &MenuData) -> Vec<MenuCommand> {
    let mut commands = Vec::new();
    let theme = Theme::dmg(ctx.screen_rect().size(), ctx.pixels_per_point());
    theme.apply(ctx);

    // Controls first, so this frame already shows their effect.
    state.process_inputs(data, &mut commands);
    let hud_bottom = screens::draw_hud(ctx, &theme, data);
    screens::draw_menu(ctx, &theme, state, data, hud_bottom, &mut commands);

    commands
}

/// A short human-facing label for a bound trigger (e.g. `Up`, `X`, `Mouse Left`,
/// `Pad Start`).
pub fn trigger_label(trigger: &InputTrigger) -> String {
    match trigger {
        InputTrigger::Key(key) => key_label(*key),
        InputTrigger::MouseButton(button) => match button {
            MouseButton::Other(code) => format!("Mouse {code}"),
            other => format!("Mouse {other:?}"),
        },
        InputTrigger::GamepadButton(button) => format!("Pad {button:?}"),
    }
}

/// Prettify a [`Key`] using its `Debug` name, stripping the noisy prefixes the
/// `winit`-mirroring variant names carry (`KeyX`→`X`, `ArrowUp`→`Up`,
/// `Digit5`→`5`); anything else (`ShiftRight`, `F5`, `Enter`) reads fine as-is.
fn key_label(key: Key) -> String {
    let raw = format!("{key:?}");
    for prefix in ["Key", "Arrow", "Digit"] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            return rest.to_owned();
        }
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::GamepadButton;

    #[test]
    fn key_labels_strip_noisy_prefixes() {
        assert_eq!(trigger_label(&InputTrigger::Key(Key::KeyX)), "X");
        assert_eq!(trigger_label(&InputTrigger::Key(Key::ArrowUp)), "Up");
        assert_eq!(trigger_label(&InputTrigger::Key(Key::Digit5)), "5");
    }

    #[test]
    fn other_keys_read_as_is() {
        assert_eq!(
            trigger_label(&InputTrigger::Key(Key::ShiftRight)),
            "ShiftRight"
        );
        assert_eq!(trigger_label(&InputTrigger::Key(Key::F5)), "F5");
        assert_eq!(trigger_label(&InputTrigger::Key(Key::Escape)), "Escape");
    }

    #[test]
    fn mouse_and_gamepad_labels() {
        assert_eq!(
            trigger_label(&InputTrigger::MouseButton(MouseButton::Left)),
            "Mouse Left"
        );
        assert_eq!(
            trigger_label(&InputTrigger::MouseButton(MouseButton::Other(7))),
            "Mouse 7"
        );
        assert_eq!(
            trigger_label(&InputTrigger::GamepadButton(GamepadButton::Start)),
            "Pad Start"
        );
    }

    #[test]
    fn every_screen_draws_headlessly() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        let bindings: Vec<BindingRow> = (0..30)
            .map(|i| BindingRow {
                set: BindingSet::System,
                action: format!("action{i}"),
                display: format!("Action {i}"),
                triggers: vec![InputTrigger::Key(Key::KeyX)],
            })
            .collect();
        let data = MenuData {
            rom_loaded: true,
            roms: &[],
            bindings: &bindings,
            speed_label: "1x",
            actual_speed: Some(1.0),
            paused: true,
        };
        let mut state = MenuState::default();
        state.open();
        // Rom list (empty) and back, then options, bindings, and past the fold.
        let mut inputs = vec![MenuInput::Confirm, MenuInput::Back, MenuInput::Down];
        inputs.extend([MenuInput::Confirm, MenuInput::Confirm]);
        inputs.extend([MenuInput::Down; 25]);
        for input in inputs {
            state.navigate(input);
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 576.0),
                )),
                ..Default::default()
            };
            let output = ctx.run(input, |ctx| {
                assert!(draw(ctx, &mut state, &data).is_empty());
            });
            assert!(!output.shapes.is_empty());
        }
        assert_eq!(state.cursor.row, 25);
        assert!(state.scroll > 0, "the list scrolled to follow the cursor");
    }
}

//! Backend-agnostic emulator UI, built on `egui` core.
//!
//! This crate owns the *actual* menu — the screens, the navigation state
//! machine, the input-bindings table, the rebind-capture logic, and the embedded
//! pixel font — expressed purely as `egui` draw calls against a caller-supplied
//! [`egui::Context`]. It deliberately depends on nothing windowing- or GPU-
//! specific (no `winit`, `wgpu`) and on no particular emulator core (no
//! `gameboy`): every system-specific value crosses the boundary as a plain
//! `common`/`String` type in [`MenuData`], and every effect the UI cannot perform
//! itself leaves as a [`MenuCommand`]. A second frontend (e.g. a web app) reuses
//! this crate verbatim, supplying its own input and render backends.

use common::{InputTrigger, Key, MouseButton};

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

/// Which menu screen is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Root,
    Options,
    InputBindings,
    SelectRom,
}

/// A pending rebind: the cell awaiting the next host input event.
#[derive(Clone, Debug)]
struct Capture {
    set: BindingSet,
    action: String,
    slot: usize,
}

/// The menu's navigation + capture state. Pure data — no egui/GPU handles — so
/// the host owns it and drives [`draw`] with it each frame.
#[derive(Default)]
pub struct MenuState {
    screen: Option<Screen>,
    capturing: Option<Capture>,
}

impl MenuState {
    pub fn is_open(&self) -> bool {
        self.screen.is_some()
    }

    /// Open the menu at its root screen.
    pub fn open(&mut self) {
        self.screen = Some(Screen::Root);
        self.capturing = None;
    }

    /// Close the menu and cancel any pending capture.
    pub fn close(&mut self) {
        self.screen = None;
        self.capturing = None;
    }

    pub fn toggle(&mut self) {
        if self.is_open() {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn is_capturing(&self) -> bool {
        self.capturing.is_some()
    }

    fn begin_capture(&mut self, set: BindingSet, action: String, slot: usize) {
        self.capturing = Some(Capture { set, action, slot });
    }

    /// Abandon a pending rebind without changing anything.
    pub fn cancel_capture(&mut self) {
        self.capturing = None;
    }

    /// Resolve a pending capture with the host input `trigger`, yielding the
    /// binding edit to apply. Returns `None` when nothing was capturing.
    pub fn capture(&mut self, trigger: InputTrigger) -> Option<MenuCommand> {
        let Capture { set, action, slot } = self.capturing.take()?;
        Some(MenuCommand::SetBinding {
            set,
            action,
            slot,
            trigger: Some(trigger),
        })
    }
}

/// Font size for the always-on HUD overlays (speed and pause).
const OVERLAY_TEXT_SIZE: f32 = 12.0;

/// Render the always-on HUD overlays plus the current menu screen (if open), and
/// return any commands the menu produced. The overlays draw every frame regardless
/// of menu state; the menu contributes nothing when closed.
pub fn draw(ctx: &egui::Context, state: &mut MenuState, data: &MenuData) -> Vec<MenuCommand> {
    let mut commands = Vec::new();

    draw_overlays(ctx, data);

    let Some(screen) = state.screen else {
        return commands;
    };

    egui::Window::new(title(screen))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| match screen {
            Screen::Root => root_screen(ui, state, data, &mut commands),
            Screen::SelectRom => select_rom_screen(ui, state, data, &mut commands),
            Screen::Options => options_screen(ui, state),
            Screen::InputBindings => bindings_screen(ui, state, data, &mut commands),
        });

    commands
}

/// Draw the always-visible HUD: the speed label pinned top-left and, when paused,
/// a `Paused` indicator pinned top-center. Both are non-interactive so they never
/// steal pointer input from the game or menu.
fn draw_overlays(ctx: &egui::Context, data: &MenuData) {
    egui::Area::new(egui::Id::new("hud-speed"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(4.0, 4.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(format!("Speed: {}", data.speed_label))
                    .size(OVERLAY_TEXT_SIZE),
            );
            if let Some(actual) = data.actual_speed {
                ui.label(
                    egui::RichText::new(format!("Actual Speed: {actual:.2}x"))
                        .size(OVERLAY_TEXT_SIZE),
                );
            }
        });

    if data.paused {
        egui::Area::new(egui::Id::new("hud-paused"))
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 4.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Paused").size(OVERLAY_TEXT_SIZE));
            });
    }
}

fn title(screen: Screen) -> &'static str {
    match screen {
        Screen::Root => "Menu",
        Screen::SelectRom => "Select ROM",
        Screen::Options => "Options",
        Screen::InputBindings => "Input Bindings",
    }
}

fn root_screen(
    ui: &mut egui::Ui,
    state: &mut MenuState,
    data: &MenuData,
    commands: &mut Vec<MenuCommand>,
) {
    let select_label = if data.rom_loaded {
        "Select A Different ROM"
    } else {
        "Select ROM"
    };
    if ui.button(select_label).clicked() {
        state.screen = Some(Screen::SelectRom);
    }
    if ui.button("Options").clicked() {
        state.screen = Some(Screen::Options);
    }
    if ui.button("Exit").clicked() {
        commands.push(MenuCommand::Exit);
    }
}

fn select_rom_screen(
    ui: &mut egui::Ui,
    state: &mut MenuState,
    data: &MenuData,
    commands: &mut Vec<MenuCommand>,
) {
    if data.roms.is_empty() {
        ui.label("No ROMs found in the roms dir.");
    } else {
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for rom in data.roms {
                    if ui.button(&rom.label).clicked() {
                        commands.push(MenuCommand::LoadRom(rom.id.clone()));
                    }
                }
            });
    }
    ui.separator();
    if ui.button("Back").clicked() {
        state.screen = Some(Screen::Root);
    }
}

fn options_screen(ui: &mut egui::Ui, state: &mut MenuState) {
    if ui.button("Input Bindings").clicked() {
        state.screen = Some(Screen::InputBindings);
    }
    if ui.button("Back").clicked() {
        state.screen = Some(Screen::Root);
    }
}

fn bindings_screen(
    ui: &mut egui::Ui,
    state: &mut MenuState,
    data: &MenuData,
    commands: &mut Vec<MenuCommand>,
) {
    ui.label("Click a cell to rebind, then press the input. Use x to clear a binding.");
    egui::ScrollArea::vertical()
        .max_height(300.0)
        .show(ui, |ui| {
            egui::Grid::new("bindings-grid")
                .num_columns(3)
                .striped(true)
                .show(ui, |ui| {
                    for row in data.bindings {
                        ui.label(&row.display);
                        for slot in 0..2 {
                            binding_cell(ui, state, row, slot, commands);
                        }
                        ui.end_row();
                    }
                });
        });
    ui.separator();
    ui.horizontal(|ui| {
        if ui.button("Back").clicked() {
            state.screen = Some(Screen::Options);
        }
        if ui.button("Reset To Defaults").clicked() {
            state.cancel_capture();
            commands.push(MenuCommand::ResetBindings);
        }
    });
}

fn binding_cell(
    ui: &mut egui::Ui,
    state: &mut MenuState,
    row: &BindingRow,
    slot: usize,
    commands: &mut Vec<MenuCommand>,
) {
    let current = row.triggers.get(slot);
    let capturing_here = state
        .capturing
        .as_ref()
        .is_some_and(|c| c.set == row.set && c.action == row.action && c.slot == slot);

    ui.horizontal(|ui| {
        let text = if capturing_here {
            "press…".to_owned()
        } else {
            current.map(trigger_label).unwrap_or_else(|| "—".to_owned())
        };

        let cell = ui.button(text);
        if cell.clicked() && !capturing_here {
            state.begin_capture(row.set, row.action.clone(), slot);
        }

        // While capturing this cell, offer an explicit cancel (Escape is a
        // bindable key now, so it can't double as the cancel). Otherwise, a
        // filled cell gets an "x" to unbind it.
        if capturing_here {
            if ui.small_button("cancel").clicked() {
                state.cancel_capture();
            }
        } else if current.is_some() && ui.small_button("x").clicked() {
            commands.push(MenuCommand::SetBinding {
                set: row.set,
                action: row.action.clone(),
                slot,
                trigger: None,
            });
        }
    });
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
    fn open_close_toggle_track_visibility() {
        let mut state = MenuState::default();
        assert!(!state.is_open());
        state.toggle();
        assert!(state.is_open());
        state.toggle();
        assert!(!state.is_open());
    }

    #[test]
    fn capture_yields_a_set_binding_then_clears() {
        let mut state = MenuState::default();
        state.begin_capture(BindingSet::System, "a".to_owned(), 1);
        assert!(state.is_capturing());
        let cmd = state.capture(InputTrigger::Key(Key::KeyQ));
        assert_eq!(
            cmd,
            Some(MenuCommand::SetBinding {
                set: BindingSet::System,
                action: "a".to_owned(),
                slot: 1,
                trigger: Some(InputTrigger::Key(Key::KeyQ)),
            })
        );
        assert!(!state.is_capturing());
        assert_eq!(state.capture(InputTrigger::Key(Key::KeyW)), None);
    }
}

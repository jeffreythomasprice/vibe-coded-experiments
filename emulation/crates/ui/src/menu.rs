//! The menu's state machine: which screen is showing, where the cursor is, and
//! what activating an entry does.

use common::InputTrigger;

use crate::nav::{Cursor, MenuInput};
use crate::{BindingRow, BindingSet, MenuCommand, MenuData};

/// Editable binding slots per action.
pub(crate) const BINDING_SLOTS: usize = 2;

/// Which menu screen is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Root,
    Options,
    InputBindings,
    SelectRom,
}

impl Screen {
    pub fn title(self) -> &'static str {
        match self {
            Screen::Root => "Menu",
            Screen::SelectRom => "Select ROM",
            Screen::Options => "Options",
            Screen::InputBindings => "Input Bindings",
        }
    }
}

/// A selectable thing on a screen. The keyboard/gamepad cursor and the mouse both
/// resolve to one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    Open(Screen),
    Exit,
    Rom(usize),
    Binding { row: usize, slot: usize },
    ResetBindings,
    Back,
}

/// The entries of `screen`, as rows of columns.
pub(crate) fn entries(screen: Screen, data: &MenuData) -> Vec<Vec<Entry>> {
    match screen {
        Screen::Root => vec![
            vec![Entry::Open(Screen::SelectRom)],
            vec![Entry::Open(Screen::Options)],
            vec![Entry::Exit],
        ],
        Screen::Options => vec![vec![Entry::Open(Screen::InputBindings)], vec![Entry::Back]],
        Screen::SelectRom => (0..data.roms.len())
            .map(|index| vec![Entry::Rom(index)])
            .chain([vec![Entry::Back]])
            .collect(),
        Screen::InputBindings => (0..data.bindings.len())
            .map(|row| {
                (0..BINDING_SLOTS)
                    .map(|slot| Entry::Binding { row, slot })
                    .collect()
            })
            .chain([vec![Entry::Back, Entry::ResetBindings]])
            .collect(),
    }
}

/// A pending rebind: the cell awaiting the next host input event.
#[derive(Clone, Debug)]
struct Capture {
    set: BindingSet,
    action: String,
    slot: usize,
}

/// The menu's navigation + capture state. Pure data — no egui/GPU handles — so
/// the host owns it and drives [`crate::draw`] with it each frame.
#[derive(Default)]
pub struct MenuState {
    screen: Option<Screen>,
    capturing: Option<Capture>,
    pending: Vec<MenuInput>,
    pub(crate) cursor: Cursor,
    /// First visible entry of the current screen's scrolling list.
    pub(crate) scroll: usize,
    /// Cursor and scroll of each parent screen, restored on the way back.
    history: Vec<(Screen, Cursor, usize)>,
}

impl MenuState {
    pub fn is_open(&self) -> bool {
        self.screen.is_some()
    }

    pub(crate) fn screen(&self) -> Option<Screen> {
        self.screen
    }

    /// Open the menu at its root screen.
    pub fn open(&mut self) {
        *self = MenuState {
            screen: Some(Screen::Root),
            ..MenuState::default()
        };
    }

    /// Close the menu and cancel any pending capture.
    pub fn close(&mut self) {
        *self = MenuState::default();
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

    pub(crate) fn is_capturing_cell(&self, row: &BindingRow, slot: usize) -> bool {
        self.capturing
            .as_ref()
            .is_some_and(|c| c.set == row.set && c.action == row.action && c.slot == slot)
    }

    pub(crate) fn begin_capture(&mut self, set: BindingSet, action: String, slot: usize) {
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

    /// Queue a menu control for the next [`crate::draw`]. Ignored while the menu
    /// is closed.
    pub fn navigate(&mut self, input: MenuInput) {
        if self.is_open() {
            self.pending.push(input);
        }
    }

    /// Apply every queued control, in order.
    pub(crate) fn process_inputs(&mut self, data: &MenuData, commands: &mut Vec<MenuCommand>) {
        for input in std::mem::take(&mut self.pending) {
            // While capturing, the next input belongs to the rebind.
            let Some(screen) = self.screen else { return };
            if self.is_capturing() {
                return;
            }
            let rows = entries(screen, data);
            let cols = |row: usize| rows.get(row).map_or(0, Vec::len);
            self.cursor = self.cursor.clamped(rows.len(), cols);
            let selected = rows
                .get(self.cursor.row)
                .and_then(|row| row.get(self.cursor.col))
                .copied();
            match input {
                MenuInput::Up | MenuInput::Down | MenuInput::Left | MenuInput::Right => {
                    self.cursor = self.cursor.moved(input, rows.len(), cols);
                }
                MenuInput::Confirm => {
                    if let Some(entry) = selected {
                        self.activate(entry, data, commands);
                    }
                }
                MenuInput::Back => self.back(commands),
                MenuInput::Clear => {
                    if let Some(Entry::Binding { row, slot }) = selected {
                        clear_binding(&data.bindings[row], slot, commands);
                    }
                }
            }
        }
    }

    /// Carry out the effect of choosing `entry`.
    pub(crate) fn activate(
        &mut self,
        entry: Entry,
        data: &MenuData,
        commands: &mut Vec<MenuCommand>,
    ) {
        match entry {
            Entry::Open(next) => {
                if let Some(current) = self.screen {
                    self.history.push((current, self.cursor, self.scroll));
                }
                self.screen = Some(next);
                self.cursor = Cursor::default();
                self.scroll = 0;
            }
            Entry::Exit => commands.push(MenuCommand::Exit),
            Entry::Rom(index) => {
                if let Some(rom) = data.roms.get(index) {
                    commands.push(MenuCommand::LoadRom(rom.id.clone()));
                }
            }
            Entry::Binding { row, slot } => {
                if let Some(row) = data.bindings.get(row) {
                    self.begin_capture(row.set, row.action.clone(), slot);
                }
            }
            Entry::ResetBindings => commands.push(MenuCommand::ResetBindings),
            Entry::Back => self.back(commands),
        }
    }

    /// Return to the parent screen, or ask to close the menu from the root.
    fn back(&mut self, commands: &mut Vec<MenuCommand>) {
        match self.history.pop() {
            Some((screen, cursor, scroll)) => {
                self.screen = Some(screen);
                self.cursor = cursor;
                self.scroll = scroll;
            }
            None => commands.push(MenuCommand::CloseMenu),
        }
    }
}

/// Unbind a slot, if it holds anything.
pub(crate) fn clear_binding(row: &BindingRow, slot: usize, commands: &mut Vec<MenuCommand>) {
    if row.triggers.get(slot).is_some() {
        commands.push(MenuCommand::SetBinding {
            set: row.set,
            action: row.action.clone(),
            slot,
            trigger: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RomChoice;
    use common::Key;

    fn run(state: &mut MenuState, data: &MenuData, inputs: &[MenuInput]) -> Vec<MenuCommand> {
        for &input in inputs {
            state.navigate(input);
        }
        let mut commands = Vec::new();
        state.process_inputs(data, &mut commands);
        commands
    }

    fn with_data(test: impl FnOnce(&MenuData)) {
        let roms = [RomChoice {
            id: "tetris.gb".to_owned(),
            label: "tetris".to_owned(),
        }];
        let bindings = [BindingRow {
            set: BindingSet::System,
            action: "a".to_owned(),
            display: "A".to_owned(),
            triggers: vec![InputTrigger::Key(Key::KeyX)],
        }];
        test(&MenuData {
            rom_loaded: false,
            roms: &roms,
            bindings: &bindings,
            speed_label: "1x",
            actual_speed: None,
            paused: false,
        });
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

    #[test]
    fn confirm_walks_into_the_rom_list_and_loads() {
        with_data(|data| {
            let mut state = MenuState::default();
            state.open();
            let commands = run(&mut state, data, &[MenuInput::Confirm, MenuInput::Confirm]);
            assert_eq!(commands, vec![MenuCommand::LoadRom("tetris.gb".to_owned())]);
        });
    }

    #[test]
    fn back_restores_the_parent_cursor_and_closes_from_the_root() {
        with_data(|data| {
            let mut state = MenuState::default();
            state.open();
            run(&mut state, data, &[MenuInput::Down, MenuInput::Confirm]);
            assert_eq!(state.screen(), Some(Screen::Options));
            assert_eq!(state.cursor.row, 0);
            run(&mut state, data, &[MenuInput::Back]);
            assert_eq!(state.screen(), Some(Screen::Root));
            assert_eq!(state.cursor.row, 1);
            let commands = run(&mut state, data, &[MenuInput::Back]);
            assert_eq!(commands, vec![MenuCommand::CloseMenu]);
        });
    }

    #[test]
    fn bindings_can_be_cleared_and_captured_from_the_cursor() {
        with_data(|data| {
            let mut state = MenuState::default();
            state.open();
            let to_bindings = [MenuInput::Down, MenuInput::Confirm, MenuInput::Confirm];
            run(&mut state, data, &to_bindings);

            let commands = run(&mut state, data, &[MenuInput::Clear]);
            assert_eq!(
                commands,
                vec![MenuCommand::SetBinding {
                    set: BindingSet::System,
                    action: "a".to_owned(),
                    slot: 0,
                    trigger: None,
                }]
            );
            // An empty slot has nothing to clear.
            assert!(run(&mut state, data, &[MenuInput::Right, MenuInput::Clear]).is_empty());

            // Controls queued behind the confirm don't move the cursor.
            run(&mut state, data, &[MenuInput::Confirm, MenuInput::Down]);
            assert!(state.is_capturing_cell(&data.bindings[0], 1));
            assert_eq!(state.cursor, Cursor { row: 0, col: 1 });
        });
    }

    #[test]
    fn controls_are_ignored_while_closed() {
        with_data(|data| {
            let mut state = MenuState::default();
            state.navigate(MenuInput::Confirm);
            state.open();
            assert!(run(&mut state, data, &[]).is_empty());
            assert_eq!(state.screen(), Some(Screen::Root));
        });
    }
}

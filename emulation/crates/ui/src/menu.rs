//! The menu's state machine: which screen is showing, where the cursor is, and
//! what activating an entry does.

use common::{InputTrigger, StateSlot};

use crate::nav::{Cursor, MenuInput};
use crate::{BindingRow, BindingSet, MenuCommand, MenuData};

/// Editable binding slots per action.
pub(crate) const BINDING_SLOTS: usize = 2;
/// Longest save-state name that can be typed.
pub(crate) const MAX_STATE_NAME: usize = 24;

/// Which menu screen is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Root,
    Options,
    InputBindings,
    SelectRom,
    SaveState,
    LoadState,
}

impl Screen {
    pub fn title(self) -> &'static str {
        match self {
            Screen::Root => "Menu",
            Screen::SelectRom => "Select ROM",
            Screen::SaveState => "Save State",
            Screen::LoadState => "Load State",
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
    /// Name a new save state.
    NewState,
    /// An existing save state; the screen decides whether to load or overwrite it.
    State(usize),
    /// Accept or abandon the name being typed.
    CommitName,
    CancelName,
    /// Restart the game from power-on.
    Reset,
    Binding { row: usize, slot: usize },
    ResetBindings,
    Back,
}

/// The entries of `screen`, as rows of columns.
pub(crate) fn entries(screen: Screen, data: &MenuData) -> Vec<Vec<Entry>> {
    match screen {
        Screen::Root => {
            let game: &[Entry] = if data.rom_loaded {
                &[
                    Entry::Open(Screen::SaveState),
                    Entry::Open(Screen::LoadState),
                    Entry::Reset,
                ]
            } else {
                &[]
            };
            [Entry::Open(Screen::SelectRom)]
                .iter()
                .chain(game)
                .chain(&[Entry::Open(Screen::Options), Entry::Exit])
                .map(|&entry| vec![entry])
                .collect()
        }
        Screen::SaveState => [vec![Entry::NewState]]
            .into_iter()
            // Only the user's own saves can be overwritten by hand.
            .chain(
                data.states
                    .iter()
                    .enumerate()
                    .filter(|(_, state)| matches!(state.slot, StateSlot::Manual(_)))
                    .map(|(index, _)| vec![Entry::State(index)]),
            )
            .chain([vec![Entry::Back]])
            .collect(),
        Screen::LoadState => (0..data.states.len())
            .map(|index| vec![Entry::State(index)])
            .chain([vec![Entry::Back]])
            .collect(),
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
    /// The save-state name being typed, if the name field is open.
    naming: Option<String>,
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

    /// Whether the user is typing a name. The host should then leave keyboard
    /// input to egui rather than translating it into controls or hotkeys.
    pub fn is_naming(&self) -> bool {
        self.naming.is_some()
    }

    pub(crate) fn name_being_typed(&self) -> Option<&str> {
        self.naming.as_deref()
    }

    /// Edit the open name field with this frame's keyboard events.
    pub(crate) fn process_text(&mut self, events: &[egui::Event], commands: &mut Vec<MenuCommand>) {
        for event in events {
            let Some(name) = self.naming.as_mut() else {
                return;
            };
            match event {
                egui::Event::Text(text) => {
                    for c in text.chars() {
                        let allowed = c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-');
                        if allowed && name.len() < MAX_STATE_NAME {
                            name.push(c);
                        }
                    }
                }
                egui::Event::Key {
                    key, pressed: true, ..
                } => match key {
                    egui::Key::Backspace => {
                        name.pop();
                    }
                    egui::Key::Enter => self.commit_name(commands),
                    egui::Key::Escape => self.naming = None,
                    _ => {}
                },
                _ => {}
            }
        }
    }

    /// Save under the typed name. A blank name leaves the field open.
    fn commit_name(&mut self, commands: &mut Vec<MenuCommand>) {
        let Some(name) = self.naming.as_deref().map(str::trim) else {
            return;
        };
        if !name.is_empty() {
            commands.push(MenuCommand::SaveState(StateSlot::Manual(name.to_owned())));
            self.naming = None;
        }
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
            // The name field has only two controls, wherever the cursor is.
            if self.is_naming() {
                match input {
                    MenuInput::Confirm => self.commit_name(commands),
                    MenuInput::Back => self.naming = None,
                    _ => {}
                }
                continue;
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
            Entry::NewState => self.naming = Some(data.suggested_state_name.to_owned()),
            Entry::State(index) => {
                if let Some(state) = data.states.get(index) {
                    let slot = state.slot.clone();
                    match self.screen {
                        Some(Screen::SaveState) => commands.push(MenuCommand::SaveState(slot)),
                        _ => commands.push(MenuCommand::LoadState(slot)),
                    }
                }
            }
            Entry::CommitName => self.commit_name(commands),
            Entry::CancelName => self.naming = None,
            Entry::Reset => commands.push(MenuCommand::ResetGame),
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
    use crate::{RomChoice, StateChoice};
    use common::{Corner, Key};

    fn run(state: &mut MenuState, data: &MenuData, inputs: &[MenuInput]) -> Vec<MenuCommand> {
        for &input in inputs {
            state.navigate(input);
        }
        let mut commands = Vec::new();
        state.process_inputs(data, &mut commands);
        commands
    }

    fn manual(name: &str) -> StateSlot {
        StateSlot::Manual(name.to_owned())
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
        let states = [StateSlot::Quick(1), manual("boss")].map(|slot| StateChoice {
            label: slot.label(),
            slot,
            screenshot: None,
        });
        test(&MenuData {
            rom_loaded: false,
            roms: &roms,
            bindings: &bindings,
            states: &states,
            suggested_state_name: "save-1",
            toast: None,
            speed_label: "1x",
            actual_speed: None,
            speed_corner: Corner::TopLeft,
            speed_opacity: 1.0,
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

    /// Open the menu with a game loaded and walk to a root entry.
    fn open_at(data: &MenuData, downs: usize) -> MenuState {
        let mut state = MenuState::default();
        state.open();
        let mut inputs = vec![MenuInput::Down; downs];
        inputs.push(MenuInput::Confirm);
        assert!(run(&mut state, data, &inputs).is_empty());
        state
    }

    fn with_game(test: impl FnOnce(&MenuData)) {
        with_data(|data| {
            test(&MenuData {
                rom_loaded: true,
                ..*data
            })
        });
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn typed(state: &mut MenuState, events: &[egui::Event]) -> Vec<MenuCommand> {
        let mut commands = Vec::new();
        state.process_text(events, &mut commands);
        commands
    }

    #[test]
    fn state_screens_need_a_loaded_game() {
        with_data(|data| {
            let root = entries(Screen::Root, data);
            assert!(!root.concat().contains(&Entry::Open(Screen::LoadState)));
        });
        with_game(|data| {
            let root = entries(Screen::Root, data);
            assert_eq!(root[1], vec![Entry::Open(Screen::SaveState)]);
            assert_eq!(root[2], vec![Entry::Open(Screen::LoadState)]);
        });
    }

    #[test]
    fn reset_is_offered_for_a_loaded_game() {
        with_data(|data| {
            assert!(!entries(Screen::Root, data).concat().contains(&Entry::Reset));
        });
        with_game(|data| {
            let mut state = MenuState::default();
            state.open();
            let mut inputs = vec![MenuInput::Down; 3];
            inputs.push(MenuInput::Confirm);
            assert_eq!(run(&mut state, data, &inputs), vec![MenuCommand::ResetGame]);
        });
    }

    #[test]
    fn choosing_a_state_loads_or_overwrites_it() {
        with_game(|data| {
            let mut state = open_at(data, 2);
            let commands = run(&mut state, data, &[MenuInput::Down, MenuInput::Confirm]);
            assert_eq!(commands, vec![MenuCommand::LoadState(manual("boss"))]);

            // The save screen lists "New Save" first, then only manual saves.
            let mut state = open_at(data, 1);
            let commands = run(&mut state, data, &[MenuInput::Down, MenuInput::Confirm]);
            assert_eq!(commands, vec![MenuCommand::SaveState(manual("boss"))]);
            assert_eq!(
                entries(Screen::SaveState, data),
                [
                    vec![Entry::NewState],
                    vec![Entry::State(1)],
                    vec![Entry::Back]
                ]
            );
        });
    }

    #[test]
    fn a_new_save_can_be_typed_and_committed() {
        with_game(|data| {
            let mut state = open_at(data, 1);
            assert!(run(&mut state, data, &[MenuInput::Confirm]).is_empty());
            assert_eq!(state.name_being_typed(), Some("save-1"));

            let mut events = vec![key(egui::Key::Backspace); 6];
            events.push(egui::Event::Text("My ../Run_2!".to_owned()));
            events.push(key(egui::Key::Enter));
            // Anything after the commit is not part of the name.
            events.push(egui::Event::Text("x".to_owned()));
            assert_eq!(
                typed(&mut state, &events),
                vec![MenuCommand::SaveState(manual("My Run_2"))]
            );
            assert!(!state.is_naming());
        });
    }

    #[test]
    fn typed_names_are_bounded_and_never_blank() {
        with_game(|data| {
            let mut state = open_at(data, 1);
            run(&mut state, data, &[MenuInput::Confirm]);
            typed(&mut state, &[egui::Event::Text("a".repeat(100))]);
            assert_eq!(state.name_being_typed().map(str::len), Some(MAX_STATE_NAME));

            let mut events = vec![key(egui::Key::Backspace); MAX_STATE_NAME];
            events.push(egui::Event::Text("  ".to_owned()));
            events.push(key(egui::Key::Enter));
            assert!(typed(&mut state, &events).is_empty());
            assert!(state.is_naming());
        });
    }

    #[test]
    fn the_name_field_can_be_cancelled_or_accepted_with_menu_controls() {
        with_game(|data| {
            let mut state = open_at(data, 1);
            run(&mut state, data, &[MenuInput::Confirm]);
            assert!(typed(&mut state, &[key(egui::Key::Escape)]).is_empty());
            assert!(!state.is_naming());

            run(&mut state, data, &[MenuInput::Confirm, MenuInput::Back]);
            assert!(!state.is_naming());
            assert_eq!(state.screen(), Some(Screen::SaveState));

            let inputs = [MenuInput::Confirm, MenuInput::Down, MenuInput::Confirm];
            let commands = run(&mut state, data, &inputs);
            assert_eq!(commands, vec![MenuCommand::SaveState(manual("save-1"))]);
            assert_eq!(state.cursor.row, 0, "the cursor stayed put while naming");
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

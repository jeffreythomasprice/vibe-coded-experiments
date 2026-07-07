use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use common::input::{Action, GenericAction};
use common::AudioSettings;
use common::{
    ActionBindings, InputBindings, InputTrigger, RomId, RomLibrary, SaveId, ScaleMode, SpeedMode,
};
use gameboy::{Cartridge, Emulator, GameboyButton};
use gilrs::Gilrs;
use ui::{BindingRow, BindingSet, MenuCommand, MenuData, MenuState, RomChoice};
use video::EguiPaint;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::catalog::RomCatalog;
use crate::egui_platform::EguiPlatform;
use crate::gfx::Gfx;
use crate::input::{self, InputRouter};
use crate::pace::Pacer;
use crate::snd::Snd;
use crate::storage::FileStore;

/// Minimum wall-clock spacing between presents. The renderer's swapchain is
/// vsync-locked (Fifo), so calling `request_redraw` faster than the display can
/// refresh fills the swapchain queue and then blocks every present on the next
/// vblank — which would serialize emulation behind the ~60 Hz display and cap
/// even the "as fast as possible" mode at the refresh rate. Presenting at most
/// this often decouples emulation speed from the display. Chosen just under the
/// ~16.74 ms native frame period so native-rate frames are never dropped, yet at
/// or above a 60 Hz panel's ~16.67 ms period so we don't request presents faster
/// than the display can retire them (which would refill the swapchain queue and
/// reintroduce the vsync stall this is meant to avoid).
const MIN_REDRAW_INTERVAL: Duration = Duration::from_micros(16_700);

/// The winit application: owns the window and its renderer once resumed, the
/// persistent-state store, the loaded cartridge, and the menu overlay.
///
/// winit 0.30 drives everything through [`ApplicationHandler`], creating the
/// window in `resumed` rather than up front so it works uniformly across
/// desktop and mobile lifecycles.
pub struct App {
    window: Option<Arc<Window>>,
    gfx: Option<Gfx>,
    snd: Option<Snd>,
    /// The winit+egui glue, built alongside the window in `resumed`.
    egui: Option<EguiPlatform>,
    /// The shared UI's navigation/capture state (backend-agnostic).
    menu: MenuState,
    store: FileStore,
    catalog: RomCatalog,
    emu: Option<Emulator>,
    save_id: Option<SaveId>,
    scale_mode: ScaleMode,
    audio: AudioSettings,
    input: InputRouter,
    /// The editable source of truth for bindings (the router keeps only reverse
    /// indices), used to display, edit, persist, and rebuild the router.
    generic_bindings: InputBindings,
    gameboy_bindings: ActionBindings,
    gilrs: Option<Gilrs>,
    pacer: Pacer,
    /// Set once the CPU locks on an illegal opcode, to stop driving it.
    faulted: bool,
    /// Whether the user has paused the machine (independent of the menu, which
    /// also pauses emulation while open).
    paused: bool,
    /// When the last present was requested, to throttle redraws to the display
    /// rate (see [`MIN_REDRAW_INTERVAL`]). `None` until the first present.
    last_redraw: Option<Instant>,
}

impl App {
    /// Create the application with a store and an optional already-loaded cartridge
    /// (plus its save identity). `scale_mode` comes from the loaded settings; the
    /// input router is built from the loaded generic and Game Boy bindings.
    pub fn new(
        store: FileStore,
        cartridge: Option<Cartridge>,
        save_id: Option<SaveId>,
        scale_mode: ScaleMode,
        speed_override: Option<SpeedMode>,
    ) -> App {
        let settings = store.settings();
        let generic_bindings = settings.input.clone();
        let gameboy_bindings = store.gameboy_bindings();
        let input = InputRouter::new(&generic_bindings, &gameboy_bindings);
        let audio = settings.audio.clone();
        // The CLI override wins over the persisted setting; neither is written back.
        let speed = speed_override.unwrap_or(settings.emulation.speed);
        let pacer = Pacer::new(speed);
        let emu = cartridge.map(Emulator::new);
        let catalog = RomCatalog::load(&store);
        let gilrs = match Gilrs::new() {
            Ok(gilrs) => Some(gilrs),
            Err(err) => {
                tracing::warn!(%err, "gamepad input unavailable");
                None
            }
        };

        App {
            window: None,
            gfx: None,
            snd: None,
            egui: None,
            menu: MenuState::default(),
            store,
            catalog,
            emu,
            save_id,
            scale_mode,
            audio,
            input,
            generic_bindings,
            gameboy_bindings,
            gilrs,
            pacer,
            faulted: false,
            paused: false,
            last_redraw: None,
        }
    }

    /// Request a present, but at most once per [`MIN_REDRAW_INTERVAL`], so we
    /// don't outrun the vsync-locked swapchain and stall emulation behind the
    /// display's vblank. `force` bypasses the throttle for one-shot redraws
    /// (initial paint, resize) that must land regardless of timing.
    fn request_present(&mut self, force: bool) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let now = Instant::now();
        if force || self.last_redraw.is_none_or(|t| now - t >= MIN_REDRAW_INTERVAL) {
            self.last_redraw = Some(now);
            window.request_redraw();
        }
    }

    /// Persist battery-backed cartridge RAM, if any, through the store.
    fn save_battery(&self) {
        if let (Some(emu), Some(id)) = (&self.emu, &self.save_id) {
            crate::rom::save_battery(emu.cartridge(), id, &self.store);
        }
    }

    /// Drain and act on any generic actions the router queued this event.
    fn apply_generic_actions(&mut self) {
        for action in self.input.take_generic_actions() {
            match action {
                GenericAction::SaveBattery => self.save_battery(),
                GenericAction::Menu => self.menu.toggle(),
                GenericAction::IncreaseSpeed => {
                    self.pacer.set_mode(self.pacer.mode().increased())
                }
                GenericAction::DecreaseSpeed => {
                    self.pacer.set_mode(self.pacer.mode().decreased())
                }
                GenericAction::TogglePause => {
                    self.paused = !self.paused;
                    // Re-arm the pacer so the paused interval isn't owed as frames.
                    if !self.paused {
                        self.pacer.resync();
                    }
                }
            }
        }
    }

    /// Carry out a command the menu produced.
    fn apply_menu_command(&mut self, command: MenuCommand, event_loop: &ActiveEventLoop) {
        match command {
            MenuCommand::LoadRom(id) => {
                if let Err(err) = self.load_rom(&RomId(id)) {
                    tracing::error!(%err, "failed to load rom from the menu");
                }
            }
            // The battery flush happens in `Drop` when the loop returns, which
            // covers this and every other exit path.
            MenuCommand::Exit => event_loop.exit(),
            MenuCommand::CloseMenu => self.menu.close(),
            MenuCommand::SetBinding {
                set,
                action,
                slot,
                trigger,
            } => self.apply_binding_edit(set, &action, slot, trigger),
            MenuCommand::ResetBindings => self.reset_bindings(),
        }
    }

    /// Hot-swap the running cartridge for the ROM with `id`, mirroring the
    /// `main.rs::run_rom` load path: flush the current battery, read+parse the new
    /// ROM, restore its battery, and rebuild the emulator. Closes the menu on
    /// success.
    fn load_rom(&mut self, id: &RomId) -> anyhow::Result<()> {
        self.save_battery();

        let bytes = self
            .store
            .read_rom(id)
            .with_context(|| format!("failed to read rom {}", id.0))?;
        let mut cart = Cartridge::from_bytes(&bytes)
            .with_context(|| format!("failed to parse rom {}", id.0))?;

        // The save identity wants the ROM's file stem as a fallback for blank
        // header titles; the catalog already carries it.
        let stem = self
            .catalog
            .roms()
            .iter()
            .find(|info| &info.id == id)
            .map(|info| info.file_stem.clone())
            .unwrap_or_else(|| id.0.clone());
        let save_id = crate::rom::save_id_for(&cart, &stem);
        crate::rom::restore_battery(&mut cart, &save_id, &self.store)?;

        self.emu = Some(Emulator::new(cart));
        self.save_id = Some(save_id);
        self.faulted = false;
        self.menu.close();
        tracing::info!(rom = %id.0, "loaded rom from the menu");
        Ok(())
    }

    /// Apply a rebind from the menu: seed from the action's currently *resolved*
    /// triggers (defaults included) so filling the second slot preserves the
    /// first, set or clear the chosen slot, then rebuild the router and persist.
    fn apply_binding_edit(
        &mut self,
        set: BindingSet,
        action: &str,
        slot: usize,
        trigger: Option<InputTrigger>,
    ) {
        let triggers = edit_triggers(self.resolved_triggers(set, action), slot, trigger);

        match set {
            BindingSet::Generic => self.generic_bindings.0.set(action, triggers),
            BindingSet::System => self.gameboy_bindings.set(action, triggers),
        }

        self.rebuild_and_persist_bindings();
    }

    /// Drop every explicit binding override so both sets fall back to their code
    /// defaults, then rebuild the router and persist.
    fn reset_bindings(&mut self) {
        self.generic_bindings = InputBindings::default();
        self.gameboy_bindings = ActionBindings::default();
        self.rebuild_and_persist_bindings();
        tracing::info!("reset input bindings to defaults");
    }

    /// Rebuild the input router from the current bindings and persist them.
    fn rebuild_and_persist_bindings(&mut self) {
        self.input = InputRouter::new(&self.generic_bindings, &self.gameboy_bindings);
        if let Err(err) = self
            .store
            .save_input_bindings(&self.generic_bindings, &self.gameboy_bindings)
        {
            tracing::error!(%err, "failed to persist input bindings");
        }
    }

    /// The triggers an action currently resolves to (explicit config or code
    /// default), looked up by its stable name within the given binding set.
    fn resolved_triggers(&self, set: BindingSet, action: &str) -> Vec<InputTrigger> {
        match set {
            BindingSet::Generic => GenericAction::all()
                .iter()
                .find(|a| a.name() == action)
                .map(|a| self.generic_bindings.0.triggers(a))
                .unwrap_or_default(),
            BindingSet::System => GameboyButton::all()
                .iter()
                .find(|b| b.name() == action)
                .map(|b| self.gameboy_bindings.triggers(b))
                .unwrap_or_default(),
        }
    }

    /// The ROM catalog as menu entries.
    fn build_rom_choices(&self) -> Vec<RomChoice> {
        self.catalog
            .roms()
            .iter()
            .map(|info| {
                let title = info.title();
                let label = if title.is_empty() {
                    info.file_stem.clone()
                } else {
                    format!("{} ({title})", info.file_stem)
                };
                RomChoice {
                    id: info.id.0.clone(),
                    label,
                }
            })
            .collect()
    }

    /// Every bindable action as a menu row: generic actions first, then the Game
    /// Boy buttons, each with its currently resolved triggers.
    fn build_binding_rows(&self) -> Vec<BindingRow> {
        let mut rows = Vec::new();
        for action in GenericAction::all() {
            rows.push(BindingRow {
                set: BindingSet::Generic,
                action: action.name().to_string(),
                display: prettify(action.name()),
                triggers: self.generic_bindings.0.triggers(action),
            });
        }
        for button in GameboyButton::all() {
            rows.push(BindingRow {
                set: BindingSet::System,
                action: button.name().to_string(),
                display: prettify(button.name()),
                triggers: self.gameboy_bindings.triggers(button),
            });
        }
        rows
    }

    /// Build and run the egui UI for this frame — the always-on HUD plus the menu
    /// when it's open — applying any commands the menu produced. Runs every frame
    /// (the HUD is always drawn), so it returns paint whenever the egui platform
    /// and window exist. The ROM/binding lists are only built when the menu is
    /// open; the HUD needs neither.
    fn run_ui_frame(&mut self, event_loop: &ActiveEventLoop) -> Option<EguiPaint> {
        let menu_open = self.menu.is_open();
        let (roms, rows) = if menu_open {
            (self.build_rom_choices(), self.build_binding_rows())
        } else {
            (Vec::new(), Vec::new())
        };
        let speed_label = self.pacer.mode().label();
        // Only meaningful while actually running; hide the measurement when the
        // machine is halted so a stale reading isn't shown.
        let actual_speed = if self.paused || menu_open {
            None
        } else {
            self.pacer.actual_speed()
        };
        let data = MenuData {
            rom_loaded: self.emu.is_some(),
            roms: &roms,
            bindings: &rows,
            speed_label: &speed_label,
            actual_speed,
            paused: self.paused,
        };

        let window = self.window.clone()?;
        let (paint, commands) = {
            let egui = self.egui.as_mut()?;
            egui.run(&window, &mut self.menu, &data)
        };
        for command in commands {
            self.apply_menu_command(command, event_loop);
        }
        Some(paint)
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let paint = self.run_ui_frame(event_loop);
        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };
        match gfx.render(paint.as_ref()) {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                gfx.reconfigure();
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                tracing::error!("surface out of memory, exiting");
                event_loop.exit();
            }
            Err(err) => tracing::warn!(%err, "dropped frame"),
        }
    }

    /// Handle a physical key transition, split three ways: resolve a pending
    /// rebind capture, route to the game (menu closed, egui didn't consume), or
    /// fire only global hotkeys (menu open — so the menu key still toggles).
    fn handle_key(
        &mut self,
        code: KeyCode,
        pressed: bool,
        egui_consumed: bool,
        event_loop: &ActiveEventLoop,
    ) {
        if self.menu.is_capturing() {
            // Every key is bindable, Escape included — the menu's own "cancel"
            // control abandons a pending rebind instead.
            if pressed {
                if let Some(mapped) = input::key_from_winit(code) {
                    if let Some(command) = self.menu.capture(InputTrigger::Key(mapped)) {
                        self.apply_menu_command(command, event_loop);
                    }
                }
            }
            return;
        }

        let Some(mapped) = input::key_from_winit(code) else {
            return;
        };
        let trigger = InputTrigger::Key(mapped);
        if !self.menu.is_open() && !egui_consumed {
            self.input.handle(trigger, pressed);
        } else if pressed {
            self.input.handle_generic(trigger);
        }
    }

    fn handle_mouse(
        &mut self,
        button: winit::event::MouseButton,
        pressed: bool,
        egui_consumed: bool,
        event_loop: &ActiveEventLoop,
    ) {
        let trigger = InputTrigger::MouseButton(input::mouse_from_winit(button));
        if self.menu.is_capturing() {
            // A click egui consumed is the user operating the menu (e.g. the
            // cancel/x controls), not a mouse button to bind; only a click
            // outside the menu becomes a binding.
            if pressed && !egui_consumed {
                if let Some(command) = self.menu.capture(trigger) {
                    self.apply_menu_command(command, event_loop);
                }
            }
            return;
        }
        if !self.menu.is_open() && !egui_consumed {
            self.input.handle(trigger, pressed);
        } else if pressed {
            self.input.handle_generic(trigger);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("Emulator")
            .with_inner_size(LogicalSize::new(160 * 4, 144 * 4));

        let window = match event_loop.create_window(attrs) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                tracing::error!(%err, "failed to create window");
                event_loop.exit();
                return;
            }
        };

        // Open the audio device (independent of the window). Unlike graphics, a
        // failure here is non-fatal: a machine with no output device still runs,
        // just silently.
        if self.audio.enabled && self.snd.is_none() {
            match Snd::new(&self.audio) {
                Ok(snd) => self.snd = Some(snd),
                Err(err) => {
                    tracing::error!(%err, "failed to initialize audio; continuing without sound")
                }
            }
        }

        match pollster::block_on(Gfx::new(window.clone(), self.scale_mode)) {
            Ok(gfx) => {
                self.egui = Some(EguiPlatform::new(&window));
                // With no ROM loaded, come up in menu mode so the user can pick one.
                if self.emu.is_none() {
                    self.menu.open();
                }
                // Kick one redraw so the placeholder (and menu, if open) shows;
                // from here redraws are driven by the run loop.
                window.request_redraw();
                self.window = Some(window);
                self.gfx = Some(gfx);
            }
            Err(err) => {
                tracing::error!(%err, "failed to initialize graphics");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // Feed egui first (it needs pointer moves, wheel, focus, etc.), and note
        // whether it consumed the event so the game input router can skip it.
        let window = self.window.clone();
        let egui_consumed = match (self.egui.as_mut(), window.as_ref()) {
            (Some(egui), Some(window)) => egui.on_window_event(window, &event).consumed,
            _ => false,
        };

        // Input handling comes before the gfx guard so it works even during
        // startup and regardless of the renderer.
        match &event {
            WindowEvent::KeyboardInput { event: key, .. } if !key.repeat => {
                if let PhysicalKey::Code(code) = key.physical_key {
                    let pressed = key.state == ElementState::Pressed;
                    self.handle_key(code, pressed, egui_consumed, event_loop);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = *state == ElementState::Pressed;
                self.handle_mouse(*button, pressed, egui_consumed, event_loop);
            }
            _ => {}
        }

        self.apply_generic_actions();

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("close requested");
                // The battery save is flushed by `Drop` when the event loop
                // returns, which also covers the non-graceful exit paths.
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(gfx) = self.gfx.as_mut() {
                    gfx.resize(size);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Drain gamepad events each poll iteration. Disjoint field borrows so the
        // router/menu can be updated while the gilrs context is borrowed; captured
        // rebinds are applied after the borrow scope ends.
        let mut captured = Vec::new();
        {
            let App {
                gilrs,
                input: router,
                menu,
                ..
            } = self;
            if let Some(gilrs) = gilrs.as_mut() {
                while let Some(event) = gilrs.next_event() {
                    let (button, pressed) = match event.event {
                        gilrs::EventType::ButtonPressed(button, _) => (button, true),
                        gilrs::EventType::ButtonReleased(button, _) => (button, false),
                        _ => continue,
                    };
                    let Some(mapped) = crate::input::gamepad_from_gilrs(button) else {
                        continue;
                    };
                    let trigger = InputTrigger::GamepadButton(mapped);
                    if menu.is_capturing() {
                        if pressed {
                            if let Some(command) = menu.capture(trigger) {
                                captured.push(command);
                            }
                        }
                    } else if !menu.is_open() {
                        router.handle(trigger, pressed);
                    }
                }
            }
        }
        for command in captured {
            self.apply_menu_command(command, event_loop);
        }
        self.apply_generic_actions();

        // Drive the emulator forward per the pacer, then arm the event loop. While
        // the menu is open or the machine is paused it doesn't advance, so pump
        // redraws ourselves (the run loop no longer does) so the HUD stays live,
        // and poll for a responsive UI.
        self.drive_emulation();
        if self.menu.is_open() || self.paused {
            // Nothing to emulate; keep the overlay/HUD live at the display rate
            // and sleep until the next redraw is due rather than busy-spinning
            // on `Poll` (the vsync block used to throttle this implicitly).
            self.request_present(false);
            match self.last_redraw.map(|t| t + MIN_REDRAW_INTERVAL) {
                Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
                None => event_loop.set_control_flow(ControlFlow::Poll),
            }
        } else {
            event_loop.set_control_flow(self.pacer.control_flow());
        }
    }
}

impl App {
    /// Run the machine forward by however many frames the pacer says are due,
    /// uploading the latest frame and submitting audio. A no-op until the
    /// renderer exists, when there's no cartridge, once the CPU has faulted, or
    /// while the menu is open (the machine pauses behind the menu).
    fn drive_emulation(&mut self) {
        if self.faulted
            || self.paused
            || self.gfx.is_none()
            || self.emu.is_none()
            || self.menu.is_open()
        {
            return;
        }

        // Disjoint field borrows so the per-frame closure can hold `emu`/`snd`
        // mutably while the pacer (also `&mut`) drives it, and `gfx` stays free
        // for the post-catch-up upload. Scoped in a block so the borrows drop
        // before the `&mut self` `request_present` below.
        let mut produced = false;
        let mut fault = None;
        {
            let App {
                emu,
                gfx,
                snd,
                input,
                pacer,
                faulted,
                ..
            } = self;
            let (Some(emu), Some(gfx)) = (emu.as_mut(), gfx.as_mut()) else {
                return;
            };

            emu.set_buttons(input.gameboy_pressed());

            let submit_audio = pacer.submit_audio();
            pacer.tick(|| {
                let result = emu.run_frame();
                // Always drain the APU so its buffer can't grow unbounded, even
                // when we're not submitting (fast-forward / unbounded mute audio).
                let samples = emu.take_audio_samples();
                if submit_audio {
                    if let Some(snd) = snd.as_mut() {
                        if let Err(err) = snd.submit_frame(&samples) {
                            tracing::warn!(%err, "failed to submit audio frame");
                        }
                    }
                }
                if let Some(f) = result.fault {
                    fault = Some(f);
                }
                produced = true;
            });

            if produced {
                if let Err(err) = gfx.update_frame(emu.framebuffer()) {
                    tracing::warn!(%err, "failed to upload frame to the renderer");
                }
            }

            if let Some(fault) = fault {
                *faulted = true;
                tracing::error!(%fault, "CPU faulted on an illegal opcode; halting emulation");
            }
        }

        // Presentation is throttled to the display rate (not to how many frames
        // we just emulated) so a fast/unbounded run isn't serialized behind
        // vsync — see `request_present`.
        if produced {
            self.request_present(false);
        }
    }
}

impl Drop for App {
    /// Flush the battery save on the way out. `App` is a local of the run
    /// function, so this fires when `run_app` returns — covering every exit path
    /// (graceful close, surface loss, init failure), not just `CloseRequested`.
    fn drop(&mut self) {
        self.save_battery();
    }
}

/// Turn a stable action name (`save_battery`, `up`) into a display label
/// (`Save Battery`, `Up`) for the bindings table.
fn prettify(name: &str) -> String {
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Apply a single rebind to an action's resolved trigger list. Setting seeds from
/// the current list so filling the second slot preserves the first; a slot past
/// the end appends (there is no representable gap). Clearing (`None`) removes the
/// slot if present.
fn edit_triggers(
    mut triggers: Vec<InputTrigger>,
    slot: usize,
    trigger: Option<InputTrigger>,
) -> Vec<InputTrigger> {
    match trigger {
        Some(t) => {
            let index = slot.min(triggers.len());
            if index < triggers.len() {
                triggers[index] = t;
            } else {
                triggers.push(t);
            }
        }
        None => {
            if slot < triggers.len() {
                triggers.remove(slot);
            }
        }
    }
    triggers
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::input::Key;

    fn key(k: Key) -> InputTrigger {
        InputTrigger::Key(k)
    }

    #[test]
    fn prettify_titlecases_and_splits_underscores() {
        assert_eq!(prettify("save_battery"), "Save Battery");
        assert_eq!(prettify("up"), "Up");
        assert_eq!(prettify("a"), "A");
    }

    #[test]
    fn setting_slot_0_replaces_the_first_trigger() {
        let out = edit_triggers(vec![key(Key::ArrowUp)], 0, Some(key(Key::KeyW)));
        assert_eq!(out, vec![key(Key::KeyW)]);
    }

    #[test]
    fn setting_slot_1_preserves_the_first_trigger() {
        let out = edit_triggers(vec![key(Key::ArrowUp)], 1, Some(key(Key::KeyW)));
        assert_eq!(out, vec![key(Key::ArrowUp), key(Key::KeyW)]);
    }

    #[test]
    fn setting_a_slot_past_the_end_appends_rather_than_leaving_a_gap() {
        // Second slot of an unbound action lands in the first free slot.
        let out = edit_triggers(vec![], 1, Some(key(Key::KeyW)));
        assert_eq!(out, vec![key(Key::KeyW)]);
    }

    #[test]
    fn clearing_removes_the_slot_and_missing_slot_is_a_noop() {
        let out = edit_triggers(vec![key(Key::ArrowUp), key(Key::KeyW)], 0, None);
        assert_eq!(out, vec![key(Key::KeyW)]);
        let out = edit_triggers(vec![key(Key::ArrowUp)], 1, None);
        assert_eq!(out, vec![key(Key::ArrowUp)]);
    }
}

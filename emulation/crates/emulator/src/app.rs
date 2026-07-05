use std::sync::Arc;

use common::input::GenericAction;
use common::{AudioSettings, InputTrigger, SaveId, ScaleMode};
use gameboy::{Cartridge, Emulator};
use gilrs::Gilrs;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::gfx::Gfx;
use crate::input::{self, InputRouter};
use crate::pace::Pacer;
use crate::snd::Snd;
use crate::storage::FileStore;

/// The winit application: owns the window and its renderer once resumed, the
/// persistent-state store, and the loaded cartridge.
///
/// winit 0.30 drives everything through [`ApplicationHandler`], creating the
/// window in `resumed` rather than up front so it works uniformly across
/// desktop and mobile lifecycles.
pub struct App {
    window: Option<Arc<Window>>,
    gfx: Option<Gfx>,
    snd: Option<Snd>,
    store: FileStore,
    emu: Option<Emulator>,
    save_id: Option<SaveId>,
    scale_mode: ScaleMode,
    audio: AudioSettings,
    input: InputRouter,
    gilrs: Option<Gilrs>,
    pacer: Pacer,
    /// Set once the CPU locks on an illegal opcode, to stop driving it.
    faulted: bool,
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
    ) -> App {
        let settings = store.settings();
        let input = InputRouter::new(&settings.input, &store.gameboy_bindings());
        let audio = settings.audio.clone();
        let pacer = Pacer::new(settings.emulation.speed);
        let emu = cartridge.map(Emulator::new);
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
            store,
            emu,
            save_id,
            scale_mode,
            audio,
            input,
            gilrs,
            pacer,
            faulted: false,
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
                GenericAction::Menu => tracing::info!("menu requested (no menu UI yet)"),
            }
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
                // Kick one redraw so the placeholder shows even with no ROM
                // loaded; from here redraws are driven by the run loop (which
                // requests one after each emulated frame), not a self-sustaining
                // chain.
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
        // Input first, so it is handled even before the renderer exists and
        // regardless of the gfx guard below.
        match &event {
            WindowEvent::KeyboardInput { event: key, .. } if !key.repeat => {
                if let PhysicalKey::Code(code) = key.physical_key {
                    if let Some(mapped) = input::key_from_winit(code) {
                        let pressed = key.state == ElementState::Pressed;
                        self.input.handle(InputTrigger::Key(mapped), pressed);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let mapped = input::mouse_from_winit(*button);
                let pressed = *state == ElementState::Pressed;
                self.input.handle(InputTrigger::MouseButton(mapped), pressed);
            }
            _ => {}
        }

        // Act on generic actions (e.g. a manual save) even before the renderer
        // exists and regardless of the gfx guard below.
        self.apply_generic_actions();

        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("close requested");
                // The battery save is flushed by `Drop` when the event loop
                // returns, which also covers the non-graceful exit paths.
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                gfx.resize(size);
            }
            WindowEvent::RedrawRequested => {
                // Rendering only: present whatever the run loop last uploaded.
                // The next redraw is requested by the run loop when it produces
                // a new frame, not here.
                match gfx.render() {
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
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Drain gamepad events each poll iteration. Disjoint field borrows so the
        // router can be updated while the gilrs context is borrowed; the scope
        // ends those borrows before we act on any queued generic actions.
        {
            let App {
                gilrs, input: router, ..
            } = self;
            if let Some(gilrs) = gilrs.as_mut() {
                while let Some(event) = gilrs.next_event() {
                    let (button, pressed) = match event.event {
                        gilrs::EventType::ButtonPressed(button, _) => (button, true),
                        gilrs::EventType::ButtonReleased(button, _) => (button, false),
                        _ => continue,
                    };
                    if let Some(mapped) = crate::input::gamepad_from_gilrs(button) {
                        router.handle(InputTrigger::GamepadButton(mapped), pressed);
                    }
                }
            }
        }
        self.apply_generic_actions();

        // Drive the emulator forward per the pacer, then arm the event loop to
        // wake at the next frame's deadline (or spin, when unbounded).
        self.drive_emulation();
        event_loop.set_control_flow(self.pacer.control_flow());
    }
}

impl App {
    /// Run the machine forward by however many frames the pacer says are due,
    /// uploading the latest frame and submitting audio. A no-op until the
    /// renderer exists, when there's no cartridge, or once the CPU has faulted.
    fn drive_emulation(&mut self) {
        if self.faulted || self.gfx.is_none() || self.emu.is_none() {
            return;
        }

        // Disjoint field borrows so the per-frame closure can hold `emu`/`snd`
        // mutably while the pacer (also `&mut`) drives it, and `gfx`/`window`
        // stay free for the post-catch-up upload.
        let App {
            emu,
            gfx,
            snd,
            window,
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
        let mut produced = false;
        let mut fault = None;
        pacer.tick(|| {
            let result = emu.run_frame();
            // Always drain the APU so its buffer can't grow unbounded, even when
            // we're not submitting (fast-forward / unbounded mute audio).
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
            if let Some(window) = window.as_ref() {
                window.request_redraw();
            }
        }

        if let Some(fault) = fault {
            *faulted = true;
            tracing::error!(%fault, "CPU faulted on an illegal opcode; halting emulation");
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

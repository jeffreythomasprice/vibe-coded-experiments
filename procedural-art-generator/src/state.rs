use std::collections::HashSet;

use glam::Vec2;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::Key;

use crate::graphics::immediate::ImmediateRenderer;

pub enum StateTransition {
    Continue,
    Switch(Box<dyn AppState>),
    Quit,
}

pub struct InitContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub surface_format: wgpu::TextureFormat,
    pub viewport_size: (u32, u32),
}

pub struct EventContext {
    pub viewport_size: (u32, u32),
}

pub struct UpdateContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub viewport_size: (u32, u32),
    pub dt: f32,
}

pub struct RenderContext<'a> {
    pub renderer: &'a mut ImmediateRenderer,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub view: &'a wgpu::TextureView,
    pub viewport_size: (u32, u32),
}

pub trait AppState {
    fn event(&mut self, ctx: &EventContext, event: &WindowEvent) -> StateTransition;
    fn update(&mut self, ctx: &UpdateContext) -> StateTransition;
    fn render(&mut self, ctx: &mut RenderContext);
    fn overlay_ui(&mut self, _ctx: &egui::Context) {}
}

pub struct InputState {
    pub keys_held: HashSet<Key>,
    pub right_mouse_held: bool,
    pub cursor_pos: Vec2,
    pub prev_cursor_pos: Vec2,
    pub scroll_delta: f32,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    pub fn new() -> Self {
        Self {
            keys_held: HashSet::new(),
            right_mouse_held: false,
            cursor_pos: Vec2::ZERO,
            prev_cursor_pos: Vec2::ZERO,
            scroll_delta: 0.0,
        }
    }

    pub fn is_held(&self, ch: &str) -> bool {
        self.keys_held.contains(&Key::Character(ch.into()))
    }

    pub fn handle_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => match event.state {
                ElementState::Pressed => {
                    self.keys_held.insert(event.logical_key.clone());
                }
                ElementState::Released => {
                    self.keys_held.remove(&event.logical_key);
                }
            },
            WindowEvent::MouseInput { state, button, .. } => {
                if *button == MouseButton::Right {
                    self.right_mouse_held = state.is_pressed();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = Vec2::new(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 40.0,
                };
                self.scroll_delta += lines;
            }
            _ => {}
        }
    }

    pub fn end_frame(&mut self) {
        self.scroll_delta = 0.0;
        self.prev_cursor_pos = self.cursor_pos;
    }
}

use std::collections::HashSet;

use crate::hex::Point;

const WHEEL_DELTA_LIMIT: f64 = 200.0;
const WHEEL_DELTA_UNIT: f64 = 100.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Other(i16),
}

impl From<i16> for MouseButton {
    fn from(value: i16) -> Self {
        match value {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            n => MouseButton::Other(n),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Trigger {
    Key(String),
    Mouse(MouseButton),
    Wheel,
}

impl Trigger {
    fn key(name: &str) -> Self {
        Trigger::Key(name.to_lowercase())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    ZoomIn,
    ZoomOut,
    Deselect,
    Select,
    DragPan,
    Gather,
    ScrollZoom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gesture {
    Held,
    Press,
    Click,
    Drag,
    HeldPress,
    Wheel,
}

impl Action {
    fn gesture(self) -> Gesture {
        match self {
            Action::PanLeft
            | Action::PanRight
            | Action::PanUp
            | Action::PanDown
            | Action::ZoomIn
            | Action::ZoomOut => Gesture::Held,
            Action::Deselect => Gesture::Press,
            Action::Select => Gesture::Click,
            Action::DragPan => Gesture::Drag,
            Action::Gather => Gesture::HeldPress,
            Action::ScrollZoom => Gesture::Wheel,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InputConfig {
    pub bindings: Vec<(Trigger, Action)>,
    pub drag_threshold: f64,
    pub key_pan_speed: f64,
    pub key_zoom_rate: f64,
    pub wheel_zoom_base: f64,
}

impl Default for InputConfig {
    fn default() -> Self {
        let keys = |names: &[&str], action| {
            names
                .iter()
                .map(|n| (Trigger::key(n), action))
                .collect::<Vec<_>>()
        };
        let bindings = [
            keys(&["a", "arrowleft"], Action::PanLeft),
            keys(&["d", "arrowright"], Action::PanRight),
            keys(&["w", "arrowup"], Action::PanUp),
            keys(&["s", "arrowdown"], Action::PanDown),
            keys(&["=", "+"], Action::ZoomIn),
            keys(&["-", "_"], Action::ZoomOut),
            keys(&["escape"], Action::Deselect),
            vec![
                (Trigger::Mouse(MouseButton::Left), Action::Select),
                (Trigger::Mouse(MouseButton::Left), Action::DragPan),
                (Trigger::Mouse(MouseButton::Left), Action::Gather),
                (Trigger::Wheel, Action::ScrollZoom),
            ],
        ]
        .concat();
        InputConfig {
            bindings,
            drag_threshold: 4.0,
            key_pan_speed: 600.0,
            key_zoom_rate: 2.0,
            wheel_zoom_base: 1.1,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RawInput {
    KeyDown(String),
    KeyUp(String),
    MouseDown(MouseButton, Point),
    MouseUp(MouseButton, Point),
    MouseMove(Point),
    Wheel { delta_y: f64, at: Point },
    Blur,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Select(Point),
    Gather(Point, f64),
    Deselect,
    PanBy(Point),
    ZoomAt(Point, f64),
    ZoomAtCenter(f64),
}

struct Press {
    trigger: Trigger,
    start: Point,
    last: Point,
    dragging: bool,
}

pub struct Input {
    config: InputConfig,
    held: HashSet<Trigger>,
    press: Option<Press>,
}

impl Input {
    pub fn new(config: InputConfig) -> Self {
        Input {
            config,
            held: HashSet::new(),
            press: None,
        }
    }

    pub fn handle(&mut self, raw: RawInput) -> Vec<Command> {
        match raw {
            RawInput::KeyDown(key) => self.down(Trigger::key(&key), None),
            RawInput::KeyUp(key) => {
                self.held.remove(&Trigger::key(&key));
                Vec::new()
            }
            RawInput::MouseDown(button, p) => self.down(Trigger::Mouse(button), Some(p)),
            RawInput::MouseUp(button, p) => self.up(Trigger::Mouse(button), p),
            RawInput::MouseMove(p) => self.moved(p),
            RawInput::Wheel { delta_y, at } => self.wheel(delta_y, at),
            RawInput::Blur => {
                self.held.clear();
                self.press = None;
                Vec::new()
            }
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.press.as_ref().is_some_and(|p| p.dragging)
    }

    pub fn frame(&self, dt: f64) -> Vec<Command> {
        let active = |action| {
            self.config
                .bindings
                .iter()
                .any(|(t, a)| *a == action && self.held.contains(t))
        };
        let axis = |neg, pos| f64::from(i8::from(active(pos)) - i8::from(active(neg)));
        let pan = Point::new(
            axis(Action::PanRight, Action::PanLeft),
            axis(Action::PanDown, Action::PanUp),
        );
        let zoom = axis(Action::ZoomOut, Action::ZoomIn);
        let mut commands = Vec::new();
        if let Some(press) = &self.press
            && !press.dragging
            && self
                .actions(&press.trigger, Gesture::HeldPress)
                .next()
                .is_some()
        {
            commands.push(Command::Gather(press.start, dt));
        }
        if pan != Point::default() {
            commands.push(Command::PanBy(pan * (self.config.key_pan_speed * dt)));
        }
        if zoom != 0.0 {
            commands.push(Command::ZoomAtCenter(
                self.config.key_zoom_rate.powf(zoom * dt),
            ));
        }
        commands
    }

    fn actions<'a>(
        &'a self,
        trigger: &'a Trigger,
        gesture: Gesture,
    ) -> impl Iterator<Item = Action> + 'a {
        self.config
            .bindings
            .iter()
            .filter(move |(t, a)| t == trigger && a.gesture() == gesture)
            .map(|(_, a)| *a)
    }

    fn down(&mut self, trigger: Trigger, at: Option<Point>) -> Vec<Command> {
        if !self.held.insert(trigger.clone()) {
            return Vec::new();
        }
        let commands = self
            .actions(&trigger, Gesture::Press)
            .filter_map(|a| match a {
                Action::Deselect => Some(Command::Deselect),
                _ => None,
            })
            .collect();
        if let Some(p) = at
            && self.press.is_none()
            && (self.actions(&trigger, Gesture::Click).next().is_some()
                || self.actions(&trigger, Gesture::Drag).next().is_some()
                || self.actions(&trigger, Gesture::HeldPress).next().is_some())
        {
            self.press = Some(Press {
                trigger,
                start: p,
                last: p,
                dragging: false,
            });
        }
        commands
    }

    fn moved(&mut self, p: Point) -> Vec<Command> {
        let Some(press) = &self.press else {
            return Vec::new();
        };
        if self.actions(&press.trigger, Gesture::Drag).next().is_none() {
            return Vec::new();
        }
        let threshold = self.config.drag_threshold;
        let press = self.press.as_mut().expect("press checked above");
        let from_start = p - press.start;
        if !press.dragging && from_start.x.hypot(from_start.y) < threshold {
            return Vec::new();
        }
        press.dragging = true;
        let delta = p - press.last;
        press.last = p;
        vec![Command::PanBy(delta)]
    }

    fn up(&mut self, trigger: Trigger, p: Point) -> Vec<Command> {
        self.held.remove(&trigger);
        let Some(press) = self.press.take_if(|press| press.trigger == trigger) else {
            return Vec::new();
        };
        if press.dragging {
            return Vec::new();
        }
        self.actions(&trigger, Gesture::Click)
            .filter_map(|a| match a {
                Action::Select => Some(Command::Select(p)),
                _ => None,
            })
            .collect()
    }

    fn wheel(&self, delta_y: f64, at: Point) -> Vec<Command> {
        let delta = delta_y.clamp(-WHEEL_DELTA_LIMIT, WHEEL_DELTA_LIMIT);
        self.actions(&Trigger::Wheel, Gesture::Wheel)
            .map(|_| {
                Command::ZoomAt(
                    at,
                    self.config.wheel_zoom_base.powf(-delta / WHEEL_DELTA_UNIT),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> Input {
        Input::new(InputConfig::default())
    }

    fn key_down(k: &str) -> RawInput {
        RawInput::KeyDown(k.to_string())
    }

    fn key_up(k: &str) -> RawInput {
        RawInput::KeyUp(k.to_string())
    }

    #[test]
    fn click_selects() {
        let mut i = input();
        let p = Point::new(10.0, 20.0);
        assert!(
            i.handle(RawInput::MouseDown(MouseButton::Left, p))
                .is_empty()
        );
        assert_eq!(
            i.handle(RawInput::MouseUp(MouseButton::Left, p)),
            vec![Command::Select(p)]
        );
    }

    #[test]
    fn small_move_still_selects() {
        let mut i = input();
        let p = Point::new(10.0, 20.0);
        i.handle(RawInput::MouseDown(MouseButton::Left, p));
        assert!(
            i.handle(RawInput::MouseMove(Point::new(11.0, 21.0)))
                .is_empty()
        );
        let up = Point::new(12.0, 21.0);
        assert_eq!(
            i.handle(RawInput::MouseUp(MouseButton::Left, up)),
            vec![Command::Select(up)]
        );
    }

    #[test]
    fn drag_pans_and_does_not_select() {
        let mut i = input();
        i.handle(RawInput::MouseDown(MouseButton::Left, Point::new(0.0, 0.0)));
        let moved = i.handle(RawInput::MouseMove(Point::new(10.0, 0.0)));
        assert_eq!(moved, vec![Command::PanBy(Point::new(10.0, 0.0))]);
        let moved = i.handle(RawInput::MouseMove(Point::new(12.0, 3.0)));
        assert_eq!(moved, vec![Command::PanBy(Point::new(2.0, 3.0))]);
        assert!(
            i.handle(RawInput::MouseUp(MouseButton::Left, Point::new(12.0, 3.0)))
                .is_empty()
        );
    }

    #[test]
    fn holding_gathers_each_frame_and_release_still_selects() {
        let mut i = input();
        let p = Point::new(3.0, 4.0);
        i.handle(RawInput::MouseDown(MouseButton::Left, p));
        assert_eq!(i.frame(0.5), vec![Command::Gather(p, 0.5)]);
        assert_eq!(i.frame(0.25), vec![Command::Gather(p, 0.25)]);
        assert_eq!(
            i.handle(RawInput::MouseUp(MouseButton::Left, p)),
            vec![Command::Select(p)]
        );
        assert!(i.frame(0.5).is_empty());
    }

    #[test]
    fn dragging_stops_gathering() {
        let mut i = input();
        i.handle(RawInput::MouseDown(MouseButton::Left, Point::new(0.0, 0.0)));
        i.handle(RawInput::MouseMove(Point::new(20.0, 0.0)));
        assert!(i.frame(0.5).is_empty());
    }

    #[test]
    fn escape_deselects_once() {
        let mut i = input();
        assert_eq!(i.handle(key_down("Escape")), vec![Command::Deselect]);
        assert!(i.handle(key_down("Escape")).is_empty());
        i.handle(key_up("Escape"));
        assert_eq!(i.handle(key_down("Escape")), vec![Command::Deselect]);
    }

    #[test]
    fn redundant_keys_do_not_stack() {
        let mut i = input();
        i.handle(key_down("a"));
        i.handle(key_down("ArrowLeft"));
        assert_eq!(i.frame(1.0), vec![Command::PanBy(Point::new(600.0, 0.0))]);
    }

    #[test]
    fn opposing_keys_cancel() {
        let mut i = input();
        i.handle(key_down("a"));
        i.handle(key_down("d"));
        assert!(i.frame(1.0).is_empty());
    }

    #[test]
    fn zoom_keys_zoom_at_center() {
        let mut i = input();
        i.handle(key_down("="));
        assert_eq!(i.frame(1.0), vec![Command::ZoomAtCenter(2.0)]);
    }

    #[test]
    fn wheel_zooms_at_cursor() {
        let mut i = input();
        let at = Point::new(5.0, 6.0);
        let cmds = i.handle(RawInput::Wheel {
            delta_y: -100.0,
            at,
        });
        assert_eq!(cmds, vec![Command::ZoomAt(at, 1.1)]);
    }

    #[test]
    fn unbound_inputs_do_nothing() {
        let mut i = input();
        let p = Point::new(1.0, 1.0);
        assert!(i.handle(key_down("q")).is_empty());
        assert!(i.frame(1.0).is_empty());
        assert!(
            i.handle(RawInput::MouseDown(MouseButton::Right, p))
                .is_empty()
        );
        assert!(
            i.handle(RawInput::MouseUp(MouseButton::Right, p))
                .is_empty()
        );
    }

    #[test]
    fn blur_cancels_press_and_held_keys() {
        let mut i = input();
        let p = Point::new(1.0, 1.0);
        i.handle(key_down("a"));
        i.handle(RawInput::MouseDown(MouseButton::Left, p));
        i.handle(RawInput::Blur);
        assert!(i.frame(1.0).is_empty());
        assert!(i.handle(RawInput::MouseUp(MouseButton::Left, p)).is_empty());
    }
}

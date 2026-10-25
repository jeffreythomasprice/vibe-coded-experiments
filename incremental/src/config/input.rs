use crate::input::{Action, MouseButton, Trigger};

pub const WHEEL_DELTA_LIMIT: f64 = 200.0;
pub const WHEEL_DELTA_UNIT: f64 = 100.0;

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
                (Trigger::Mouse(MouseButton::Left), Action::Gather),
                (Trigger::Mouse(MouseButton::Left), Action::Select),
                (Trigger::Mouse(MouseButton::Left), Action::DragPan),
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

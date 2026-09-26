use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    CycleRenderScale,
    CycleRenderMode,
    Quit,
    ZoomIn,
    ZoomOut,
}

impl Action {
    pub const ALL: &'static [Action] = &[
        Action::MoveUp,
        Action::MoveDown,
        Action::MoveLeft,
        Action::MoveRight,
        Action::CycleRenderScale,
        Action::CycleRenderMode,
        Action::Quit,
        Action::ZoomIn,
        Action::ZoomOut,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::MoveUp => "Move Up",
            Action::MoveDown => "Move Down",
            Action::MoveLeft => "Move Left",
            Action::MoveRight => "Move Right",
            Action::CycleRenderScale => "Cycle Render Scale",
            Action::CycleRenderMode => "Cycle Render Mode",
            Action::Quit => "Quit",
            Action::ZoomIn => "Zoom In",
            Action::ZoomOut => "Zoom Out",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn all_covers_every_variant() {
        assert_eq!(Action::ALL.len(), 9);
    }

    #[test]
    fn labels_are_unique() {
        let labels: HashSet<_> = Action::ALL.iter().map(|a| a.label()).collect();
        assert_eq!(labels.len(), Action::ALL.len());
    }

    #[test]
    fn config_names_are_snake_case() {
        // `Action` is always serialized as a `Bindings` map key, never as a
        // bare top-level value (which `toml` can't represent), so this
        // exercises it the same way.
        let mut map = std::collections::BTreeMap::new();
        map.insert(Action::CycleRenderScale, 1);
        assert_eq!(
            toml::to_string(&map).unwrap().trim(),
            "cycle_render_scale = 1"
        );
    }
}

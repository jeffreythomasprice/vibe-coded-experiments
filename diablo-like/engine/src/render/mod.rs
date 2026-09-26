pub mod camera_ubo;
pub mod chunks;
pub mod composite;
pub mod crisp;
pub mod gpu;
pub mod offscreen;
pub mod streaming_fixture;
pub mod text_pipeline;

/// Which world-content pass(es) draw each frame. The exact contour stroke
/// (see `crisp::CrispPipeline::draw_stroke`) is drawn in every mode — it's
/// the cross-check between the baked field and the exact geometry, not
/// something exclusive to crisp mode.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RenderMode {
    Crisp,
    Glow,
    Both,
}

impl RenderMode {
    pub fn next(self) -> Self {
        match self {
            RenderMode::Crisp => RenderMode::Glow,
            RenderMode::Glow => RenderMode::Both,
            RenderMode::Both => RenderMode::Crisp,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RenderMode::Crisp => "CRISP",
            RenderMode::Glow => "GLOW",
            RenderMode::Both => "BOTH",
        }
    }
}

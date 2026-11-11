use std::collections::{BTreeSet, HashMap, HashSet};

use level_cmd::{ColorMode, scene_bounds};
use level_core::{Camera, Document, ObjectId};
use level_geom::Aabb;
use level_geom::camera::View;

use crate::color::BACKGROUND;
use crate::error::RenderError;
use crate::gpu::{DrawLists, DrawParams, Renderer};
use crate::overlay::{axes_lines, grid_lines};
use crate::scene::{
    EdgeStyle, HIGHLIGHT_COLOR, SELECTION_COLOR, Scene, SceneOptions, Shading, build_scene,
    decode_id,
};

/// The document's geometry turned into draw lists once, shared by every viewport drawn from it.
pub struct PreparedScene {
    scene: Scene,
    bounds: Option<Aabb>,
    reach: f64,
}

impl PreparedScene {
    pub fn new(
        doc: &Document,
        selection: &BTreeSet<ObjectId>,
        hover: Option<ObjectId>,
        hidden: &HashSet<ObjectId>,
    ) -> Self {
        let mut tint: HashMap<ObjectId, [f32; 3]> = HashMap::new();
        for &id in selection.iter().filter(|&&id| doc.contains(id)) {
            tint.insert(id, SELECTION_COLOR);
        }
        if let Some(id) = hover.filter(|&id| doc.contains(id)) {
            tint.entry(id).or_insert(HIGHLIGHT_COLOR);
        }
        let scene = build_scene(
            doc,
            &SceneOptions {
                shading: Shading::Lit,
                edges: EdgeStyle::Dark,
                draw_faces: true,
                tint: &tint,
                hidden,
                colors: ColorMode::Material,
                overrides: &HashMap::new(),
                want_ids: true,
            },
        );
        let bounds = scene_bounds(doc);
        Self {
            scene,
            reach: bounds.as_ref().map_or(64.0, |b| b.reach()),
            bounds,
        }
    }

    pub fn bounds(&self) -> Option<&Aabb> {
        self.bounds.as_ref()
    }

    pub fn view(&self, camera: &Camera, width: u32, height: u32) -> View {
        View::new(camera, width, height, self.reach)
    }

    fn params(&self, view: &View, width: u32, height: u32, clear: [f64; 3]) -> DrawParams {
        DrawParams {
            view_proj: view.view_proj().as_mat4().to_cols_array_2d(),
            width,
            height,
            clear,
            clip_z: 1.0e30,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ViewportOptions {
    /// Grid cell size to draw; `None` hides the grid.
    pub grid: Option<f64>,
    pub axes: bool,
}

/// Per-pixel object ids of one viewport, for picking.
pub struct PickMap {
    ids: Vec<u8>,
    width: u32,
    height: u32,
    objects: Vec<ObjectId>,
}

impl PickMap {
    pub fn at(&self, x: u32, y: u32) -> Option<ObjectId> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let code = decode_id([self.ids[i], self.ids[i + 1], self.ids[i + 2], 0]) as usize;
        code.checked_sub(1)
            .and_then(|k| self.objects.get(k))
            .copied()
    }
}

impl Renderer {
    /// Draws one viewport into `target` (`Rgba8Unorm`, `RENDER_ATTACHMENT`, sized `width` x `height`).
    pub fn draw_viewport(
        &self,
        prepared: &PreparedScene,
        view: &View,
        options: ViewportOptions,
        (width, height): (u32, u32),
        target: &wgpu::TextureView,
    ) {
        let mut lines = prepared.scene.edges.clone();
        if let Some(cell) = options.grid {
            lines.extend(grid_lines(view, cell, prepared.bounds.as_ref()));
        }
        if options.axes {
            lines.extend(axes_lines(view, prepared.reach));
        }
        let [r, g, b] = BACKGROUND.map(|c| f64::from(c) / 255.0);
        self.draw_to_view(
            &prepared.params(view, width, height, [r, g, b]),
            &DrawLists {
                tris: &prepared.scene.tris,
                lines: &lines,
                on_top: &prepared.scene.highlight_edges,
            },
            target,
        );
    }

    pub fn pick_map(
        &self,
        prepared: &PreparedScene,
        view: &View,
        (width, height): (u32, u32),
    ) -> Result<PickMap, RenderError> {
        let ids = self.draw_ids(
            &prepared.params(view, width, height, [0.0; 3]),
            &prepared.scene.id_tris,
        )?;
        Ok(PickMap {
            ids,
            width,
            height,
            objects: prepared.scene.drawn.iter().map(|d| d.id).collect(),
        })
    }
}

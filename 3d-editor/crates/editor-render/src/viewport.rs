use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use editor_cmd::links::EntityLink;
use editor_cmd::{ColorMode, scene_bounds};
use editor_core::{Camera, Document, EntityId, FaceId, ObjectId};
use editor_geom::Aabb;
use editor_geom::camera::View;
use glam::DAffine3;

use crate::color::BACKGROUND;
use crate::error::RenderError;
use crate::gpu::{DrawLists, DrawParams, Renderer};
use crate::overlay::{axes_lines, grid_lines, link_lines, pivot_lines};
use crate::scene::{
    ComponentLook, EdgeStyle, HIGHLIGHT_COLOR, SELECTION_COLOR, Scene, SceneOptions, Shading,
    TextureInputs, build_scene, decode_id,
};
use crate::scene::{FaceBatch, Vertex};
use crate::texture::TextureCache;

/// What a scene needs besides the document: where relative texture paths start, the shared image cache and the selected faces.
pub struct SceneEnv<'a> {
    pub scene_dir: Option<&'a Path>,
    pub textures: &'a TextureCache,
    pub faces: &'a BTreeSet<(ObjectId, FaceId)>,
    /// Marker looks of defined components.
    pub looks: &'a HashMap<String, ComponentLook>,
    /// Entity links to draw as arrows; empty draws none.
    pub links: &'a [EntityLink],
}

/// The document's geometry turned into draw lists once, shared by every viewport drawn from it.
pub struct PreparedScene {
    scene: Scene,
    link_lines: Vec<crate::scene::Vertex>,
    bounds: Option<Aabb>,
    reach: f64,
}

impl PreparedScene {
    pub fn new(
        doc: &Document,
        selection: &BTreeSet<ObjectId>,
        hover: Option<ObjectId>,
        hidden: &HashSet<ObjectId>,
        env: &SceneEnv,
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
                looks: env.looks,
                shading: Shading::Lit,
                edges: EdgeStyle::Dark,
                draw_faces: true,
                tint: &tint,
                hidden,
                colors: ColorMode::Material,
                overrides: &HashMap::new(),
                want_ids: true,
                textures: Some(TextureInputs {
                    cache: env.textures,
                    scene_dir: env.scene_dir,
                }),
                face_tint: env.faces,
                markers: true,
                split_owned: false,
            },
        );
        Self::finish(doc, scene, env, selection)
    }

    /// The scene as played: no markers, selection, hover or edge lines, hidden flags ignored. Entity-owned geometry is kept apart so `draw_viewport_posed` can move it.
    pub fn game(doc: &Document, excluded: &HashSet<ObjectId>, env: &SceneEnv) -> Self {
        let scene = build_scene(
            doc,
            &SceneOptions {
                looks: env.looks,
                shading: Shading::Lit,
                edges: EdgeStyle::None,
                draw_faces: true,
                tint: &HashMap::new(),
                hidden: excluded,
                colors: ColorMode::Material,
                overrides: &HashMap::new(),
                want_ids: false,
                textures: Some(TextureInputs {
                    cache: env.textures,
                    scene_dir: env.scene_dir,
                }),
                face_tint: &BTreeSet::new(),
                markers: false,
                split_owned: true,
            },
        );
        Self::finish(doc, scene, env, &BTreeSet::new())
    }

    fn finish(
        doc: &Document,
        scene: Scene,
        env: &SceneEnv,
        selection: &BTreeSet<ObjectId>,
    ) -> Self {
        let bounds = scene_bounds(doc);
        let mut link_lines = link_lines(doc, env.links, selection);
        link_lines.extend(pivot_lines(doc, selection));
        Self {
            link_lines,
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
        size: (u32, u32),
        target: &wgpu::TextureView,
    ) {
        self.draw_viewport_posed(prepared, view, options, size, target, &HashMap::new());
    }

    /// Like `draw_viewport`, with each listed entity's geometry moved by its transform (rest pose to current pose).
    pub fn draw_viewport_posed(
        &self,
        prepared: &PreparedScene,
        view: &View,
        options: ViewportOptions,
        (width, height): (u32, u32),
        target: &wgpu::TextureView,
        poses: &HashMap<EntityId, DAffine3>,
    ) {
        let posed = posed_batches(&prepared.scene.owned, poses);
        let mut lines = prepared.scene.edges.clone();
        if let Some(cell) = options.grid {
            lines.extend(grid_lines(view, cell, prepared.bounds.as_ref()));
        }
        if options.axes {
            lines.extend(axes_lines(view, prepared.reach));
        }
        let mut on_top = prepared.scene.highlight_edges.clone();
        on_top.extend(prepared.link_lines.iter().copied());
        let [r, g, b] = BACKGROUND.map(|c| f64::from(c) / 255.0);
        self.draw_to_view(
            &prepared.params(view, width, height, [r, g, b]),
            &DrawLists {
                faces: &prepared.scene.faces,
                extra_faces: &posed,
                lines: &lines,
                on_top: &on_top,
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

/// Copies of the owned batches with each owner's transform applied; owners without one keep their rest position.
pub(crate) fn posed_batches(
    owned: &[FaceBatch],
    poses: &HashMap<EntityId, DAffine3>,
) -> Vec<FaceBatch> {
    owned
        .iter()
        .map(|batch| {
            let m = batch.owner.and_then(|o| poses.get(&o));
            FaceBatch {
                material: batch.material.clone(),
                owner: batch.owner,
                verts: batch
                    .verts
                    .iter()
                    .map(|v| match m {
                        Some(m) => {
                            let p = m.transform_point3(glam::DVec3::new(
                                f64::from(v.pos[0]),
                                f64::from(v.pos[1]),
                                f64::from(v.pos[2]),
                            ));
                            Vertex {
                                pos: [p.x as f32, p.y as f32, p.z as f32],
                                ..*v
                            }
                        }
                        None => *v,
                    })
                    .collect(),
            }
        })
        .collect()
}

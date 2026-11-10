use std::collections::{BTreeSet, HashMap};
use std::io::Cursor;

use level_cmd::{
    CameraArg, CommandError, Mode, Overlay, ScreenshotArgs, build_camera, camera_json, parse_args,
    resolve_camera, scene_bounds,
};
use level_core::{CameraId, Document, ObjectId};
use level_geom::camera::{MAX_IMAGE_DIM, REFERENCE_HEIGHT, REFERENCE_WIDTH, View};
use serde_json::{Value, json};

use crate::color::BACKGROUND;
use crate::error::RenderError;
use crate::gpu::{DrawLists, DrawParams, Renderer};
use crate::label::{Text, draw_labels};
use crate::overlay::{axes_lines, bounds_lines, grid_lines};
use crate::scene::{
    EdgeStyle, HIGHLIGHT_COLOR, SELECTION_COLOR, SceneOptions, Shading, build_scene,
};

#[derive(Debug, Clone, PartialEq)]
pub struct LabelPos {
    pub id: ObjectId,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone)]
pub struct Shot {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub camera: Value,
    pub labels: Option<Vec<LabelPos>>,
    pub warnings: Vec<String>,
}

impl Shot {
    /// The JSON `result` of the `view.screenshot` response.
    pub fn result(&self) -> Value {
        let mut v = json!({ "width": self.width, "height": self.height, "camera": self.camera });
        if let Some(labels) = &self.labels {
            v["labels"] = labels
                .iter()
                .map(|l| json!({ "id": l.id, "x": l.x, "y": l.y }))
                .collect();
        }
        v
    }
}

fn dimension(field: &str, value: Option<u32>, default: u32) -> Result<u32, CommandError> {
    let v = value.unwrap_or(default);
    if (1..=MAX_IMAGE_DIM).contains(&v) {
        Ok(v)
    } else {
        Err(CommandError::invalid_arg(
            field,
            format!("{field} must be between 1 and {MAX_IMAGE_DIM}"),
        ))
    }
}

fn encode_png(rgba: Vec<u8>, width: u32, height: u32) -> Result<Vec<u8>, RenderError> {
    let image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| RenderError::Encode("pixel buffer has the wrong size".into()))?;
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| RenderError::Encode(e.to_string()))?;
    Ok(out.into_inner())
}

pub fn screenshot(renderer: &Renderer, doc: &Document, args: Value) -> Result<Shot, CommandError> {
    screenshot_with_selection(renderer, doc, &BTreeSet::new(), args)
}

pub fn screenshot_with_selection(
    renderer: &Renderer,
    doc: &Document,
    selection: &BTreeSet<ObjectId>,
    args: Value,
) -> Result<Shot, CommandError> {
    let args: ScreenshotArgs = parse_args(args)?;
    let width = dimension("width", args.width, REFERENCE_WIDTH)?;
    let height = dimension("height", args.height, REFERENCE_HEIGHT)?;

    let camera = match args.camera {
        None => resolve_camera(doc, "viewport.persp", width, height)?,
        Some(CameraArg::Reference(r)) => resolve_camera(doc, &r, width, height)?,
        Some(CameraArg::Inline(params)) => {
            build_camera(doc, "inline".into(), CameraId(0), &params)?
        }
    };
    let session = camera.id == CameraId(0);

    for &id in &args.highlight {
        if !doc.contains(id) {
            return Err(CommandError::not_found(id));
        }
    }
    let mut tint: HashMap<ObjectId, [f32; 3]> = HashMap::new();
    if args.overlays.contains(&Overlay::Selection) {
        for &id in selection.iter().filter(|&&id| doc.contains(id)) {
            tint.insert(id, SELECTION_COLOR);
        }
    }
    for &id in &args.highlight {
        tint.insert(id, HIGHLIGHT_COLOR);
    }

    let bounds = scene_bounds(doc);
    let reach = bounds.map_or(64.0, |b| b.reach());
    let view = View::new(&camera, width, height, reach);

    let (shading, edges, draw_faces) = match args.mode {
        Mode::Shaded => (Shading::Lit, EdgeStyle::Dark, true),
        Mode::Flat => (Shading::Flat, EdgeStyle::None, true),
        Mode::Wireframe => (Shading::Flat, EdgeStyle::Colored, false),
    };
    let scene = build_scene(
        doc,
        &SceneOptions {
            shading,
            edges,
            draw_faces,
            tint: &tint,
        },
    );

    let mut overlay_lines = Vec::new();
    if args.overlays.contains(&Overlay::Grid) {
        overlay_lines.extend(grid_lines(&view, doc.editor.grid, bounds.as_ref()));
    }
    if args.overlays.contains(&Overlay::Axes) {
        overlay_lines.extend(axes_lines(&view, reach));
    }
    if args.overlays.contains(&Overlay::Bounds) {
        overlay_lines.extend(bounds_lines(&scene.drawn));
    }
    let mut lines = scene.edges;
    lines.extend(overlay_lines);

    let [r, g, b] = BACKGROUND.map(|c| f64::from(c) / 255.0);
    let mut rgba = renderer.draw(
        &DrawParams {
            view_proj: view.view_proj().as_mat4().to_cols_array_2d(),
            width,
            height,
            clear: [r, g, b],
        },
        &DrawLists {
            tris: &scene.tris,
            lines: &lines,
            on_top: &scene.highlight_edges,
        },
    )?;

    let mut positions = Vec::new();
    for d in &scene.drawn {
        let Some(p) = view.project(d.bounds.center()) else {
            continue;
        };
        let inside = (0.0..width as f64).contains(&p.px.x)
            && (0.0..height as f64).contains(&p.px.y)
            && (0.0..=1.0).contains(&p.depth);
        if inside {
            positions.push(LabelPos {
                id: d.id,
                x: p.px.x as u32,
                y: p.px.y as u32,
            });
        }
    }
    let want_labels = args.overlays.contains(&Overlay::Labels);
    if want_labels {
        let texts: Vec<Text> = positions
            .iter()
            .map(|l| Text {
                text: l.id.to_string(),
                x: l.x as f32,
                y: l.y as f32,
            })
            .collect();
        draw_labels(&mut rgba, width, height, &texts);
    }

    let mut cam = camera_json(&camera, session);
    if let Some(o) = cam.as_object_mut() {
        o.remove("session");
    }
    Ok(Shot {
        png: encode_png(rgba, width, height)?,
        width,
        height,
        camera: cam,
        labels: want_labels.then_some(positions),
        warnings: scene.warnings,
    })
}

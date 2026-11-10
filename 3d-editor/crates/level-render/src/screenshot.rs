use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Cursor;

use level_cmd::descendants;
use level_cmd::{
    CameraArg, ColorMode, CommandError, Mode, Overlay, ScreenshotArgs, build_camera, camera_json,
    parse_args, resolve_camera, scene_bounds,
};
use level_core::{CameraId, Document, ObjectId, ObjectKind};
use level_geom::camera::{MAX_IMAGE_DIM, REFERENCE_HEIGHT, REFERENCE_WIDTH, View};
use serde_json::{Value, json};

use crate::color::BACKGROUND;
use crate::error::RenderError;
use crate::gpu::{DrawLists, DrawParams, Renderer};
use crate::label::{Text, draw_labels, text_size};
use crate::overlay::{axes_lines, bounds_lines, grid_lines};
use crate::scene::{
    EdgeStyle, HIGHLIGHT_COLOR, SELECTION_COLOR, SceneOptions, Shading, build_scene, decode_id,
};

#[derive(Debug, Clone, PartialEq)]
pub struct LabelPos {
    pub id: ObjectId,
    pub x: u32,
    pub y: u32,
    pub pixels: u32,
}

#[derive(Debug, Clone)]
pub struct Shot {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub camera: Value,
    pub labels: Option<Vec<LabelPos>>,
    pub labels_omitted: usize,
    pub legend: Value,
    pub warnings: Vec<String>,
}

impl Shot {
    /// The JSON `result` of the `view.screenshot` response.
    pub fn result(&self) -> Value {
        let mut v = json!({ "width": self.width, "height": self.height, "camera": self.camera });
        if let Some(labels) = &self.labels {
            v["labels"] = labels
                .iter()
                .map(|l| json!({ "id": l.id, "x": l.x, "y": l.y, "pixels": l.pixels }))
                .collect();
            v["labels_drawn"] = json!(labels.len());
            v["labels_omitted"] = json!(self.labels_omitted);
        }
        v["legend"] = self.legend.clone();
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

const DEFAULT_MIN_LABEL_PIXELS: u32 = 12;
const DEFAULT_MAX_LABELS: usize = 40;

fn expand(doc: &Document, ids: &[ObjectId]) -> Result<HashSet<ObjectId>, CommandError> {
    let mut out = BTreeSet::new();
    for &id in ids {
        if !doc.contains(id) {
            return Err(CommandError::not_found(id));
        }
        descendants(doc, id, &mut out);
        if matches!(id, ObjectId::Layer(_)) {
            for child in doc.children(id) {
                descendants(doc, child, &mut out);
            }
        }
    }
    Ok(out.into_iter().collect())
}

fn parse_hex(name: &str, text: &str) -> Result<[u8; 3], CommandError> {
    let bad = || {
        CommandError::invalid_arg(
            "material_colors",
            format!("`{text}` for `{name}` is not a colour"),
        )
        .with_hint("use \"#rrggbb\"")
    };
    let digits = text.strip_prefix('#').ok_or_else(bad)?;
    if digits.len() != 6 || !digits.is_ascii() {
        return Err(bad());
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).map_err(|_| bad());
    Ok([byte(0)?, byte(2)?, byte(4)?])
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

struct Candidate {
    id: ObjectId,
    pixels: u32,
    x: i32,
    y: i32,
}

fn visible_objects(ids: &[u8], width: u32, drawn: &[crate::scene::Drawn]) -> Vec<Candidate> {
    let w = width as usize;
    let mut stats = vec![(0u32, 0u64, 0u64); drawn.len()];
    for (i, px) in ids.as_chunks::<4>().0.iter().enumerate() {
        let code = decode_id(*px) as usize;
        if code == 0 || code > drawn.len() {
            continue;
        }
        let s = &mut stats[code - 1];
        s.0 += 1;
        s.1 += (i % w) as u64;
        s.2 += (i / w) as u64;
    }
    let mut best: Vec<Option<(i64, i32, i32)>> = vec![None; drawn.len()];
    for (i, px) in ids.as_chunks::<4>().0.iter().enumerate() {
        let code = decode_id(*px) as usize;
        if code == 0 || code > drawn.len() {
            continue;
        }
        let (n, sx, sy) = stats[code - 1];
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        let (cx, cy) = (sx as i64 / i64::from(n), sy as i64 / i64::from(n));
        let d = (x - cx).pow(2) + (y - cy).pow(2);
        if best[code - 1].is_none_or(|(bd, _, _)| d < bd) {
            best[code - 1] = Some((d, x as i32, y as i32));
        }
    }
    drawn
        .iter()
        .zip(stats)
        .zip(best)
        .filter_map(|((d, (pixels, _, _)), best)| {
            let (_, x, y) = best?;
            Some(Candidate {
                id: d.id,
                pixels,
                x,
                y,
            })
        })
        .collect()
}

fn place_labels(
    mut candidates: Vec<Candidate>,
    width: u32,
    height: u32,
    max: usize,
) -> (Vec<LabelPos>, Vec<Text>, usize) {
    candidates.sort_by(|a, b| b.pixels.cmp(&a.pixels).then(a.id.cmp(&b.id)));
    let (w, h) = (width as i32, height as i32);
    let mut rects: Vec<(i32, i32, i32, i32)> = Vec::new();
    let (mut labels, mut texts, mut omitted) = (Vec::new(), Vec::new(), 0);
    for c in candidates {
        if labels.len() >= max {
            omitted += 1;
            continue;
        }
        let text = c.id.to_string();
        let (bw, bh) = text_size(&text);
        let spot = [(0, 0), (0, -bh), (0, bh), (bw, 0), (-bw, 0)]
            .into_iter()
            .find_map(|(dx, dy)| {
                let (cx, cy) = (c.x + dx, c.y + dy);
                let x0 = (cx - bw / 2).clamp(0, (w - bw).max(0));
                let y0 = (cy - bh / 2).clamp(0, (h - bh).max(0));
                let free = rects.iter().all(|&(rx, ry, rw, rh)| {
                    x0 + bw <= rx || rx + rw <= x0 || y0 + bh <= ry || ry + rh <= y0
                });
                free.then_some((cx, cy, x0, y0))
            });
        let Some((cx, cy, x0, y0)) = spot else {
            omitted += 1;
            continue;
        };
        rects.push((x0, y0, bw, bh));
        texts.push(Text {
            text,
            x: cx as f32,
            y: cy as f32,
            anchor: (cx != c.x || cy != c.y).then_some((c.x as f32, c.y as f32)),
        });
        labels.push(LabelPos {
            id: c.id,
            x: c.x as u32,
            y: c.y as u32,
            pixels: c.pixels,
        });
    }
    (labels, texts, omitted)
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

    let hidden = expand(doc, &args.hide)?;
    let label_filter = expand(doc, &args.label_ids)?;
    if let Some(bad) = args
        .label_kinds
        .iter()
        .find(|k| !matches!(k, ObjectKind::Brush | ObjectKind::Mesh | ObjectKind::Entity))
    {
        return Err(CommandError::invalid_arg(
            "label_kinds",
            format!("{bad:?} is never labelled; use brush, mesh or entity"),
        ));
    }
    let min_label_pixels = match args.min_label_pixels {
        Some(0) => {
            return Err(CommandError::invalid_arg(
                "min_label_pixels",
                "min_label_pixels must be at least 1",
            ));
        }
        Some(n) => n,
        None => DEFAULT_MIN_LABEL_PIXELS,
    };
    let overrides: HashMap<String, [u8; 3]> = args
        .material_colors
        .iter()
        .map(|(name, text)| parse_hex(name, text).map(|rgb| (name.clone(), rgb)))
        .collect::<Result<_, _>>()?;
    let want_labels = args.overlays.contains(&Overlay::Labels);
    let clip_z = args.clip_above.map_or(1.0e30, |z| z as f32);

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
            hidden: &hidden,
            colors: args.colors,
            overrides: &overrides,
            want_ids: want_labels,
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
            clip_z,
        },
        &DrawLists {
            tris: &scene.tris,
            lines: &lines,
            on_top: &scene.highlight_edges,
        },
    )?;

    let (positions, omitted) = if want_labels {
        let ids = renderer.draw_ids(
            &DrawParams {
                view_proj: view.view_proj().as_mat4().to_cols_array_2d(),
                width,
                height,
                clear: [0.0, 0.0, 0.0],
                clip_z,
            },
            &scene.id_tris,
        )?;
        let candidates: Vec<Candidate> = visible_objects(&ids, width, &scene.drawn)
            .into_iter()
            .filter(|c| c.pixels >= min_label_pixels)
            .filter(|c| args.label_kinds.is_empty() || args.label_kinds.contains(&c.id.kind()))
            .filter(|c| label_filter.is_empty() || label_filter.contains(&c.id))
            .collect();
        let (labels, texts, omitted) = place_labels(
            candidates,
            width,
            height,
            args.max_labels.unwrap_or(DEFAULT_MAX_LABELS),
        );
        draw_labels(&mut rgba, width, height, &texts);
        (labels, omitted)
    } else {
        (Vec::new(), 0)
    };

    let mode_name = match args.colors {
        ColorMode::Material => "material",
        ColorMode::Orientation => "orientation",
        ColorMode::Layer => "layer",
    };
    let legend = json!({
        "mode": mode_name,
        "background": hex(BACKGROUND),
        "faces": scene.legend_faces.iter().map(|(name, rgb)| json!({
            "name": if name.is_empty() { "(no material)" } else { name.as_str() },
            "color": hex(*rgb),
        })).collect::<Vec<_>>(),
        "entities": scene.legend_entities.iter().map(|(class, (rgb, marker))| json!({
            "classname": class, "color": hex(*rgb), "marker": marker,
        })).collect::<Vec<_>>(),
    });

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
        labels_omitted: omitted,
        legend,
        warnings: scene.warnings,
    })
}

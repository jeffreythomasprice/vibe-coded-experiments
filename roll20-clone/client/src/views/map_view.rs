//! The canvas map view: renders a map, handles pan/zoom, the tool palette,
//! selection, and rectangle creation. The right sidebar is the inspector.

use leptos::ev;
use leptos::html::Canvas;
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use shared::{CreateShapeRequest, Geometry, Map, Style};
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlCanvasElement;

use super::inspector::SelectionInspector;
use crate::api;
use crate::camera::{Camera, GRID_PX};
use crate::render::{self, SelId};
use crate::ws;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Rectangle,
}

/// An in-progress mouse interaction.
#[derive(Clone, Copy)]
enum Drag {
    /// Middle-button pan; tracks the last client position.
    Pan { last_x: f64, last_y: f64 },
    /// Rectangle tool; start/current corners in grid units.
    Rect { start: (f64, f64), current: (f64, f64) },
}

fn default_shape_style() -> Style {
    Style {
        line_color: "#e0e0e0".to_string(),
        line_width: 2.0,
        background_color: "#3a6ea5".to_string(),
    }
}

/// Normalize two corners (grid units) to `(x, y, w, h)` with positive size.
fn norm_rect((x0, y0): (f64, f64), (x1, y1): (f64, f64)) -> (f64, f64, f64, f64) {
    (x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs())
}

/// Mouse position relative to the canvas, in CSS pixels.
fn local_pos(canvas: &HtmlCanvasElement, client_x: f64, client_y: f64) -> (f64, f64) {
    let r = canvas.get_bounding_client_rect();
    (client_x - r.left(), client_y - r.top())
}

#[component]
pub fn MapView() -> impl IntoView {
    let params = use_params_map();
    let map_id = params.read_untracked().get("id").unwrap_or_default();

    let map = RwSignal::new(Option::<Map>::None);
    let camera = RwSignal::new(Camera::default());
    let tool = RwSignal::new(Tool::Select);
    let selection = RwSignal::new(Vec::<SelId>::new());
    let drag = RwSignal::new(Option::<Drag>::None);
    let redraw_tick = RwSignal::new(0u32);
    let canvas_ref = NodeRef::<Canvas>::new();

    // Initial load + live follow.
    {
        let id = map_id.clone();
        spawn_local(async move {
            match api::get_map(&id).await {
                Ok(m) => map.set(Some(m)),
                Err(e) => tracing::error!(error = %e, "failed to load map"),
            }
        });
    }
    ws::follow_map(map_id.clone(), move |m| map.set(Some(m)));

    // Clamp the camera against the current map + viewport size.
    let clamp_camera = move || {
        if let (Some(canvas), Some(m)) = (canvas_ref.get_untracked(), map.get_untracked()) {
            let vw = canvas.client_width() as f64;
            let vh = canvas.client_height() as f64;
            let mw = m.width as f64 * GRID_PX;
            let mh = m.height as f64 * GRID_PX;
            camera.update(|c| c.clamp(mw, mh, vw, vh));
        }
    };

    // Redraw whenever any visual input changes.
    Effect::new(move |_| {
        let cam = camera.get();
        let sel = selection.get();
        let d = drag.get();
        redraw_tick.track();
        if let (Some(m), Some(canvas)) = (map.get(), canvas_ref.get()) {
            let preview = match d {
                Some(Drag::Rect { start, current }) => Some(norm_rect(start, current)),
                _ => None,
            };
            render::draw(&canvas, &m, &cam, &sel, preview);
        }
    });

    // Keyboard pan/zoom.
    let kb = window_event_listener(ev::keydown, move |ev: web_sys::KeyboardEvent| {
        let step = 60.0;
        let mut handled = true;
        match ev.key().as_str() {
            "ArrowLeft" => camera.update(|c| c.ox -= step / c.zoom),
            "ArrowRight" => camera.update(|c| c.ox += step / c.zoom),
            "ArrowUp" => camera.update(|c| c.oy -= step / c.zoom),
            "ArrowDown" => camera.update(|c| c.oy += step / c.zoom),
            "+" | "=" => zoom_center(camera, canvas_ref, 1.15),
            "-" | "_" => zoom_center(camera, canvas_ref, 1.0 / 1.15),
            _ => handled = false,
        }
        if handled {
            ev.prevent_default();
            clamp_camera();
        }
    });
    on_cleanup(move || kb.remove());

    // Redraw on window resize.
    let rz = window_event_listener(ev::resize, move |_| redraw_tick.update(|n| *n += 1));
    on_cleanup(move || rz.remove());

    // --- mouse handlers ---
    let on_mousedown = move |ev: web_sys::MouseEvent| {
        let Some(canvas) = canvas_ref.get_untracked() else { return };
        let (sx, sy) = local_pos(&canvas, ev.client_x() as f64, ev.client_y() as f64);

        // Middle button: start panning.
        if ev.button() == 1 {
            ev.prevent_default();
            drag.set(Some(Drag::Pan {
                last_x: ev.client_x() as f64,
                last_y: ev.client_y() as f64,
            }));
            return;
        }
        if ev.button() != 0 {
            return;
        }

        let cam = camera.get_untracked();
        let (wx, wy) = cam.screen_to_world(sx, sy);
        let (gx, gy) = (wx / GRID_PX, wy / GRID_PX);

        match tool.get_untracked() {
            Tool::Select => {
                let hit = map.get_untracked().and_then(|m| render::hit_test(&m, gx, gy));
                selection.update(|sel| match hit {
                    Some(h) => {
                        if ev.shift_key() {
                            if let Some(pos) = sel.iter().position(|x| *x == h) {
                                sel.remove(pos);
                            } else {
                                sel.push(h);
                            }
                        } else {
                            *sel = vec![h];
                        }
                    }
                    None => {
                        if !ev.shift_key() {
                            sel.clear();
                        }
                    }
                });
            }
            Tool::Rectangle => {
                drag.set(Some(Drag::Rect {
                    start: (gx, gy),
                    current: (gx, gy),
                }));
            }
        }
    };

    let on_mousemove = move |ev: web_sys::MouseEvent| {
        match drag.get_untracked() {
            Some(Drag::Pan { last_x, last_y }) => {
                let dx = ev.client_x() as f64 - last_x;
                let dy = ev.client_y() as f64 - last_y;
                camera.update(|c| c.pan_screen(dx, dy));
                clamp_camera();
                drag.set(Some(Drag::Pan {
                    last_x: ev.client_x() as f64,
                    last_y: ev.client_y() as f64,
                }));
            }
            Some(Drag::Rect { start, .. }) => {
                let Some(canvas) = canvas_ref.get_untracked() else { return };
                let (sx, sy) = local_pos(&canvas, ev.client_x() as f64, ev.client_y() as f64);
                let cam = camera.get_untracked();
                let (wx, wy) = cam.screen_to_world(sx, sy);
                drag.set(Some(Drag::Rect {
                    start,
                    current: (wx / GRID_PX, wy / GRID_PX),
                }));
            }
            None => {}
        }
    };

    let on_mouseup = move |_ev: web_sys::MouseEvent| {
        if let Some(Drag::Rect { start, current }) = drag.get_untracked() {
            let (x, y, w, h) = norm_rect(start, current);
            if w > 0.05 && h > 0.05 {
                if let Some(m) = map.get_untracked() {
                    let map_id = m.id.clone();
                    spawn_local(async move {
                        let req = CreateShapeRequest {
                            geometry: Geometry::Rect { x, y, w, h },
                            style: default_shape_style(),
                        };
                        match api::add_shape(&map_id, &req).await {
                            Ok(updated) => map.set(Some(updated)),
                            Err(e) => tracing::error!(error = %e, "failed to add shape"),
                        }
                    });
                }
            }
        }
        drag.set(None);
    };

    let on_wheel = move |ev: web_sys::WheelEvent| {
        ev.prevent_default();
        let Some(canvas) = canvas_ref.get_untracked() else { return };
        let (sx, sy) = local_pos(&canvas, ev.client_x() as f64, ev.client_y() as f64);
        let factor = if ev.delta_y() < 0.0 { 1.1 } else { 1.0 / 1.1 };
        camera.update(|c| c.zoom_at(factor, sx, sy));
        clamp_camera();
    };

    view! {
        <div class="map-view">
            <nav class="toolbar">
                <button
                    class:active=move || tool.get() == Tool::Select
                    title="Select"
                    on:click=move |_| tool.set(Tool::Select)
                >"▲"</button>
                <button
                    class:active=move || tool.get() == Tool::Rectangle
                    title="Rectangle"
                    on:click=move |_| tool.set(Tool::Rectangle)
                >"▢"</button>
            </nav>

            <div class="canvas-wrap">
                <canvas
                    node_ref=canvas_ref
                    class="map-canvas"
                    on:mousedown=on_mousedown
                    on:mousemove=on_mousemove
                    on:mouseup=on_mouseup
                    on:mouseleave=on_mouseup
                    on:wheel=on_wheel
                    on:contextmenu=move |ev: web_sys::MouseEvent| ev.prevent_default()
                ></canvas>
                {move || map.get().is_none().then(|| view! { <div class="overlay">"Loading map…"</div> })}
            </div>

            <SelectionInspector map=map selection=selection/>
        </div>
    }
}

/// Zoom keeping the viewport center fixed.
fn zoom_center(camera: RwSignal<Camera>, canvas_ref: NodeRef<Canvas>, factor: f64) {
    if let Some(canvas) = canvas_ref.get_untracked() {
        let cx = canvas.client_width() as f64 / 2.0;
        let cy = canvas.client_height() as f64 / 2.0;
        camera.update(|c| c.zoom_at(factor, cx, cy));
    }
}

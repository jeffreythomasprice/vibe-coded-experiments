//! DOM canvas setup and sizing.

use gloo::utils::{document, window};
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// Looks up a canvas by element id and returns it with its 2d context.
pub fn canvas_2d(id: &str) -> Result<(HtmlCanvasElement, CanvasRenderingContext2d), JsValue> {
    let canvas: HtmlCanvasElement = document()
        .get_element_by_id(id)
        .ok_or_else(|| JsValue::from_str("canvas element not found"))?
        .dyn_into()?;
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("2d context unavailable"))?
        .dyn_into()?;
    Ok((canvas, ctx))
}

/// An offscreen canvas holding the scene at art-pixel resolution.
pub fn offscreen_2d() -> Result<(HtmlCanvasElement, CanvasRenderingContext2d), JsValue> {
    let canvas: HtmlCanvasElement = document().create_element("canvas")?.dyn_into()?;
    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("2d context unavailable"))?
        .dyn_into()?;
    Ok((canvas, ctx))
}

/// Matches the canvas to the window's resolution.
pub fn fit_to_window(canvas: &HtmlCanvasElement, ctx: &CanvasRenderingContext2d) {
    // CSS pixels times the pixel ratio gives physical pixels, which keeps
    // high-DPI and zoomed displays sharp.
    let ratio = window().device_pixel_ratio();
    let width = (f64::from(canvas.client_width()) * ratio).round() as u32;
    let height = (f64::from(canvas.client_height()) * ratio).round() as u32;

    if (canvas.width(), canvas.height()) != (width, height) {
        // Setting the size clears the canvas and resets all context state.
        canvas.set_width(width);
        canvas.set_height(height);
        log::debug!("canvas is now {width}x{height}");
    }
    ctx.set_image_smoothing_enabled(false);
}

/// Size of one art pixel in canvas pixels, and the scene size in art pixels
/// needed to cover the canvas.
pub fn art_scale(width: u32, height: u32) -> (u32, i32, i32) {
    let scale = (width / 160).min(height / 90).max(1);
    let cover = |side: u32| side.div_ceil(scale).max(1) as i32;
    (scale, cover(width), cover(height))
}

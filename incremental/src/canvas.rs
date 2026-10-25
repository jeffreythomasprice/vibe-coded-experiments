use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::error::AppError;

pub struct Canvas {
    element: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    width: f64,
    height: f64,
    dpr: f64,
}

impl Canvas {
    pub fn from_element_id(id: &'static str) -> Result<Self, AppError> {
        let document = web_sys::window()
            .ok_or(AppError::NoWindow)?
            .document()
            .ok_or(AppError::NoDocument)?;
        let element = document
            .get_element_by_id(id)
            .ok_or(AppError::ElementNotFound(id))?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| AppError::NotACanvas(id))?;
        let ctx = element
            .get_context("2d")?
            .ok_or(AppError::NoContext2d)?
            .dyn_into::<CanvasRenderingContext2d>()
            .map_err(|_| AppError::NoContext2d)?;
        let mut canvas = Self {
            element,
            ctx,
            width: 0.0,
            height: 0.0,
            dpr: 1.0,
        };
        canvas.resize_to_window()?;
        Ok(canvas)
    }

    pub fn ctx(&self) -> &CanvasRenderingContext2d {
        &self.ctx
    }

    pub fn dpr(&self) -> f64 {
        self.dpr
    }

    pub fn width(&self) -> f64 {
        self.width
    }

    pub fn height(&self) -> f64 {
        self.height
    }

    pub fn resize_to_window(&mut self) -> Result<(), AppError> {
        let window = web_sys::window().ok_or(AppError::NoWindow)?;
        let dpr = window.device_pixel_ratio();
        self.dpr = dpr;
        self.width = window.inner_width()?.as_f64().unwrap_or(0.0);
        self.height = window.inner_height()?.as_f64().unwrap_or(0.0);
        self.element.set_width((self.width * dpr).round() as u32);
        self.element.set_height((self.height * dpr).round() as u32);
        self.ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0)?;
        log::debug!(
            "canvas resized to {}x{} at dpr {dpr}",
            self.width,
            self.height
        );
        Ok(())
    }
}

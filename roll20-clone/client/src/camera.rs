//! World <-> screen coordinate transform with pan/zoom clamping.
//!
//! "World" coordinates are in pixels: one grid unit is [`GRID_PX`] world pixels.
//! Shape geometry is in grid units and multiplied by `GRID_PX` to reach world
//! space. The camera maps world space to on-screen CSS pixels.

/// World pixels per grid unit (square) at zoom 1.
pub const GRID_PX: f64 = 50.0;

pub const ZOOM_MIN: f64 = 0.15;
pub const ZOOM_MAX: f64 = 8.0;

/// How much of the map (in world px) must remain on-screen at every edge, so
/// the user can never scroll the map entirely out of view.
const MARGIN: f64 = GRID_PX;

/// The view transform. `(ox, oy)` is the world-space coordinate shown at the
/// viewport's top-left corner; `zoom` is screen px per world px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub ox: f64,
    pub oy: f64,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            ox: -MARGIN,
            oy: -MARGIN,
            zoom: 1.0,
        }
    }
}

impl Camera {
    pub fn world_to_screen(&self, wx: f64, wy: f64) -> (f64, f64) {
        ((wx - self.ox) * self.zoom, (wy - self.oy) * self.zoom)
    }

    pub fn screen_to_world(&self, sx: f64, sy: f64) -> (f64, f64) {
        (self.ox + sx / self.zoom, self.oy + sy / self.zoom)
    }

    /// Pan by a screen-space delta (e.g. mouse drag), in CSS px.
    pub fn pan_screen(&mut self, dx: f64, dy: f64) {
        self.ox -= dx / self.zoom;
        self.oy -= dy / self.zoom;
    }

    /// Zoom by `factor`, keeping the world point under `(anchor_x, anchor_y)`
    /// (screen px) fixed on screen.
    pub fn zoom_at(&mut self, factor: f64, anchor_x: f64, anchor_y: f64) {
        let (wx, wy) = self.screen_to_world(anchor_x, anchor_y);
        self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        // Re-anchor so the same world point stays under the cursor.
        self.ox = wx - anchor_x / self.zoom;
        self.oy = wy - anchor_y / self.zoom;
    }

    /// Clamp the pan offset so at least `MARGIN` world px of the map stays
    /// visible on every side. `map_w`/`map_h` are the map extent in world px;
    /// `view_w`/`view_h` are the viewport size in CSS px.
    pub fn clamp(&mut self, map_w: f64, map_h: f64, view_w: f64, view_h: f64) {
        self.ox = clamp_axis(self.ox, map_w, view_w / self.zoom);
        self.oy = clamp_axis(self.oy, map_h, view_h / self.zoom);
    }
}

/// Clamp one axis. `extent` is the map size, `view` the visible world span.
fn clamp_axis(o: f64, extent: f64, view: f64) -> f64 {
    // Top-left may go as far negative as `MARGIN - view` (map's right/bottom
    // edge still `MARGIN` inside the view) and as far positive as
    // `extent - MARGIN` (map's left/top edge `MARGIN` inside the view).
    let lo = MARGIN - view;
    let hi = extent - MARGIN;
    if lo <= hi {
        o.clamp(lo, hi)
    } else {
        // Map smaller than the viewport: center it.
        (extent - view) / 2.0
    }
}

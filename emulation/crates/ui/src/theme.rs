//! The look of the UI: the four-shade palette and the pixel grid everything is
//! laid out on.
//!
//! All sizes in this crate are expressed in **UI pixels**, a whole number of
//! physical pixels chosen from the window size. That keeps borders, cursors and
//! glyphs on one consistent grid at any window size or display scale factor.

use egui::{Color32, FontFamily, FontId, Rounding, Stroke, TextStyle, Vec2};

/// The classic DMG green shades, lightest (index 0) to darkest (index 3), as
/// `[r, g, b]`. Public so a host can color the emulated screen to match.
pub const DMG_GREEN: [[u8; 3]; 4] = [
    [0x9b, 0xbc, 0x0f],
    [0x8b, 0xac, 0x0f],
    [0x30, 0x62, 0x30],
    [0x0f, 0x38, 0x0f],
];

/// The smallest virtual screen, in UI pixels, the layout is designed for: twice
/// the Game Boy's resolution, since at 160x144 the bindings table can't fit.
const VIRTUAL_SIZE: Vec2 = Vec2::new(320.0, 288.0);

/// The font size, in UI pixels, at which one font pixel is one UI pixel. The
/// embedded font is drawn on a grid of 128 units, and egui sizes a font by its
/// ascent-to-descent height (1280 units).
const FONT_SIZE: f32 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub lightest: Color32,
    pub light: Color32,
    pub dark: Color32,
    pub darkest: Color32,
}

impl Palette {
    pub fn from_shades(shades: [[u8; 3]; 4]) -> Palette {
        let [lightest, light, dark, darkest] = shades.map(|[r, g, b]| Color32::from_rgb(r, g, b));
        Palette {
            lightest,
            light,
            dark,
            darkest,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub palette: Palette,
    /// The size of one UI pixel, in egui points.
    pub pixel: f32,
}

impl Theme {
    /// The DMG theme scaled for a screen of `screen` points.
    pub fn dmg(screen: Vec2, pixels_per_point: f32) -> Theme {
        let physical = screen * pixels_per_point;
        let scale = (physical.x / VIRTUAL_SIZE.x)
            .min(physical.y / VIRTUAL_SIZE.y)
            .floor()
            .max(1.0);
        Theme {
            palette: Palette::from_shades(DMG_GREEN),
            pixel: scale / pixels_per_point,
        }
    }

    /// Convert a length in UI pixels to points.
    pub fn px(&self, ui_pixels: f32) -> f32 {
        ui_pixels * self.pixel
    }

    /// Round a length in points down to the UI pixel grid.
    pub fn snap(&self, points: f32) -> f32 {
        (points / self.pixel).floor() * self.pixel
    }

    pub fn font(&self) -> FontId {
        FontId::new(self.px(FONT_SIZE), FontFamily::Proportional)
    }

    /// Install this theme as `ctx`'s global style, so that stock egui widgets
    /// match the custom ones. Cheap when the theme hasn't changed.
    pub fn apply(&self, ctx: &egui::Context) {
        let id = egui::Id::new("ui-applied-theme");
        if ctx.data(|d| d.get_temp::<Theme>(id)) == Some(*self) {
            return;
        }
        ctx.data_mut(|d| d.insert_temp(id, *self));
        // Anti-aliased edges would blur the pixel grid.
        ctx.tessellation_options_mut(|options| options.feathering = false);
        ctx.all_styles_mut(|style| self.restyle(style));
    }

    fn restyle(&self, style: &mut egui::Style) {
        let Palette {
            lightest,
            light,
            dark,
            darkest,
        } = self.palette;

        for text_style in [
            TextStyle::Small,
            TextStyle::Body,
            TextStyle::Button,
            TextStyle::Heading,
            TextStyle::Monospace,
        ] {
            style.text_styles.insert(text_style, self.font());
        }
        style.animation_time = 0.0;
        style.interaction.selectable_labels = false;
        style.spacing.item_spacing = Vec2::splat(self.px(2.0));
        style.spacing.button_padding = Vec2::splat(self.px(2.0));
        style.spacing.window_margin = egui::Margin::same(self.px(6.0));

        let visuals = &mut style.visuals;
        visuals.dark_mode = false;
        visuals.override_text_color = Some(darkest);
        visuals.window_fill = lightest;
        visuals.panel_fill = lightest;
        visuals.window_stroke = Stroke::new(self.px(2.0), darkest);
        visuals.window_rounding = Rounding::ZERO;
        visuals.menu_rounding = Rounding::ZERO;
        visuals.window_shadow = egui::Shadow::NONE;
        visuals.popup_shadow = egui::Shadow::NONE;
        visuals.faint_bg_color = light;
        visuals.extreme_bg_color = lightest;
        visuals.code_bg_color = light;
        visuals.selection.bg_fill = light;
        visuals.selection.stroke = Stroke::new(self.px(1.0), darkest);
        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
            &mut visuals.widgets.open,
        ] {
            widget.bg_fill = lightest;
            widget.weak_bg_fill = lightest;
            widget.bg_stroke = Stroke::new(self.px(1.0), dark);
            widget.fg_stroke = Stroke::new(self.px(1.0), darkest);
            widget.rounding = Rounding::ZERO;
            widget.expansion = 0.0;
        }
        visuals.widgets.hovered.weak_bg_fill = light;
        visuals.widgets.active.weak_bg_fill = light;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_pixel_is_a_whole_number_of_physical_pixels() {
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            for size in [
                Vec2::new(640.0, 576.0),
                Vec2::new(1920.0, 1080.0),
                Vec2::new(801.0, 603.0),
            ] {
                let physical = Theme::dmg(size, ppp).pixel * ppp;
                assert!((physical - physical.round()).abs() < 1e-4, "{physical}");
            }
        }
    }

    #[test]
    fn scale_follows_the_tighter_dimension_and_never_drops_below_one() {
        assert_eq!(Theme::dmg(Vec2::new(640.0, 576.0), 1.0).pixel, 2.0);
        assert_eq!(Theme::dmg(Vec2::new(1920.0, 1080.0), 1.0).pixel, 3.0);
        assert_eq!(Theme::dmg(Vec2::new(100.0, 100.0), 1.0).pixel, 1.0);
    }
}

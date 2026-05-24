//! Small icon buttons. Egui's default font ships without many decorative
//! glyphs, so trying to render something like "✕" as a delete affordance
//! falls back to a missing-glyph box on systems whose installed fonts don't
//! cover it. To keep the UI portable we embed a tiny SVG icon and render
//! it through `egui_extras`' SVG loader.

use egui::{Button, Image, Response, Ui, include_image, vec2};

const ICON_SIZE: f32 = 12.0;

fn trash_image(ui: &Ui) -> Image<'static> {
    let tint = ui.visuals().widgets.inactive.fg_stroke.color;
    Image::new(include_image!("../../../assets/icons/trash.svg"))
        .tint(tint)
        .fit_to_exact_size(vec2(ICON_SIZE, ICON_SIZE))
}

/// Icon-only delete button. Drop-in replacement for `ui.small_button("✕")`.
pub fn trash_button(ui: &mut Ui) -> Response {
    ui.add(Button::image(trash_image(ui)).small())
}

/// Delete button with a trailing text label (e.g. "remove"). Drop-in
/// replacement for `ui.small_button("✕ remove")`.
pub fn trash_button_with_label(ui: &mut Ui, label: &str) -> Response {
    ui.add(Button::image_and_text(trash_image(ui), label).small())
}

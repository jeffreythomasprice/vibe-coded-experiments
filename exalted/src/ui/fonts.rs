//! Install DejaVu Sans / Sans Mono as low-priority fallbacks for egui's
//! bundled fonts. Egui's defaults (Ubuntu-Light + Hack) miss many symbol
//! glyphs we use in labels — e.g. `→` (U+2192) — and render them as the
//! missing-glyph box. DejaVu has broad BMP coverage and only kicks in for
//! glyphs the primary fonts don't supply, so normal text keeps the egui
//! look.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "DejaVuSans".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/DejaVuSans.ttf"
        ))),
    );
    fonts.font_data.insert(
        "DejaVuSansMono".to_owned(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/DejaVuSansMono.ttf"
        ))),
    );

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("DejaVuSans".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .push("DejaVuSansMono".to_owned());

    ctx.set_fonts(fonts);
}

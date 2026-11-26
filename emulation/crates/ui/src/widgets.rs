//! Custom-painted pieces of the old-school menu: the double-bordered dialog box,
//! the selection cursor, and single-line text.

use egui::{Align2, Id, Mesh, Painter, Pos2, Rect, Rounding, Sense, Vec2};

use crate::theme::Theme;

/// Space between a dialog's outer edge and its content, in UI pixels.
pub(crate) const DIALOG_PADDING: f32 = 8.0;
/// Height of one line of a menu, in UI pixels.
pub(crate) const ROW_HEIGHT: f32 = 12.0;
/// Horizontal space reserved to the left of an item for the cursor, in UI pixels.
pub(crate) const CURSOR_WIDTH: f32 = 8.0;

/// A clickable region that never takes egui keyboard focus: the menu has its own
/// cursor, and a focused widget would also react to Enter/Space.
const CLICK: Sense = Sense {
    click: true,
    drag: false,
    focusable: false,
};

/// Show a dialog box occupying exactly `rect`, painting `contents` inside it. The
/// closure receives the area inside the padding.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dialog_box(
    ctx: &egui::Context,
    theme: &Theme,
    id: Id,
    order: egui::Order,
    interactable: bool,
    rect: Rect,
    opacity: f32,
    contents: impl FnOnce(&mut egui::Ui, Rect),
) {
    egui::Area::new(id)
        .order(order)
        .fixed_pos(rect.min)
        .interactable(interactable)
        .show(ctx, |ui| {
            let sense = if interactable {
                Sense::hover()
            } else {
                Sense {
                    click: false,
                    drag: false,
                    focusable: false,
                }
            };
            ui.allocate_rect(rect, sense);
            ui.set_opacity(opacity);
            paint_border(ui.painter(), theme, rect);
            contents(ui, rect.shrink(theme.px(DIALOG_PADDING)));
        });
}

/// A thick outer line and a thin inner one over a light fill, with the corner
/// pixels notched out to fake rounding.
fn paint_border(painter: &Painter, theme: &Theme, rect: Rect) {
    let colors = theme.palette;
    let shades = [
        colors.darkest,
        colors.lightest,
        colors.dark,
        colors.lightest,
    ];
    let size = rect.size() / theme.px(1.0);
    // A mesh has no antialiased fringe, so neighbouring pieces neither overlap
    // nor leave seams.
    let mut mesh = Mesh::default();
    for (piece, layer) in border_pieces(size) {
        let piece = Rect::from_min_max(
            rect.min + piece.min.to_vec2() * theme.px(1.0),
            rect.min + piece.max.to_vec2() * theme.px(1.0),
        );
        mesh.add_colored_rect(piece, shades[layer]);
    }
    painter.add(mesh);
}

/// Where each of the border's nested layers starts, in UI pixels from the edge.
const LAYER_INSETS: [u8; 4] = [0, 2, 3, 4];
/// Depth from the top or bottom edge past which no layer is notched any more.
const NOTCHED_ROWS: u8 = 6;

/// Cut a dialog box of `size` UI pixels into rectangles that don't overlap, each
/// tagged with the layer visible there. Painting the layers on top of each other
/// instead would make a translucent box far more opaque than asked.
fn border_pieces(size: Vec2) -> Vec<(Rect, usize)> {
    let edge = f32::from(NOTCHED_ROWS);
    // Horizontal bands, with the depth of each from its nearer edge.
    let mut bands: Vec<(f32, f32, u8)> = vec![(edge, size.y - edge, NOTCHED_ROWS)];
    for depth in 0..NOTCHED_ROWS {
        let top = f32::from(depth);
        bands.push((top, top + 1.0, depth));
        bands.push((size.y - top - 1.0, size.y - top, depth));
    }

    let mut pieces = Vec::new();
    for (top, bottom, depth) in bands {
        // How far in from the sides each layer present in this band starts: its
        // corners are cut by two pixels in its first row and one in its second.
        let starts: Vec<f32> = LAYER_INSETS
            .iter()
            .filter(|&&inset| inset <= depth)
            .map(|&inset| f32::from(inset + 2u8.saturating_sub(depth - inset)))
            .collect();
        let mut push = |left: f32, right: f32, layer: usize| {
            if right > left {
                let rect = Rect::from_min_max(Pos2::new(left, top), Pos2::new(right, bottom));
                pieces.push((rect, layer));
            }
        };
        for (layer, &start) in starts.iter().enumerate() {
            match starts.get(layer + 1) {
                Some(&next) => {
                    push(start, next, layer);
                    push(size.x - next, size.x - start, layer);
                }
                None => push(start, size.x - start, layer),
            }
        }
    }
    pieces
}

/// Paint `text` on one line with its top-left at `pos`, cut short with `..` if it
/// is wider than `max_width`.
pub(crate) fn text(painter: &Painter, theme: &Theme, pos: Pos2, max_width: f32, text: &str) {
    let shown = truncate(text, max_width, |s| text_width(painter.ctx(), theme, s));
    painter.text(
        pos + Vec2::new(0.0, theme.px(1.0)),
        Align2::LEFT_TOP,
        shown,
        theme.font(),
        theme.palette.darkest,
    );
}

pub(crate) fn text_width(ctx: &egui::Context, theme: &Theme, text: &str) -> f32 {
    ctx.fonts(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), theme.font(), theme.palette.darkest)
            .size()
            .x
    })
}

fn truncate(text: &str, max_width: f32, width: impl Fn(&str) -> f32) -> String {
    if width(text) <= max_width {
        return text.to_owned();
    }
    let mut kept = text.to_owned();
    while kept.pop().is_some() {
        let candidate = format!("{kept}..");
        if width(&candidate) <= max_width {
            return candidate;
        }
    }
    String::new()
}

/// One selectable entry filling `rect`: the cursor (when selected) followed by
/// the label.
pub(crate) fn menu_item(
    ui: &mut egui::Ui,
    theme: &Theme,
    id: Id,
    rect: Rect,
    label: &str,
    selected: bool,
) -> egui::Response {
    let response = ui.interact(rect, id, CLICK);
    if selected {
        cursor(ui.painter(), theme, rect.min);
    }
    let indent = theme.px(CURSOR_WIDTH);
    text(
        ui.painter(),
        theme,
        rect.min + Vec2::new(indent, 0.0),
        rect.width() - indent,
        label,
    );
    response
}

/// The right-pointing selection triangle, built from pixel columns because the
/// font has no glyph for it.
fn cursor(painter: &Painter, theme: &Theme, row_min: Pos2) {
    for column in 0..4 {
        let column = column as f32;
        let min = row_min + Vec2::new(theme.px(1.0 + column), theme.px(2.0 + column));
        let size = Vec2::new(theme.px(1.0), theme.px(7.0 - 2.0 * column));
        painter.rect_filled(
            Rect::from_min_size(min, size),
            Rounding::ZERO,
            theme.palette.darkest,
        );
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Arrow {
    Up,
    Down,
}

/// The underline marking where typed text goes, for a line whose top-left would
/// be `pos`.
pub(crate) fn text_caret(painter: &Painter, theme: &Theme, pos: Pos2) {
    let min = pos + Vec2::new(0.0, theme.px(9.0));
    let size = Vec2::new(theme.px(6.0), theme.px(1.0));
    painter.rect_filled(
        Rect::from_min_size(min, size),
        Rounding::ZERO,
        theme.palette.darkest,
    );
}

/// A small triangle, its top-left at `pos`, hinting that a list continues.
pub(crate) fn scroll_arrow(painter: &Painter, theme: &Theme, pos: Pos2, arrow: Arrow) {
    for line in 0..4 {
        let inset = match arrow {
            Arrow::Up => 3 - line,
            Arrow::Down => line,
        } as f32;
        let min = pos + Vec2::new(theme.px(inset), theme.px(line as f32));
        let size = Vec2::new(theme.px(7.0 - 2.0 * inset), theme.px(1.0));
        painter.rect_filled(
            Rect::from_min_size(min, size),
            Rounding::ZERO,
            theme.palette.darkest,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_pieces_match_the_layered_shapes_without_overlapping() {
        let size = Vec2::new(30.0, 20.0);
        let pieces = border_pieces(size);
        // The topmost nested, notched layer covering a pixel, if any.
        let expected = |x: f32, y: f32| {
            LAYER_INSETS.iter().rposition(|&inset| {
                let inset = f32::from(inset);
                let dx = x.min(size.x - x) - inset;
                let dy = y.min(size.y - y) - inset;
                dx > 0.0 && dy > 0.0 && dx + dy > 2.0
            })
        };
        for column in 0..30 {
            for row in 0..20 {
                let pixel = Pos2::new(column as f32 + 0.5, row as f32 + 0.5);
                let layers: Vec<usize> = pieces
                    .iter()
                    .filter(|(rect, _)| rect.contains(pixel))
                    .map(|&(_, layer)| layer)
                    .collect();
                let expected: Vec<usize> = expected(pixel.x, pixel.y).into_iter().collect();
                assert_eq!(layers, expected, "pixel {pixel:?}");
            }
        }
    }

    #[test]
    fn truncate_keeps_short_text_and_marks_cut_text() {
        let width = |s: &str| s.chars().count() as f32;
        assert_eq!(truncate("tetris", 6.0, width), "tetris");
        assert_eq!(truncate("tetris", 5.0, width), "tet..");
        assert_eq!(truncate("tetris", 1.0, width), "");
    }
}

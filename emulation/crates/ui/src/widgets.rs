//! Custom-painted pieces of the old-school menu: the double-bordered dialog box,
//! the selection cursor, and single-line text.

use egui::{Align2, Color32, Id, Painter, Pos2, Rect, Rounding, Sense, Vec2};

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
pub(crate) fn dialog_box(
    ctx: &egui::Context,
    theme: &Theme,
    id: Id,
    order: egui::Order,
    interactable: bool,
    rect: Rect,
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
            paint_border(ui.painter(), theme, rect);
            contents(ui, rect.shrink(theme.px(DIALOG_PADDING)));
        });
}

/// A thick outer line and a thin inner one over a light fill, with the corner
/// pixels notched out to fake rounding.
fn paint_border(painter: &Painter, theme: &Theme, rect: Rect) {
    let colors = theme.palette;
    let notched = |inset: f32, color: Color32| {
        let r = rect.shrink(theme.px(inset));
        let notch = theme.px(2.0);
        painter.rect_filled(r.shrink2(Vec2::new(notch, 0.0)), Rounding::ZERO, color);
        painter.rect_filled(r.shrink2(Vec2::new(0.0, notch)), Rounding::ZERO, color);
        painter.rect_filled(r.shrink(theme.px(1.0)), Rounding::ZERO, color);
    };
    notched(0.0, colors.darkest);
    notched(2.0, colors.lightest);
    notched(3.0, colors.dark);
    notched(4.0, colors.lightest);
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
    fn truncate_keeps_short_text_and_marks_cut_text() {
        let width = |s: &str| s.chars().count() as f32;
        assert_eq!(truncate("tetris", 6.0, width), "tetris");
        assert_eq!(truncate("tetris", 5.0, width), "tet..");
        assert_eq!(truncate("tetris", 1.0, width), "");
    }
}

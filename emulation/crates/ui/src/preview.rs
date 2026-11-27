//! The picture of the highlighted save on the load screen.

use std::sync::Arc;

use common::Screenshot;
use egui::{Color32, ColorImage, Id, Order, Pos2, Rect, TextureHandle, TextureOptions, Vec2};

use crate::menu::{Entry, Screen};
use crate::screens::{Layout, MARGIN};
use crate::theme::Theme;
use crate::widgets::{self, DIALOG_PADDING, ROW_HEIGHT};
use crate::{MenuData, StateChoice};

/// The picture size the box is built around: one Game Boy screen. Pictures of
/// other sizes are fitted inside it.
const PICTURE_SIZE: Vec2 = Vec2::new(160.0, 144.0);
/// The picture shrinks until the list below it can show this many saves.
const MIN_LIST_ENTRIES: usize = 5;
/// On two lines, to fit the smallest box.
const NO_PICTURE: [&str; 2] = ["No", "picture"];

/// Where the picture box goes relative to the list.
#[derive(Clone, Copy)]
enum Side {
    Right,
    Above,
}

#[derive(Clone, Copy)]
pub(crate) struct Placement {
    side: Side,
    size: Vec2,
    /// The area the list and the picture share.
    space: Rect,
}

impl Placement {
    /// The picture box, next to the list dialog at `list`.
    pub(crate) fn rect(&self, theme: &Theme, list: Rect) -> Rect {
        let gap = theme.px(MARGIN);
        let min = match self.side {
            Side::Right => Pos2::new(
                list.max.x + gap,
                list.min.y.min(self.space.max.y - self.size.y),
            ),
            Side::Above => Pos2::new(
                self.space.min.x + theme.snap(((self.space.width() - self.size.x) / 2.0).max(0.0)),
                list.min.y - gap - self.size.y,
            ),
        };
        Rect::from_min_size(min, self.size)
    }
}

/// The size of a picture box whose picture is magnified `scale` times, in
/// physical pixels, so that every picture pixel stays the same size.
fn box_size(ctx: &egui::Context, theme: &Theme, scale: f32) -> Vec2 {
    let picture = PICTURE_SIZE * scale / ctx.pixels_per_point();
    // Round up to the UI grid so the picture is never clipped.
    let grid = |length: f32| theme.px((length / theme.px(1.0) - 1e-3).ceil());
    Vec2::new(grid(picture.x), grid(picture.y)) + Vec2::splat(theme.px(2.0 * DIALOG_PADDING))
}

/// Reserve room for the picture box, shrinking `space` to what is left for the
/// list. Returns `None`, leaving `space` alone, when even the smallest picture
/// would crowd out the list.
pub(crate) fn place(
    ctx: &egui::Context,
    theme: &Theme,
    data: &MenuData,
    rows: &[Vec<Entry>],
    space: &mut Rect,
) -> Option<Placement> {
    let full = *space;
    let gap = theme.px(MARGIN);
    // At most one picture pixel per UI pixel.
    let largest = (theme.px(1.0) * ctx.pixels_per_point()).round().max(1.0) as u32;
    let layout = |space: Rect| Layout::new(ctx, theme, Screen::LoadState, data, rows, space);

    let size = box_size(ctx, theme, largest as f32);
    if layout(full).rect.width() + gap + size.x <= full.width() && size.y <= full.height() {
        space.max.x -= size.x + gap;
        return Some(Placement {
            side: Side::Right,
            size,
            space: full,
        });
    }

    let wanted = data.states.len().min(MIN_LIST_ENTRIES);
    for scale in (1..=largest).rev() {
        let size = box_size(ctx, theme, scale as f32);
        let mut below = full;
        below.min.y += size.y + gap;
        let list = layout(below);
        let fits = size.x <= full.width() && list.rect.height() <= below.height();
        // The smallest picture will do with a shorter list.
        if fits && (list.visible >= wanted || scale == 1) {
            *space = below;
            return Some(Placement {
                side: Side::Above,
                size,
                space: full,
            });
        }
    }
    None
}

/// The uploaded picture, kept until another one is shown.
#[derive(Clone)]
struct Uploaded {
    screenshot: Arc<Screenshot>,
    texture: TextureHandle,
}

fn texture(ctx: &egui::Context, screenshot: &Arc<Screenshot>) -> TextureHandle {
    let id = Id::new("menu-preview-texture");
    let cached = ctx.data(|d| d.get_temp::<Uploaded>(id));
    if let Some(uploaded) = cached {
        // Holding the `Arc` keeps its address from being reused.
        if Arc::ptr_eq(&uploaded.screenshot, screenshot) {
            return uploaded.texture;
        }
    }
    let size = [screenshot.width() as usize, screenshot.height() as usize];
    let image = ColorImage::from_rgba_unmultiplied(size, screenshot.rgba());
    // Loaded outside `ctx.data_mut`, which would deadlock.
    let texture = ctx.load_texture("save-preview", image, TextureOptions::NEAREST);
    let uploaded = Uploaded {
        screenshot: screenshot.clone(),
        texture: texture.clone(),
    };
    ctx.data_mut(|d| d.insert_temp(id, uploaded));
    texture
}

pub(crate) fn draw(ctx: &egui::Context, theme: &Theme, rect: Rect, choice: &StateChoice) {
    let texture = choice
        .screenshot
        .as_ref()
        .map(|screenshot| texture(ctx, screenshot));
    widgets::dialog_box(
        ctx,
        theme,
        Id::new("menu-preview"),
        Order::Foreground,
        true,
        rect,
        1.0,
        |ui, inner| match &texture {
            Some(texture) => {
                let actual = texture.size_vec2();
                let fit = (inner.width() / actual.x).min(inner.height() / actual.y);
                // Whole physical pixels per picture pixel, when it is no
                // smaller than that.
                let physical = fit * ctx.pixels_per_point();
                let scale = if physical >= 1.0 {
                    (physical + 1e-3).floor() / ctx.pixels_per_point()
                } else {
                    fit
                };
                let size = actual * scale;
                let offset = (inner.size() - size) / 2.0;
                let min = inner.min + Vec2::new(theme.snap(offset.x), theme.snap(offset.y));
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                ui.painter().image(
                    texture.id(),
                    Rect::from_min_size(min, size),
                    uv,
                    Color32::WHITE,
                );
            }
            None => {
                let line_height = theme.px(ROW_HEIGHT);
                let top = (inner.height() - line_height * NO_PICTURE.len() as f32) / 2.0;
                for (index, line) in NO_PICTURE.iter().enumerate() {
                    let left = (inner.width() - widgets::text_width(ctx, theme, line)) / 2.0;
                    let pos = inner.min
                        + Vec2::new(
                            theme.snap(left.max(0.0)),
                            theme.snap(top.max(0.0)) + line_height * index as f32,
                        );
                    widgets::text(ui.painter(), theme, pos, inner.width(), line);
                }
            }
        },
    );
}
